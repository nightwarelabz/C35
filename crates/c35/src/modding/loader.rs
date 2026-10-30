//! Loading mods: folders in, a deterministic world-addendum out.
//!
//! The loader enforces the spec's §4, in order:
//!
//! 1. A mod is a folder of text files. Image files anywhere in it
//!    refuse the whole mod ("image imports are not supported;
//!    define the surface as a painter recipe"). `tuning.json` is a
//!    v2 file and is refused in v1. Unknown files are errors —
//!    strictness is the sandbox.
//! 2. `mod.json` is read first and its `engine` range is checked
//!    BEFORE any other file in the mod is read. A mod built for
//!    another engine range is refused, not loaded hopefully. A mod
//!    declaring no range is refused.
//! 3. Every prop file is schema-checked: ids, namespace ownership,
//!    closed vocabularies (primitives, materials, tags, collision,
//!    footstep), parameter declarations and ranges, and the budget
//!    — declared triangles must equal the triangles the recipe
//!    actually emits, exactly, and declared atlas share must be 0
//!    (v1 reuses shipped regions; there is nothing else to declare).
//! 4. Placements are schema-checked and their variant overrides
//!    range-checked. Anchor resolution happens at multi-mod load
//!    against a host-provided anchor table; an unknown anchor drops
//!    that placement and reports it — it never quarantines the mod,
//!    because the anchor set belongs to the city, not the mod.
//! 5. Multi-mod load is deterministic: base first, `load_after`
//!    topology, ties broken by folder name. A `load_after` target
//!    that is missing or quarantined quarantines the dependent.
//!    A cycle quarantines its members. Conflicts are reported and
//!    the later mod wins — never silent, never merged.
//! 6. A quarantined mod contributes nothing: no props, no
//!    placements, no replacements. The game always boots.
//!
//! v1 limitation, stated so nobody trips on it: a placement may
//! reference a shipped prop or a prop from the SAME mod only.
//! Cross-mod placement references would make single-folder
//! validation (`check_mod`) disagree with multi-mod load, and the
//! two must never disagree. Cross-mod references are a future
//! extension, not a silent behaviour.

use crate::modding::json::{self, Kind, Value};
use crate::modding::model::*;
use crate::modding::registry::{resolve_defaults, resolve_recipe, Registry};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The engine version mods declare compatibility against. This is
/// the crate's own version — the C35 version the game ships with.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Image extensions refused anywhere in a mod folder (matched
/// case-insensitively, on the file name's extension).
const IMAGE_EXTS: [&str; 12] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tga", "dds", "exr", "hdr", "svg", "ico",
];

/// Fields that claim a v2/v3 power. They are refused with a plain
/// reason naming the stage, not a generic "unknown field".
const REFUSED_FIELDS: [(&str, &str); 8] = [
    ("script", "v3 (scripting)"),
    ("code", "v3 (scripting)"),
    ("missions", "v3 (missions)"),
    ("dialogue", "v3 (dialogue)"),
    ("ai", "v2/v3 (AI behaviour)"),
    ("behavior", "v2/v3 (AI behaviour)"),
    ("behaviour", "v2/v3 (AI behaviour)"),
    ("enemies", "v3 (new enemy types)"),
];

/// One error, located: `file:line: message`.
fn at(file: &str, value: &Value, message: impl Into<String>) -> String {
    format!("{}:{}: {}", file, value.line, message.into())
}

// ---------------------------------------------------------------------
// Schema helpers: typed extraction with located errors.
// ---------------------------------------------------------------------

fn expect_obj<'a>(file: &str, v: &'a Value, what: &str) -> Result<&'a [(String, Value)], String> {
    v.as_obj()
        .ok_or_else(|| at(file, v, format!("{what} must be an object, got {}", v.type_name())))
}

fn expect_arr<'a>(file: &str, v: &'a Value, what: &str) -> Result<&'a [Value], String> {
    v.as_arr()
        .ok_or_else(|| at(file, v, format!("{what} must be an array, got {}", v.type_name())))
}

fn expect_str<'a>(file: &str, v: &'a Value, what: &str) -> Result<&'a str, String> {
    v.as_str()
        .ok_or_else(|| at(file, v, format!("{what} must be a string, got {}", v.type_name())))
}

fn expect_num(file: &str, v: &Value, what: &str) -> Result<f64, String> {
    v.as_num()
        .ok_or_else(|| at(file, v, format!("{what} must be a number, got {}", v.type_name())))
}

fn expect_bool(file: &str, v: &Value, what: &str) -> Result<bool, String> {
    v.as_bool()
        .ok_or_else(|| at(file, v, format!("{what} must be a boolean, got {}", v.type_name())))
}

fn check_fields(file: &str, v: &Value, allowed: &[&str], what: &str) -> Result<(), String> {
    let entries = expect_obj(file, v, what)?;
    for (k, fv) in entries {
        if !allowed.contains(&k.as_str()) {
            if let Some((_, stage)) = REFUSED_FIELDS.iter().find(|(f, _)| f == k) {
                return Err(at(
                    file,
                    fv,
                    format!("field '{k}' claims a {stage} power; refused in v1"),
                ));
            }
            return Err(at(file, fv, format!("unknown field '{k}' in {what}")));
        }
    }
    Ok(())
}

fn required<'a>(file: &str, v: &'a Value, name: &str, what: &str) -> Result<&'a Value, String> {
    v.field(name)
        .ok_or_else(|| at(file, v, format!("{what} is missing required field '{name}'")))
}

fn parse_id_field(file: &str, v: &Value, what: &str) -> Result<String, String> {
    let s = expect_str(file, v, what)?;
    if !valid_id(s) {
        return Err(at(
            file,
            v,
            format!("'{s}' is not a valid namespaced id (namespace:name, lowercase, one colon)"),
        ));
    }
    Ok(s.to_string())
}

