//! The prop registry: every shipped prop, named and parametric.
//!
//! The registry is engine-owned and built at startup from the
//! builders below. It maps a stable namespaced id to a definition:
//! parts, parameters with legal ranges, placement contract, budget,
//! tags. Mods reuse shipped props by id and override parameters;
//! they never copy geometry.
//!
//! The shipped set mirrors what the game's block builder actually
//! puts in Greyport (lamp standards, the street banner, crates,
//! drums, the kiosk's neighbours, checkpoint furniture, the debrief
//! room's desk and chair) — not an invented catalogue. Proportions
//! follow the block builder's own numbers where they exist (the
//! lamp standard's base/pole/head stack is the block's, to the
//! centimetre).
//!
//! Game-side adoption, stated plainly: the block builder itself
//! still builds its geometry inline (it lives in the game crate,
//! outside this engine crate's scope). This registry is built so
//! the city pass can rebuild the block FROM these entries —
//! `build_mesh(id, overrides)` is the call the city builder will
//! make. Until that adoption lands, the registry is the engine's
//! contract for what the city may reuse, and the validator proves
//! every entry builds.

use crate::modding::mesh;
use crate::modding::model::*;
use crate::props::PropMesh;
use std::collections::BTreeMap;

/// A built prop: resolved parts + mesh + budget.
pub struct BuiltProp {
    pub parts: Vec<Part>,
    pub mesh: PropMesh,
    pub triangles: u64,
}

fn part(
    primitive: Primitive,
    position: [f32; 3],
    size: [f32; 3],
    rotation: [f32; 3],
    material: &str,
) -> Part {
    let (region, mat_flag) = material_region(material)
        .unwrap_or_else(|| panic!("shipped builder used unknown material '{material}'"));
    Part {
        primitive,
        position,
        size,
        rotation,
        region,
        mat_flag,
    }
}

fn bx(position: [f32; 3], size: [f32; 3], material: &str) -> Part {
    part(Primitive::Box, position, size, [0.0, 0.0, 0.0], material)
}

fn num(values: &ParamValues, name: &str) -> f32 {
    match values.get(name) {
        Some(ParamValue::Num(n)) => *n,
        _ => panic!("shipped builder missing numeric parameter '{name}'"),
    }
}

fn flag(values: &ParamValues, name: &str) -> bool {
    match values.get(name) {
        Some(ParamValue::Bool(b)) => *b,
        _ => panic!("shipped builder missing boolean parameter '{name}'"),
    }
}

// ---------------------------------------------------------------------
// Shipped builders (proportions follow the block builder)
// ---------------------------------------------------------------------

fn build_lamp(values: &ParamValues) -> Vec<Part> {
    let h = num(values, "height");
    vec![
        // Base, pole, arm, head housing, glowing core, down glass.
        bx([0.0, 0.31, 0.0], [0.32, 0.38, 0.32], "painted-metal"),
        bx(
            [0.0, 0.12 + (h - 0.12) / 2.0, 0.0],
            [0.11, h - 0.12, 0.11],
            "painted-metal",
        ),
        bx([0.45, h - 0.07, 0.0], [0.9, 0.1, 0.1], "painted-metal"),
        bx([0.9, h + 0.09, 0.0], [0.68, 0.16, 0.3], "painted-metal"),
        bx([0.9, h + 0.12, 0.0], [0.52, 0.1, 0.33], "lamp-glass"),
        // Down glass: a quad rotated to face -Y.
        part(
            Primitive::Quad,
            [0.9, h, 0.0],
            [0.56, 0.22, 0.0],
            [90.0, 0.0, 0.0],
            "lamp-glass",
        ),
    ]
}

fn build_crate(values: &ParamValues) -> Vec<Part> {
    let open = flag(values, "open");
    let mut parts = vec![bx([0.0, 0.3, 0.0], [0.6, 0.6, 0.6], "timber")];
    if open {
        // Lid leaning against the crate's side, rotated open.
        parts.push(part(
            Primitive::Box,
            [0.42, 0.72, 0.0],
            [0.62, 0.06, 0.62],
            [0.0, 0.0, -65.0],
            "timber",
        ));
    } else {
        parts.push(bx([0.0, 0.63, 0.0], [0.62, 0.06, 0.62], "timber"));
    }
    parts
}