fn parse_num_array(
    file: &str,
    v: &Value,
    n: usize,
    what: &str,
) -> Result<Vec<f64>, String> {
    let items = expect_arr(file, v, what)?;
    if items.len() != n {
        return Err(at(
            file,
            v,
            format!("{what} must have exactly {n} numbers, got {}", items.len()),
        ));
    }
    let mut out = Vec::new();
    for item in items {
        let num = expect_num(file, item, what)?;
        if !num.is_finite() {
            return Err(at(file, item, format!("{what} must be finite")));
        }
        out.push(num);
    }
    Ok(out)
}

/// A number in a recipe, or a `$param` reference.
fn parse_num_or_param(
    file: &str,
    v: &Value,
    what: &str,
) -> Result<NumOrParam, String> {
    match &v.kind {
        Kind::Num(n) => {
            if n.is_finite() {
                Ok(NumOrParam::Num(*n as f32))
            } else {
                Err(at(file, v, format!("{what} must be finite")))
            }
        }
        Kind::Str(s) => {
            let name = s.strip_prefix('$').ok_or_else(|| {
                at(file, v, format!("{what} must be a number or a '$param' reference"))
            })?;
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(at(
                    file,
                    v,
                    format!("'{s}' is not a valid '$param' reference"),
                ));
            }
            Ok(NumOrParam::Param(name.to_string()))
        }
        _ => Err(at(
            file,
            v,
            format!("{what} must be a number or a '$param' reference, got {}", v.type_name()),
        )),
    }
}

fn parse_num_or_param3(
    file: &str,
    v: &Value,
    what: &str,
) -> Result<[NumOrParam; 3], String> {
    let items = expect_arr(file, v, what)?;
    if items.len() != 3 {
        return Err(at(
            file,
            v,
            format!("{what} must have exactly 3 entries, got {}", items.len()),
        ));
    }
    Ok([
        parse_num_or_param(file, &items[0], what)?,
        parse_num_or_param(file, &items[1], what)?,
        parse_num_or_param(file, &items[2], what)?,
    ])
}

// ---------------------------------------------------------------------
// Engine version ranges: ">=0.1 <0.2", "=0.1.0", "0.1.0".
// ---------------------------------------------------------------------

type Version = (u64, u64, u64);