fn build_drum(_values: &ParamValues) -> Vec<Part> {
    vec![
        part(
            Primitive::Cylinder,
            [0.0, 0.45, 0.0],
            [0.6, 0.9, 0.6],
            [0.0, 0.0, 0.0],
            "painted-metal",
        ),
        part(
            Primitive::Cylinder,
            [0.0, 0.92, 0.0],
            [0.62, 0.04, 0.62],
            [0.0, 0.0, 0.0],
            "painted-metal",
        ),
    ]
}

fn build_bollard(_values: &ParamValues) -> Vec<Part> {
    vec![
        part(
            Primitive::Cylinder,
            [0.0, 0.45, 0.0],
            [0.22, 0.9, 0.22],
            [0.0, 0.0, 0.0],
            "stone",
        ),
        part(
            Primitive::Cylinder,
            [0.0, 0.93, 0.0],
            [0.26, 0.06, 0.26],
            [0.0, 0.0, 0.0],
            "stone",
        ),
    ]
}

fn build_banner(values: &ParamValues) -> Vec<Part> {
    let y = if flag(values, "raised") { 3.0 } else { 1.8 };
    vec![
        part(
            Primitive::Quad,
            [0.0, y, 0.0],
            [1.8, 1.2, 0.0],
            [0.0, 0.0, 0.0],
            "banner",
        ),
        bx([0.0, y + 0.62, 0.0], [1.9, 0.06, 0.06], "painted-metal"),
    ]
}

fn build_bench(_values: &ParamValues) -> Vec<Part> {
    vec![
        bx([0.0, 0.45, 0.0], [1.8, 0.08, 0.5], "timber"),
        bx([-0.7, 0.22, 0.0], [0.08, 0.44, 0.44], "painted-metal"),
        bx([0.7, 0.22, 0.0], [0.08, 0.44, 0.44], "painted-metal"),
        bx([0.0, 0.85, -0.22], [1.8, 0.5, 0.06], "timber"),
    ]
}

fn build_door(_values: &ParamValues) -> Vec<Part> {
    vec![
        bx([0.0, 1.05, 0.0], [0.95, 2.1, 0.08], "door"),
        bx([-0.52, 1.05, 0.0], [0.1, 2.2, 0.12], "painted-metal"),
        bx([0.52, 1.05, 0.0], [0.1, 2.2, 0.12], "painted-metal"),
        bx([0.0, 2.15, 0.0], [1.14, 0.1, 0.12], "painted-metal"),
    ]
}

fn build_shutter(_values: &ParamValues) -> Vec<Part> {
    vec![
        bx([0.0, 1.4, 0.0], [2.4, 2.8, 0.1], "roller-shutter"),
        bx([0.0, 2.85, 0.0], [2.5, 0.1, 0.14], "painted-metal"),
    ]
}

fn frame_window(material: &str) -> Vec<Part> {
    vec![
        part(
            Primitive::Quad,
            [0.0, 1.6, 0.0],
            [1.2, 1.4, 0.0],
            [0.0, 0.0, 0.0],
            material,
        ),
        bx([0.0, 2.32, 0.0], [1.3, 0.06, 0.06], "painted-metal"),
        bx([0.0, 0.88, 0.0], [1.3, 0.06, 0.06], "painted-metal"),
        bx([-0.63, 1.6, 0.0], [0.06, 1.46, 0.06], "painted-metal"),
        bx([0.63, 1.6, 0.0], [0.06, 1.46, 0.06], "painted-metal"),
    ]
}

fn build_window_lit(_values: &ParamValues) -> Vec<Part> {
    frame_window("window-lit")
}

fn build_window_dark(_values: &ParamValues) -> Vec<Part> {
    frame_window("window-dark")
}

fn build_desk(_values: &ParamValues) -> Vec<Part> {
    vec![
        bx([0.0, 0.74, 0.0], [1.6, 0.06, 0.8], "timber"),
        bx([-0.72, 0.36, -0.32], [0.08, 0.72, 0.08], "painted-metal"),
        bx([0.72, 0.36, -0.32], [0.08, 0.72, 0.08], "painted-metal"),
        bx([-0.72, 0.36, 0.32], [0.08, 0.72, 0.08], "painted-metal"),
        bx([0.72, 0.36, 0.32], [0.08, 0.72, 0.08], "painted-metal"),
        bx([0.4, 0.6, 0.38], [0.6, 0.2, 0.04], "timber"),
    ]
}