fn parse_version(s: &str) -> Option<Version> {
    let mut parts = s.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    let patch: u64 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn version_satisfies(range: &str, current: Version) -> Result<bool, String> {
    if range.trim().is_empty() {
        return Err("engine range is empty".to_string());
    }
    for token in range.split(|c| c == ' ' || c == ',').filter(|t| !t.is_empty()) {
        let (op, ver) = if let Some(rest) = token.strip_prefix(">=") {
            (">=", rest)
        } else if let Some(rest) = token.strip_prefix("<=") {
            ("<=", rest)
        } else if let Some(rest) = token.strip_prefix('>') {
            (">", rest)
        } else if let Some(rest) = token.strip_prefix('<') {
            ("<", rest)
        } else if let Some(rest) = token.strip_prefix('=') {
            ("=", rest)
        } else {
            ("=", token)
        };
        let want = parse_version(ver)
            .ok_or_else(|| format!("engine range token '{token}' is not a valid version"))?;
        let ok = match op {
            ">=" => current >= want,
            "<=" => current <= want,
            ">" => current > want,
            "<" => current < want,
            "=" => current == want,
            _ => unreachable!(),
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

fn current_version() -> Version {
    parse_version(ENGINE_VERSION).unwrap_or((0, 0, 0))
}

// ---------------------------------------------------------------------
// Manifest, prop files, placement files.
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModManifest {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub engine_range: String,
    pub load_after: Vec<String>,
    pub description: Option<String>,
}

fn parse_manifest(file: &str, v: &Value) -> Result<ModManifest, String> {
    check_fields(
        file,
        v,
        &["id", "name", "author", "version", "engine", "load_after", "description"],
        "manifest",
    )?;
    let id_v = required(file, v, "id", "manifest")?;
    let id = expect_str(file, id_v, "manifest id")?;
    // The manifest id IS the namespace (e.g. `my-street`): it is
    // what this mod's prop ids must start with, and what other
    // mods name in `load_after`.
    if !valid_namespace(id) {
        return Err(at(
            file,
            id_v,
            format!("manifest id '{id}' is not a valid namespace (lowercase, digits, '_' and '-', starting with a letter or digit)"),
        ));
    }
    let namespace = id;
    if RESERVED_NAMESPACES.contains(&namespace) {
        return Err(at(
            file,
            id_v,
            format!("namespace '{namespace}' belongs to the engine; a mod may not claim it"),
        ));
    }
    let name = expect_str(file, required(file, v, "name", "manifest")?, "manifest name")?;
    if name.is_empty() {
        return Err(at(file, v, "manifest name must not be empty"));
    }
    let author = expect_str(file, required(file, v, "author", "manifest")?, "manifest author")?;
    if author.is_empty() {
        return Err(at(file, v, "manifest author must not be empty"));
    }
    let version = expect_str(file, required(file, v, "version", "manifest")?, "manifest version")?;
    if version.is_empty() {
        return Err(at(file, v, "manifest version must not be empty"));
    }
    let engine_range = expect_str(
        file,
        required(file, v, "engine", "manifest")?,
        "manifest engine range",
    )?;
    if engine_range.trim().is_empty() {
        return Err(at(
            file,
            required(file, v, "engine", "manifest")?,
            "manifest engine range must not be empty (a mod with no range is refused)",
        ));
    }
    // Range syntax is checked here; satisfaction is checked by the
    // caller before any other file is read.
    if let Err(e) = version_satisfies(engine_range, current_version()) {
        let engine_field = required(file, v, "engine", "manifest")?;
        return Err(at(file, engine_field, e));
    }
    let mut load_after = Vec::new();
    if let Some(la) = v.field("load_after") {
        for item in expect_arr(file, la, "load_after")? {
            let target = expect_str(file, item, "load_after entry")?;
            if !valid_namespace(target) {
                return Err(at(
                    file,
                    item,
                    format!("load_after entry '{target}' is not a valid mod id (a namespace)"),
                ));
            }
            load_after.push(target.to_string());
        }
    }
    let description = match v.field("description") {
        Some(d) => Some(expect_str(file, d, "manifest description")?.to_string()),
        None => None,
    };
    Ok(ModManifest {
        id: id.to_string(),
        name: name.to_string(),
        author: author.to_string(),
        version: version.to_string(),
        engine_range: engine_range.to_string(),
        load_after,
        description,
    })
}

fn parse_param_defs(
    file: &str,
    v: &Value,
) -> Result<BTreeMap<String, ParamDef>, String> {
    let entries = expect_obj(file, v, "parameters")?;
    let mut out = BTreeMap::new();
    for (name, def_v) in entries {
        check_fields(file, def_v, &["default", "min", "max"], "parameter definition")?;
        let default_v = required(file, def_v, "default", "parameter definition")?;
        let def = match &default_v.kind {
            Kind::Num(n) => {
                if !n.is_finite() {
                    return Err(at(file, default_v, "parameter default must be finite"));
                }
                let mut min = None;
                let mut max = None;
                if let Some(m) = def_v.field("min") {
                    min = Some(expect_num(file, m, "parameter min")? as f32);
                }
                if let Some(m) = def_v.field("max") {
                    max = Some(expect_num(file, m, "parameter max")? as f32);
                }
                if let (Some(lo), Some(hi)) = (min, max) {
                    if lo > hi {
                        return Err(at(
                            file,
                            def_v,
                            format!("parameter '{name}': min {lo} exceeds max {hi}"),
                        ));
                    }
                }
                let d = ParamDef {
                    default: ParamValue::Num(*n as f32),
                    min,
                    max,
                };
                d.check(&ParamValue::Num(*n as f32)).map_err(|e| {
                    at(file, default_v, format!("parameter '{name}': {e}"))
                })?;
                d
            }
            Kind::Bool(b) => {
                if def_v.field("min").is_some() || def_v.field("max").is_some() {
                    return Err(at(
                        file,
                        def_v,
                        format!("parameter '{name}': boolean parameters take no min/max"),
                    ));
                }
                ParamDef::flag(*b)
            }
            _ => {
                return Err(at(
                    file,
                    default_v,
                    format!(
                        "parameter '{name}': default must be a number or a boolean, got {}",
                        default_v.type_name()
                    ),
                ))
            }
        };
        out.insert(name.clone(), def);
    }
    Ok(out)
}

fn parse_placement_contract(
    file: &str,
    v: &Value,
) -> Result<PlacementContract, String> {
    check_fields(
        file,
        v,
        &["footprint", "height", "collision", "grapple_anchor", "occluder", "footstep"],
        "placement",
    )?;
    let fp = parse_num_array(
        file,
        required(file, v, "footprint", "placement")?,
        2,
        "placement footprint",
    )?;
    if fp[0] <= 0.0 || fp[1] <= 0.0 {
        return Err(at(
            file,
            required(file, v, "footprint", "placement")?,
            "placement footprint dimensions must be positive",
        ));
    }
    let height = expect_num(file, required(file, v, "height", "placement")?, "placement height")?;
    if height <= 0.0 {
        return Err(at(
            file,
            required(file, v, "height", "placement")?,
            "placement height must be positive",
        ));
    }
    let collision_s = expect_str(
        file,
        required(file, v, "collision", "placement")?,
        "placement collision",
    )?;
    let collision_field = required(file, v, "collision", "placement")?;
    let collision = CollisionClass::from_name(collision_s).ok_or_else(|| {
        at(
            file,
            collision_field,
            format!("unknown collision class '{collision_s}' (solid, pass-through, climbable)"),
        )
    })?;
    let grapple_anchor = expect_bool(
        file,
        required(file, v, "grapple_anchor", "placement")?,
        "placement grapple_anchor",
    )?;
    let occluder = expect_bool(
        file,
        required(file, v, "occluder", "placement")?,
        "placement occluder",
    )?;
    let footstep_s = expect_str(
        file,
        required(file, v, "footstep", "placement")?,
        "placement footstep",
    )?;
    let footstep_field = required(file, v, "footstep", "placement")?;
    let footstep = Footstep::from_name(footstep_s).ok_or_else(|| {
        at(
            file,
            footstep_field,
            format!("unknown footstep surface '{footstep_s}' (wood, metal, concrete, wet-stone, none)"),
        )
    })?;
    Ok(PlacementContract {
        footprint: [fp[0] as f32, fp[1] as f32],
        height: height as f32,
        collision,
        grapple_anchor,
        occluder,
        footstep,
    })
}

fn parse_prop_file(
    file: &str,
    v: &Value,
    namespace: &str,
) -> Result<(PropDef, Vec<String>), String> {
    let mut warnings = Vec::new();
    check_fields(
        file,
        v,
        &["id", "parts", "parameters", "placement", "tags", "budget"],
        "prop file",
    )?;
    let id = parse_id_field(file, required(file, v, "id", "prop file")?, "prop id")?;
    let (ns, _) = split_id(&id).unwrap();
    if ns != namespace {
        return Err(at(
            file,
            required(file, v, "id", "prop file")?,
            format!("prop '{id}' claims namespace '{ns}', but the manifest declares '{namespace}'"),
        ));
    }
    let parts_v = expect_arr(file, required(file, v, "parts", "prop file")?, "parts")?;
    if parts_v.is_empty() {
        return Err(at(
            file,
            required(file, v, "parts", "prop file")?,
            "prop must declare at least one part",
        ));
    }
    let mut params = BTreeMap::new();
    if let Some(p) = v.field("parameters") {
        params = parse_param_defs(file, p)?;
    }
    let mut recipe: Vec<PartRecipe> = Vec::new();
    for part_v in parts_v {
        check_fields(
            file,
            part_v,
            &["primitive", "size", "position", "rotation", "material"],
            "part",
        )?;
        let prim_s = expect_str(
            file,
            required(file, part_v, "primitive", "part")?,
            "part primitive",
        )?;
        let primitive_field = required(file, part_v, "primitive", "part")?;
        let primitive = Primitive::from_name(prim_s).ok_or_else(|| {
            at(
                file,
                primitive_field,
                format!("unknown primitive '{prim_s}' (the frozen vocabulary is: {})", PRIMITIVES.join(", ")),
            )
        })?;
        let size = parse_num_or_param3(
            file,
            required(file, part_v, "size", "part")?,
            "part size",
        )?;
        let position = parse_num_or_param3(
            file,
            required(file, part_v, "position", "part")?,
            "part position",
        )?;
        let rotation = match part_v.field("rotation") {
            Some(r) => parse_num_or_param3(file, r, "part rotation")?,
            None => [
                NumOrParam::Num(0.0),
                NumOrParam::Num(0.0),
                NumOrParam::Num(0.0),
            ],
        };
        let material = expect_str(
            file,
            required(file, part_v, "material", "part")?,
            "part material",
        )?;
        if material_region(material).is_none() {
            return Err(at(
                file,
                required(file, part_v, "material", "part")?,
                format!("unknown material '{material}' (the registry is: {})", MATERIALS.join(", ")),
            ));
        }
        recipe.push(PartRecipe {
            primitive,
            position,
            size,
            rotation,
            material: material.to_string(),
        });
    }
    // Resolve at defaults: catches undeclared $refs, bad sizes, and
    // unknown materials before the budget is checked.
    let defaults = resolve_defaults(&params);
    let parts_field = required(file, v, "parts", "prop file")?;
    let parts = resolve_recipe(&recipe, &defaults)
        .map_err(|e| at(file, parts_field, e))?;
    let mut tags = Vec::new();
    if let Some(t) = v.field("tags") {
        for item in expect_arr(file, t, "tags")? {
            let tag = expect_str(file, item, "tag")?;
            if !valid_tag(tag) {
                return Err(at(
                    file,
                    item,
                    format!("unknown tag '{tag}' (the vocabulary is: {})", TAGS.join(", ")),
                ));
            }
            tags.push(tag.to_string());
        }
    }
    let placement = match v.field("placement") {
        Some(p) => Some(parse_placement_contract(file, p)?),
        None => {
            warnings.push(format!(
                "prop '{id}': no placement contract — dressing only, no collision, no grapple anchor"
            ));
            None
        }
    };
    // Budget: required, exact. The ledger never trusts a self-report.
    let budget_v = required(file, v, "budget", "prop file")?;
    check_fields(file, budget_v, &["triangles", "atlas_tiles"], "budget")?;
    let declared_tris = expect_num(
        file,
        required(file, budget_v, "triangles", "budget")?,
        "budget triangles",
    )?;
    if declared_tris < 0.0 || declared_tris.fract() != 0.0 {
        return Err(at(
            file,
            required(file, budget_v, "triangles", "budget")?,
            "budget triangles must be a non-negative integer",
        ));
    }
    let declared_tiles = expect_num(
        file,
        required(file, budget_v, "atlas_tiles", "budget")?,
        "budget atlas_tiles",
    )?;
    if declared_tiles < 0.0 || declared_tiles.fract() != 0.0 {
        return Err(at(
            file,
            required(file, budget_v, "atlas_tiles", "budget")?,
            "budget atlas_tiles must be a non-negative integer",
        ));
    }
    let actual_tris: u64 = parts.iter().map(|p| p.primitive.triangles()).sum();
    if declared_tris as u64 != actual_tris {
        return Err(at(
            file,
            required(file, budget_v, "triangles", "budget")?,
            format!(
                "budget triangles {declared_tris} does not match the recipe's real count {actual_tris}"
            ),
        ));
    }
    if declared_tiles as u64 != 0 {
        return Err(at(
            file,
            required(file, budget_v, "atlas_tiles", "budget")?,
            "budget atlas_tiles must be 0 in v1 (mods reuse shipped regions; no new surfaces exist)",
        ));
    }
    // Solidity: the art direction's mass rule, as a warning, not a
    // rejection — a sub-20 mm member at arm's length reads as a wire.
    for (i, part) in parts.iter().enumerate() {
        let thin = match part.primitive {
            Primitive::Quad => part.size[0] < 0.02 || part.size[1] < 0.02,
            _ => part.size.iter().any(|s| *s < 0.02),
        };
        if thin {
            warnings.push(format!(
                "prop '{id}' part {i} ({}) has a sub-20 mm member; at play distance it reads as a wire, not a mass",
                part.primitive.name()
            ));
        }
    }
    Ok((
        PropDef {
            id,
            source_mod: None, // filled in by the caller
            params,
            parts,
            source: Source::Recipe(recipe),
            placement,
            tags,
        },
        warnings,
    ))
}

/// A variant override map from a JSON object (numbers + booleans).
fn parse_variant(file: &str, v: &Value) -> Result<ParamValues, String> {
    let entries = expect_obj(file, v, "variant")?;
    let mut out = ParamValues::new();
    for (k, val) in entries {
        match &val.kind {
            Kind::Num(n) => {
                if !n.is_finite() {
                    return Err(at(file, val, format!("variant '{k}' must be finite")));
                }
                out.insert(k.clone(), ParamValue::Num(*n as f32));
            }
            Kind::Bool(b) => {
                out.insert(k.clone(), ParamValue::Bool(*b));
            }
            _ => {
                return Err(at(
                    file,
                    val,
                    format!("variant '{k}' must be a number or a boolean, got {}", val.type_name()),
                ))
            }
        }
    }
    Ok(out)
}

fn validate_variant(
    file: &str,
    v: &Value,
    variant: &ParamValues,
    def: &PropDef,
    prop_id: &str,
) -> Result<(), String> {
    for (k, val) in variant {
        let param = def.params.get(k).ok_or_else(|| {
            at(file, v, format!("prop '{prop_id}' has no parameter '{k}'"))
        })?;
        param
            .check(val)
            .map_err(|e| at(file, v, format!("prop '{prop_id}' parameter '{k}': {e}")))?;
    }
    Ok(())
}

/// A placement target: a city anchor (resolved at load) or an
/// absolute position (allowed, but the validator warns it is
/// fragile — it will not follow a rebuilt street).
#[derive(Debug, Clone)]
pub enum Target {
    Anchor(String),
    Position([f32; 3]),
}

/// A repeat stamp: `count` instances spaced `spacing` metres apart
/// along the placement's facing axis ("x" or "z", in yaw space).
#[derive(Debug, Clone)]
pub struct Repeat {
    pub count: u32,
    pub spacing: f32,
    pub axis: char,
}

#[derive(Debug, Clone)]
pub struct PlacementEntry {
    pub prop: String,
    pub variant: ParamValues,
    pub target: Target,
    pub yaw: f32,
    pub repeat: Option<Repeat>,
    /// Source location for load reports (file:line of the entry).
    pub at: String,
}

#[derive(Debug, Clone)]
pub struct ReplacementEntry {
    pub prop: String,
    pub variant: ParamValues,
}

fn parse_placements_file(
    file: &str,
    v: &Value,
    shipped: &Registry,
    own_props: &BTreeMap<String, PropDef>,
) -> Result<(Vec<PlacementEntry>, Vec<ReplacementEntry>, Vec<String>), String> {
    let mut warnings = Vec::new();
    check_fields(file, v, &["placements", "replacements"], "placements file")?;
    let mut placements = Vec::new();
    if let Some(pv) = v.field("placements") {
        for entry in expect_arr(file, pv, "placements")? {
            check_fields(
                file,
                entry,
                &["prop", "variant", "anchor", "position", "yaw", "repeat"],
                "placement entry",
            )?;
            let prop = expect_str(
                file,
                required(file, entry, "prop", "placement entry")?,
                "placement prop",
            )?;
            let prop_field = required(file, entry, "prop", "placement entry")?;
            let def = shipped
                .get(prop)
                .or_else(|| own_props.get(prop))
                .ok_or_else(|| {
                    at(
                        file,
                        prop_field,
                        format!("placement references unknown prop '{prop}' (v1 placements use shipped props or this mod's own props)"),
                    )
                })?;
            let variant = match entry.field("variant") {
                Some(vv) => parse_variant(file, vv)?,
                None => ParamValues::new(),
            };
            validate_variant(file, entry, &variant, def, prop)?;
            let has_anchor = entry.field("anchor").is_some();
            let has_position = entry.field("position").is_some();
            let target = match (has_anchor, has_position) {
                (true, false) => {
                    let a = expect_str(
                        file,
                        required(file, entry, "anchor", "placement entry")?,
                        "placement anchor",
                    )?;
                    if a.is_empty() {
                        return Err(at(
                            file,
                            required(file, entry, "anchor", "placement entry")?,
                            "placement anchor must not be empty",
                        ));
                    }
                    Target::Anchor(a.to_string())
                }
                (false, true) => {
                    let pos = parse_num_array(
                        file,
                        required(file, entry, "position", "placement entry")?,
                        3,
                        "placement position",
                    )?;
                    warnings.push(format!(
                        "{}:{}: placement of '{prop}' uses an absolute position; it will not follow a rebuilt street (prefer an anchor)",
                        file, entry.line
                    ));
                    Target::Position([pos[0] as f32, pos[1] as f32, pos[2] as f32])
                }
                (true, true) => {
                    return Err(at(
                        file,
                        entry,
                        "placement entry must name exactly one of 'anchor' or 'position'",
                    ))
                }
                (false, false) => {
                    return Err(at(
                        file,
                        entry,
                        "placement entry must name 'anchor' or 'position'",
                    ))
                }
            };
            let yaw = match entry.field("yaw") {
                Some(y) => expect_num(file, y, "placement yaw")? as f32,
                None => 0.0,
            };
            let repeat = match entry.field("repeat") {
                Some(r) => {
                    check_fields(file, r, &["count", "spacing", "axis"], "repeat")?;
                    let count_f = expect_num(
                        file,
                        required(file, r, "count", "repeat")?,
                        "repeat count",
                    )?;
                    if count_f < 1.0 || count_f > 64.0 || count_f.fract() != 0.0 {
                        return Err(at(
                            file,
                            required(file, r, "count", "repeat")?,
                            "repeat count must be an integer from 1 to 64",
                        ));
                    }
                    let spacing = expect_num(
                        file,
                        required(file, r, "spacing", "repeat")?,
                        "repeat spacing",
                    )?;
                    if spacing <= 0.0 {
                        return Err(at(
                            file,
                            required(file, r, "spacing", "repeat")?,
                            "repeat spacing must be positive",
                        ));
                    }
                    let axis = match r.field("axis") {
                        Some(a) => {
                            let s = expect_str(file, a, "repeat axis")?;
                            if s != "x" && s != "z" {
                                return Err(at(
                                    file,
                                    a,
                                    "repeat axis must be 'x' or 'z'",
                                ));
                            }
                            s.chars().next().unwrap()
                        }
                        None => 'x',
                    };
                    Some(Repeat {
                        count: count_f as u32,
                        spacing: spacing as f32,
                        axis,
                    })
                }
                None => None,
            };
            placements.push(PlacementEntry {
                prop: prop.to_string(),
                variant,
                target,
                yaw,
                repeat,
                at: format!("{file}:{}", entry.line),
            });
        }
    }
    let mut replacements = Vec::new();
    if let Some(rv) = v.field("replacements") {
        for entry in expect_arr(file, rv, "replacements")? {
            check_fields(file, entry, &["prop", "variant"], "replacement entry")?;
            let prop = expect_str(
                file,
                required(file, entry, "prop", "replacement entry")?,
                "replacement prop",
            )?;
            let prop_field = required(file, entry, "prop", "replacement entry")?;
            let def = shipped.get(prop).ok_or_else(|| {
                at(
                    file,
                    prop_field,
                    format!("replacement targets '{prop}', which is not a shipped prop (replacements apply to shipped props only)"),
                )
            })?;
            let variant = parse_variant(
                file,
                required(file, entry, "variant", "replacement entry")?,
            )?;
            if variant.is_empty() {
                return Err(at(
                    file,
                    required(file, entry, "variant", "replacement entry")?,
                    "replacement variant must not be empty",
                ));
            }
            validate_variant(file, entry, &variant, def, prop)?;
            replacements.push(ReplacementEntry {
                prop: prop.to_string(),
                variant,
            });
        }
    }
    Ok((placements, replacements, warnings))
}

// ---------------------------------------------------------------------
// Single-folder load.
// ---------------------------------------------------------------------

/// A successfully loaded mod (before multi-mod ordering).
pub struct LoadedMod {
    pub folder_name: String,
    pub manifest: ModManifest,
    pub props: Vec<PropDef>,
    pub placements: Vec<PlacementEntry>,
    pub replacements: Vec<ReplacementEntry>,
    pub warnings: Vec<String>,
}

/// A quarantined mod: it contributes nothing. Reasons are plain
/// words with file:line locations.
pub struct QuarantinedMod {
    pub folder_name: String,
    pub reasons: Vec<String>,
}

fn read_text(path: &Path) -> Result<String, String> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|_| format!("{}: file is not valid UTF-8", path.display())),
        Err(e) => Err(format!("{}: cannot read file ({e})", path.display())),
    }
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn walk_sorted(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return,
    };
    entries.sort();
    for path in entries {
        if path.is_dir() && !is_symlink(&path) {
            walk_sorted(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Load one mod folder. `Ok` is a loadable mod; `Err` is the
/// quarantine reason list (the mod contributes nothing).
pub fn load_mod_folder(dir: &Path) -> Result<LoadedMod, QuarantinedMod> {
    let folder_name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    let fail = |reasons: Vec<String>| -> Result<LoadedMod, QuarantinedMod> {
        Err(QuarantinedMod {
            folder_name: folder_name.clone(),
            reasons,
        })
    };
    // 1. The manifest first: without it, nothing else is readable.
    let manifest_path = dir.join("mod.json");
    let manifest_text = match read_text(&manifest_path) {
        Ok(t) => t,
        Err(e) => return fail(vec![e]),
    };
    let manifest_value = match json::parse(&manifest_text) {
        Ok(v) => v,
        Err(e) => return fail(vec![format!("mod.json:{e}")]),
    };
    let manifest = match parse_manifest("mod.json", &manifest_value) {
        Ok(m) => m,
        Err(e) => return fail(vec![e]),
    };
    // 2. The engine range, BEFORE any other file in the mod is read.
    match version_satisfies(&manifest.engine_range, current_version()) {
        Ok(true) => {}
        Ok(false) => {
            return fail(vec![format!(
                "mod '{}' is built for engine '{}', running {} — refused, not loaded hopefully",
                manifest.id, manifest.engine_range, ENGINE_VERSION
            )]);
        }
        Err(e) => return fail(vec![format!("mod.json: engine range: {e}")]),
    }
    // 3. Scan the folder: images and v2 files refuse the mod; stray
    // files are errors; docs are ignored.
    let mut files: Vec<PathBuf> = Vec::new();
    walk_sorted(dir, &mut files);
    let mut reasons: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut prop_files: Vec<PathBuf> = Vec::new();
    let mut placements_file: Option<PathBuf> = None;
    for path in &files {
        let rel = path.strip_prefix(dir).unwrap_or(path);
        let rel_str = rel.to_string_lossy();
        if is_symlink(path) {
            warnings.push(format!("{rel_str}: symlinked file skipped"));
            continue;
        }
        if rel_str == "mod.json" {
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());
        match ext.as_deref() {
            Some(e) if IMAGE_EXTS.contains(&e) => {
                reasons.push(format!(
                    "{rel_str}: image imports are not supported; define the surface as a painter recipe"
                ));
            }
            Some("md") | Some("txt") => {} // docs, ignored
            Some("json") => {
                if rel_str == "placements.json" {
                    placements_file = Some(path.clone());
                } else if rel.components().count() == 2
                    && rel.components().next().map(|c| c.as_os_str() == "props").unwrap_or(false)
                {
                    prop_files.push(path.clone());
                } else if rel_str == "tuning.json" {
                    reasons.push(format!(
                        "{rel_str}: tuning tables are v2; refused in v1"
                    ));
                } else {
                    reasons.push(format!("{rel_str}: unexpected JSON file in a mod folder"));
                }
            }
            _ => {
                reasons.push(format!("{rel_str}: unsupported file type in a mod folder"));
            }
        }
    }
    // 4. Prop files.
    let shipped = Registry::new();
    let mut props: Vec<PropDef> = Vec::new();
    let mut seen_ids: BTreeMap<String, String> = BTreeMap::new();
    for path in &prop_files {
        let rel = path.strip_prefix(dir).unwrap_or(path);
        let rel_str = rel.to_string_lossy().into_owned();
        let text = match read_text(path) {
            Ok(t) => t,
            Err(e) => {
                reasons.push(e);
                continue;
            }
        };
        let value = match json::parse(&text) {
            Ok(v) => v,
            Err(e) => {
                reasons.push(format!("{rel_str}:{e}"));
                continue;
            }
        };
        match parse_prop_file(&rel_str, &value, &manifest.id) {
            Ok((mut def, mut warns)) => {
                if let Some(prev) = seen_ids.get(&def.id) {
                    reasons.push(format!(
                        "{rel_str}: prop '{}' is already defined in {prev} (one definition per id per mod)",
                        def.id
                    ));
                    continue;
                }
                seen_ids.insert(def.id.clone(), rel_str.clone());
                def.source_mod = Some(manifest.id.clone());
                warnings.append(&mut warns);
                props.push(def);
            }
            Err(e) => reasons.push(e),
        }
    }
    // 5. Placements + replacements (against shipped + own props).
    let own_map: BTreeMap<String, PropDef> = props
        .iter()
        .map(|p| (p.id.clone(), p.clone()))
        .collect();
    let mut placements = Vec::new();
    let mut replacements = Vec::new();
    if let Some(path) = &placements_file {
        let rel = path.strip_prefix(dir).unwrap_or(path);
        let rel_str = rel.to_string_lossy().into_owned();
        let text = match read_text(path) {
            Ok(t) => t,
            Err(e) => {
                reasons.push(e);
                String::new()
            }
        };
        if !text.is_empty() {
            match json::parse(&text) {
                Ok(value) => {
                    match parse_placements_file(&rel_str, &value, &shipped, &own_map) {
                        Ok((pl, rep, mut warns)) => {
                            placements = pl;
                            replacements = rep;
                            warnings.append(&mut warns);
                        }
                        Err(e) => reasons.push(e),
                    }
                }
                Err(e) => reasons.push(format!("{rel_str}:{e}")),
            }
        }
    }
    if !reasons.is_empty() {
        return fail(reasons);
    }
    Ok(LoadedMod {
        folder_name,
        manifest,
        props,
        placements,
        replacements,
        warnings,
    })
}

// ---------------------------------------------------------------------
// Multi-mod load: ordering, conflicts, placement resolution.
// ---------------------------------------------------------------------

/// A placed instance, resolved: prop + world transform + budget.
#[derive(Debug, Clone)]
pub struct PlacedInstance {
    pub prop: String,
    pub position: [f32; 3],
    pub yaw: f32,
    pub triangles: u64,
    pub from: String, // "mod-id (file:line)"
}

/// The full load result: everything the host needs to build the
/// modded world, plus the report the Mods menu will read.
pub struct LoadReport {
    /// Mod ids in load order (base game is implicit, first).
    pub order: Vec<String>,
    pub conflicts: Vec<String>,
    pub quarantined: Vec<QuarantinedMod>,
    pub dropped_placements: Vec<String>,
    pub placed: Vec<PlacedInstance>,
    pub warnings: Vec<String>,
    /// True when at least one mod loaded: the run is MODDED.
    pub modded: bool,
    /// Triangles added by mod placements (shipped-prop placements
    /// count their shipped geometry too — the ledger counts what
    /// the world draws).
    pub placed_triangles: u64,
}

impl LoadReport {
    /// The ledger line the host records for a modded run.
    pub fn ledger_line(&self) -> String {
        if !self.modded {
            return "MODDED=0".to_string();
        }
        format!(
            "MODDED=1 mods=[{}] order=[{}] quarantined=[{}]",
            self.order.join(","),
            self.order.join(","),
            self.quarantined
                .iter()
                .map(|q| q.folder_name.clone())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

/// Load several mod folders against an anchor table
/// (anchor name -> (position, yaw)). Deterministic: folder order
/// never affects the result except through the documented tiebreak.
pub fn load_mods(
    dirs: &[PathBuf],
    anchors: &BTreeMap<String, ([f32; 3], f32)>,
) -> LoadReport {
    // Phase 1: single-folder loads, in folder-name order.
    let mut sorted_dirs = dirs.to_vec();
    sorted_dirs.sort();
    let mut loaded: Vec<LoadedMod> = Vec::new();
    let mut quarantined: Vec<QuarantinedMod> = Vec::new();
    for dir in &sorted_dirs {
        match load_mod_folder(dir) {
            Ok(m) => loaded.push(m),
            Err(q) => quarantined.push(q),
        }
    }
    // Phase 2: ordering. Kahn's algorithm with a folder-name-sorted
    // ready set. A target is satisfied only by a mod that actually
    // ordered; a target that is missing, quarantined, or itself
    // quarantines the dependent. Whatever is left when nothing is
    // ready and nothing newly quarantines is a load_after cycle.
    let mut by_id: BTreeMap<String, usize> = BTreeMap::new();
    for (i, m) in loaded.iter().enumerate() {
        // Two folders may share a namespace id; the first in
        // folder-name order owns the id for load_after purposes.
        by_id.entry(m.manifest.id.clone()).or_insert(i);
    }
    let mut remaining: BTreeSet<usize> = (0..loaded.len()).collect();
    let mut ordered_set: BTreeSet<usize> = BTreeSet::new();
    let mut quarantined_idx: BTreeSet<usize> = BTreeSet::new();
    let mut order_idx: Vec<usize> = Vec::new();
    loop {
        if remaining.is_empty() {
            break;
        }
        // Quarantine pass: dependents of dead targets.
        let mut newly_dead: Vec<usize> = Vec::new();
        for &i in &remaining {
            for target in &loaded[i].manifest.load_after {
                match by_id.get(target) {
                    Some(&j) if j == i => {
                        newly_dead.push(i);
                        quarantined.push(QuarantinedMod {
                            folder_name: loaded[i].folder_name.clone(),
                            reasons: vec![format!(
                                "load_after target '{target}' is itself"
                            )],
                        });
                        break;
                    }
                    Some(&j) if quarantined_idx.contains(&j) => {
                        newly_dead.push(i);
                        quarantined.push(QuarantinedMod {
                            folder_name: loaded[i].folder_name.clone(),
                            reasons: vec![format!(
                                "load_after target '{target}' is quarantined, so this mod cannot load after it"
                            )],
                        });
                        break;
                    }
                    Some(_) => {} // waiting or ordered; decided below
                    None => {
                        newly_dead.push(i);
                        quarantined.push(QuarantinedMod {
                            folder_name: loaded[i].folder_name.clone(),
                            reasons: vec![format!(
                                "load_after target '{target}' is not a loaded mod"
                            )],
                        });
                        break;
                    }
                }
            }
        }
        for i in newly_dead {
            remaining.remove(&i);
            quarantined_idx.insert(i);
        }
        if remaining.is_empty() {
            break;
        }
        // Ready pass: every target ordered. Folder-name order wins.
        let mut ready: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&i| {
                loaded[i].manifest.load_after.iter().all(|target| {
                    match by_id.get(target) {
                        Some(&j) => ordered_set.contains(&j),
                        None => false, // dead targets were quarantined above
                    }
                })
            })
            .collect();
        ready.sort_by_key(|&i| loaded[i].folder_name.clone());
        if let Some(&first) = ready.first() {
            remaining.remove(&first);
            ordered_set.insert(first);
            order_idx.push(first);
            continue;
        }
        // Nothing ready and nothing newly quarantined this round:
        // the rest wait on each other — a cycle.
        let members: Vec<String> = remaining
            .iter()
            .map(|&i| loaded[i].manifest.id.clone())
            .collect();
        for &i in &remaining {
            quarantined.push(QuarantinedMod {
                folder_name: loaded[i].folder_name.clone(),
                reasons: vec![format!(
                    "load_after cycle involving: {}",
                    members.join(", ")
                )],
            });
        }
        remaining.clear();
        break;
    }
    // Phase 3: apply in order.
    let mut registry = Registry::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    let mut placed: Vec<PlacedInstance> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut placed_triangles: u64 = 0;
    let mut order: Vec<String> = Vec::new();
    for &i in &order_idx {
        let m = &loaded[i];
        order.push(m.manifest.id.clone());
        warnings.extend(m.warnings.iter().cloned());
        for prop in &m.props {
            if let Some(prev) = registry.insert(prop.clone()) {
                let prev_src = prev
                    .source_mod
                    .clone()
                    .unwrap_or_else(|| "the base game".to_string());
                conflicts.push(format!(
                    "prop '{}' redefined by '{}' (was {}); later wins, the earlier definition is ignored, not merged",
                    prop.id, m.manifest.id, prev_src
                ));
            }
        }
        for rep in &m.replacements {
            // Replacements apply to the CURRENT definition of the
            // id; if an earlier mod redefined it, the replacement
            // must still fit that definition's parameters.
            match registry.get(&rep.prop) {
                Some(def) => {
                    let mut ok = true;
                    for (k, v) in &rep.variant {
                        match def.params.get(k) {
                            Some(param) => {
                                if let Err(e) = param.check(v) {
                                    dropped.push(format!(
                                        "replacement of '{}' by '{}' dropped: parameter '{k}': {e}",
                                        rep.prop, m.manifest.id
                                    ));
                                    ok = false;
                                    break;
                                }
                            }
                            None => {
                                dropped.push(format!(
                                    "replacement of '{}' by '{}' dropped: no parameter '{k}' on the current definition",
                                    rep.prop, m.manifest.id
                                ));
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        if registry.replacements.contains_key(&rep.prop) {
                            conflicts.push(format!(
                                "shipped prop '{}' parameters replaced by '{}' after an earlier replacement; later wins",
                                rep.prop, m.manifest.id
                            ));
                        }
                        registry.replace(&rep.prop, rep.variant.clone());
                    }
                }
                None => {
                    dropped.push(format!(
                        "replacement of '{}' by '{}' dropped: unknown prop",
                        rep.prop, m.manifest.id
                    ));
                }
            }
        }
        for entry in &m.placements {
            let def = match registry.get(&entry.prop) {
                Some(d) => d.clone(),
                None => {
                    dropped.push(format!(
                        "placement of '{}' by '{}' ({}) dropped: unknown prop",
                        entry.prop, m.manifest.id, entry.at
                    ));
                    continue;
                }
            };
            // Resolve the target.
            let (base_pos, base_yaw) = match &entry.target {
                Target::Anchor(name) => match anchors.get(name) {
                    Some(&(pos, yaw)) => (pos, yaw),
                    None => {
                        dropped.push(format!(
                            "placement of '{}' by '{}' ({}) dropped: unknown anchor '{name}'",
                            entry.prop, m.manifest.id, entry.at
                        ));
                        continue;
                    }
                },
                Target::Position(pos) => (*pos, 0.0),
            };
            let yaw = base_yaw + entry.yaw;
            let repeat = entry.repeat.clone().unwrap_or(Repeat {
                count: 1,
                spacing: 0.0,
                axis: 'x',
            });
            // Build once for the budget (validates the variant
            // against the current definition, after replacements).
            let built = match registry.build(&entry.prop, &entry.variant) {
                Ok(b) => b,
                Err(e) => {
                    dropped.push(format!(
                        "placement of '{}' by '{}' ({}) dropped: {e}",
                        entry.prop, m.manifest.id, entry.at
                    ));
                    continue;
                }
            };
            let (sin, cos) = (yaw.to_radians().sin(), yaw.to_radians().cos());
            for k in 0..repeat.count {
                let d = k as f32 * repeat.spacing;
                let (dx, dz) = match repeat.axis {
                    'x' => (d * cos, -d * sin),
                    _ => (d * sin, d * cos),
                };
                let pos = [
                    base_pos[0] + dx,
                    base_pos[1],
                    base_pos[2] + dz,
                ];
                // Footprint overlap (2D, axis-aligned, yaw ignored
                // — a documented v1 approximation): dressing-only
                // props (no contract) never block.
                if let Some(contract) = &def.placement {
                    let (hw, hd) = (contract.footprint[0] / 2.0, contract.footprint[1] / 2.0);
                    let mut overlaps = false;
                    for other in &placed {
                        let other_def = registry.get(&other.prop);
                        let (ohw, ohd) = match other_def.and_then(|d| d.placement.as_ref()) {
                            Some(c) => (c.footprint[0] / 2.0, c.footprint[1] / 2.0),
                            None => continue,
                        };
                        if (pos[0] - other.position[0]).abs() < hw + ohw
                            && (pos[2] - other.position[2]).abs() < hd + ohd
                        {
                            overlaps = true;
                            break;
                        }
                    }
                    if overlaps {
                        dropped.push(format!(
                            "placement of '{}' by '{}' ({}) at [{:.2}, {:.2}, {:.2}] dropped: footprint overlaps an earlier placement",
                            entry.prop, m.manifest.id, entry.at, pos[0], pos[1], pos[2]
                        ));
                        continue;
                    }
                }
                placed_triangles += built.triangles;
                placed.push(PlacedInstance {
                    prop: entry.prop.clone(),
                    position: pos,
                    yaw,
                    triangles: built.triangles,
                    from: format!("{} ({})", m.manifest.id, entry.at),
                });
            }
        }
    }
    LoadReport {
        modded: !order.is_empty(),
        order,
        conflicts,
        quarantined,
        dropped_placements: dropped,
        placed,
        warnings,
        placed_triangles,
    }
}