fn build_chair(_values: &ParamValues) -> Vec<Part> {
    vec![
        bx([0.0, 0.45, 0.0], [0.5, 0.06, 0.5], "timber"),
        bx([-0.2, 0.22, -0.2], [0.06, 0.44, 0.06], "painted-metal"),
        bx([0.2, 0.22, -0.2], [0.06, 0.44, 0.06], "painted-metal"),
        bx([-0.2, 0.22, 0.2], [0.06, 0.44, 0.06], "painted-metal"),
        bx([0.2, 0.22, 0.2], [0.06, 0.44, 0.06], "painted-metal"),
        bx([0.0, 0.85, -0.22], [0.5, 0.7, 0.06], "timber"),
    ]
}

fn contract(
    footprint: [f32; 2],
    height: f32,
    collision: CollisionClass,
    grapple_anchor: bool,
    occluder: bool,
    footstep: Footstep,
) -> PlacementContract {
    PlacementContract {
        footprint,
        height,
        collision,
        grapple_anchor,
        occluder,
        footstep,
    }
}

fn shipped(
    id: &str,
    params: Vec<(&str, ParamDef)>,
    build: ShippedBuilder,
    placement: PlacementContract,
    tags: Vec<&str>,
) -> PropDef {
    let mut param_map = BTreeMap::new();
    for (name, def) in params {
        param_map.insert(name.to_string(), def);
    }
    let defaults = resolve_defaults(&param_map);
    let parts = build(&defaults);
    PropDef {
        id: id.to_string(),
        source_mod: None,
        params: param_map,
        parts,
        source: Source::Built(build),
        placement: Some(placement),
        tags: tags.iter().map(|s| s.to_string()).collect(),
    }
}

/// Defaults for a parameter map (used to build shipped props at
/// registration and as the base for every override chain).
pub fn resolve_defaults(params: &BTreeMap<String, ParamDef>) -> ParamValues {
    params
        .iter()
        .map(|(k, d)| (k.clone(), d.default))
        .collect()
}

/// Resolve a recipe's `$param` references against values. Unknown
/// parameters and boolean-in-a-number-slot are errors.
pub fn resolve_recipe(
    recipe: &[PartRecipe],
    values: &ParamValues,
) -> Result<Vec<Part>, String> {
    fn resolve_one(v: &NumOrParam, values: &ParamValues) -> Result<f32, String> {
        match v {
            NumOrParam::Num(n) => {
                if n.is_finite() {
                    Ok(*n)
                } else {
                    Err("recipe number must be finite".to_string())
                }
            }
            NumOrParam::Param(name) => match values.get(name) {
                Some(ParamValue::Num(n)) => Ok(*n),
                Some(ParamValue::Bool(_)) => {
                    Err(format!("parameter '${name}' is a boolean, not a number"))
                }
                None => Err(format!("recipe references undeclared parameter '${name}'")),
            },
        }
    }
    let mut out = Vec::new();
    for r in recipe {
        let (region, mat_flag) = material_region(&r.material)
            .ok_or_else(|| format!("unknown material '{}'", r.material))?;
        let mut position = [0.0; 3];
        let mut size = [0.0; 3];
        let mut rotation = [0.0; 3];
        for i in 0..3 {
            position[i] = resolve_one(&r.position[i], values)?;
            size[i] = resolve_one(&r.size[i], values)?;
            rotation[i] = resolve_one(&r.rotation[i], values)?;
        }
        // Degenerate geometry is a recipe error, not a silent
        // invisible part (and it would NaN the mesh UVs).
        match r.primitive {
            Primitive::Box | Primitive::Wedge | Primitive::Cylinder => {
                for (i, s) in size.iter().enumerate() {
                    if *s <= 0.0 {
                        return Err(format!(
                            "part size component {i} must be positive, got {s}"
                        ));
                    }
                }
            }
            Primitive::Quad => {
                if size[0] <= 0.0 || size[1] <= 0.0 {
                    return Err(format!(
                        "quad width and height must be positive, got {} x {}",
                        size[0], size[1]
                    ));
                }
            }
        }
        out.push(Part {
            primitive: r.primitive,
            position,
            size,
            rotation,
            region,
            mat_flag,
        });
    }
    Ok(out)
}

/// The shipped prop table. Every prop the game ships, named.
pub fn shipped_props() -> Vec<PropDef> {
    vec![
        shipped(
            "greyport:lamp-post",
            vec![("height", ParamDef::num(6.05, 4.0, 7.0))],
            build_lamp,
            contract(
                [1.4, 0.7],
                6.22,
                CollisionClass::Solid,
                true,
                false,
                Footstep::Metal,
            ),
            vec!["street", "kerb"],
        ),
        shipped(
            "greyport:crate",
            vec![("open", ParamDef::flag(false))],
            build_crate,
            contract(
                [0.62, 0.62],
                0.66,
                CollisionClass::Solid,
                false,
                false,
                Footstep::Wood,
            ),
            vec!["street", "dockside", "rooftop"],
        ),
        shipped(
            "greyport:drum",
            vec![],
            build_drum,
            contract(
                [0.62, 0.62],
                0.94,
                CollisionClass::Solid,
                false,
                false,
                Footstep::Metal,
            ),
            vec!["street", "dockside"],
        ),
        shipped(
            "greyport:bollard",
            vec![],
            build_bollard,
            contract(
                [0.26, 0.26],
                0.96,
                CollisionClass::Solid,
                false,
                false,
                Footstep::Concrete,
            ),
            vec!["kerb", "street"],
        ),
        shipped(
            "greyport:three-bars-banner",
            vec![("raised", ParamDef::flag(true))],
            build_banner,
            contract(
                [1.9, 0.1],
                3.65,
                CollisionClass::PassThrough,
                false,
                false,
                Footstep::None,
            ),
            vec!["wall-face"],
        ),
        shipped(
            "greyport:bench",
            vec![],
            build_bench,
            contract(
                [1.8, 0.5],
                1.1,
                CollisionClass::Solid,
                false,
                false,
                Footstep::Wood,
            ),
            vec!["street", "interior"],
        ),
        shipped(
            "greyport:door",
            vec![],
            build_door,
            contract(
                [1.14, 0.12],
                2.2,
                CollisionClass::Solid,
                false,
                false,
                Footstep::None,
            ),
            vec!["wall-face", "interior"],
        ),
        shipped(
            "greyport:roller-shutter",
            vec![],
            build_shutter,
            contract(
                [2.5, 0.14],
                2.9,
                CollisionClass::Solid,
                false,
                false,
                Footstep::None,
            ),
            vec!["wall-face"],
        ),
        shipped(
            "greyport:window-lit",
            vec![],
            build_window_lit,
            contract(
                [1.3, 0.06],
                2.35,
                CollisionClass::PassThrough,
                false,
                false,
                Footstep::None,
            ),
            vec!["wall-face"],
        ),
        shipped(
            "greyport:window-dark",
            vec![],
            build_window_dark,
            contract(
                [1.3, 0.06],
                2.35,
                CollisionClass::PassThrough,
                false,
                false,
                Footstep::None,
            ),
            vec!["wall-face"],
        ),
        shipped(
            "debrief:desk",
            vec![],
            build_desk,
            contract(
                [1.6, 0.8],
                0.77,
                CollisionClass::Solid,
                false,
                false,
                Footstep::Wood,
            ),
            vec!["interior"],
        ),
        shipped(
            "debrief:chair",
            vec![],
            build_chair,
            contract(
                [0.5, 0.5],
                1.2,
                CollisionClass::Solid,
                false,
                false,
                Footstep::Wood,
            ),
            vec!["interior"],
        ),
    ]
}

/// The live registry: shipped props plus one load's mod overlays
/// and game-wide parameter replacements, in load order.
pub struct Registry {
    pub props: BTreeMap<String, PropDef>,
    /// Game-wide replacements: prop id -> replacement defaults.
    /// Applied under placement overrides, over shipped defaults.
    pub replacements: BTreeMap<String, ParamValues>,
}

impl Registry {
    /// The base registry: shipped props only.
    pub fn new() -> Registry {
        let mut props = BTreeMap::new();
        for def in shipped_props() {
            props.insert(def.id.clone(), def);
        }
        Registry {
            props,
            replacements: BTreeMap::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&PropDef> {
        self.props.get(id)
    }

    /// Insert a mod prop. Returns the displaced definition, if any
    /// (the caller reports it as a conflict — later wins, never
    /// silent, never merged).
    pub fn insert(&mut self, def: PropDef) -> Option<PropDef> {
        self.props.insert(def.id.clone(), def)
    }

    /// Apply a game-wide parameter replacement to a shipped prop.
    /// The caller validates the overrides against the prop's
    /// declared parameters first.
    pub fn replace(&mut self, id: &str, values: ParamValues) {
        self.replacements.insert(id.to_string(), values);
    }

    /// Build a prop's mesh: defaults -> game-wide replacement ->
    /// placement overrides. Every override is range-checked against
    /// the prop's declared parameters before it applies.
    pub fn build(
        &self,
        id: &str,
        overrides: &ParamValues,
    ) -> Result<BuiltProp, String> {
        let def = self
            .props
            .get(id)
            .ok_or_else(|| format!("unknown prop '{id}'"))?;
        let mut values = resolve_defaults(&def.params);
        if let Some(rep) = self.replacements.get(id) {
            for (k, v) in rep {
                values.insert(k.clone(), *v);
            }
        }
        for (k, v) in overrides {
            let param = def
                .params
                .get(k)
                .ok_or_else(|| format!("prop '{id}' has no parameter '{k}'"))?;
            param
                .check(v)
                .map_err(|e| format!("prop '{id}' parameter '{k}': {e}"))?;
            values.insert(k.clone(), *v);
        }
        // Replacements were validated when recorded; re-checking
        // here costs nothing and keeps the builder's precondition.
        for (k, v) in &values {
            if let Some(param) = def.params.get(k) {
                param
                    .check(v)
                    .map_err(|e| format!("prop '{id}' parameter '{k}': {e}"))?;
            }
        }
        let parts = match &def.source {
            Source::Built(builder) => builder(&values),
            Source::Recipe(recipe) => resolve_recipe(recipe, &values)?,
        };
        let mesh = mesh::build_mesh(&parts);
        let triangles = parts.iter().map(|p| p.primitive.triangles()).sum();
        Ok(BuiltProp {
            parts,
            mesh,
            triangles,
        })
    }

    /// Triangles at defaults (the budget, without overrides).
    pub fn default_triangles(&self, id: &str) -> Option<u64> {
        self.props.get(id).map(|d| d.triangles())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_table_builds_and_counts() {
        let reg = Registry::new();
        let expected: [(&str, u64); 12] = [
            ("greyport:lamp-post", 64),
            ("greyport:crate", 24),
            ("greyport:drum", 60),
            ("greyport:bollard", 60),
            ("greyport:three-bars-banner", 16),
            ("greyport:bench", 48),
            ("greyport:door", 48),
            ("greyport:roller-shutter", 24),
            ("greyport:window-lit", 52),
            ("greyport:window-dark", 52),
            ("debrief:desk", 72),
            ("debrief:chair", 72),
        ];
        for (id, tris) in expected {
            let built = reg.build(id, &ParamValues::new()).unwrap_or_else(|e| {
                panic!("shipped prop '{id}' failed to build: {e}")
            });
            assert_eq!(built.triangles, tris, "prop {id}");
            assert_eq!(built.mesh.indices.len() as u64, tris * 3, "mesh {id}");
            assert!(reg.get(id).unwrap().placement.is_some(), "contract {id}");
        }
    }

    #[test]
    fn lamp_height_override_moves_the_head() {
        let reg = Registry::new();
        let mut over = ParamValues::new();
        over.insert("height".to_string(), ParamValue::Num(4.2));
        let built = reg.build("greyport:lamp-post", &over).unwrap();
        // The head part sits at height + 0.09; at 4.2 it must be 4.29.
        let head_y = built
            .parts
            .iter()
            .map(|p| p.position[1])
            .fold(0.0f32, f32::max);
        assert!((head_y - 4.32).abs() < 0.01, "head top {head_y}");
    }

    #[test]
    fn out_of_range_override_is_refused() {
        let reg = Registry::new();
        let mut over = ParamValues::new();
        over.insert("height".to_string(), ParamValue::Num(9.0));
        assert!(reg.build("greyport:lamp-post", &over).is_err());
    }

    #[test]
    fn unknown_prop_is_refused() {
        let reg = Registry::new();
        assert!(reg.build("greyport:nope", &ParamValues::new()).is_err());
    }
}

