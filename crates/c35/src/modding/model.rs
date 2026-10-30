//! The mod system's data model: prop recipes, parameters, placement
//! contracts, and the material registry.
//!
//! One rule governs this file: **a mod never addresses atlas
//! coordinates.** Materials are names. The engine resolves a name to
//! a painted region + shader flag internally, through
//! [`material_region`]. A mod file naming a material that is not in
//! the registry is a validation error, not a fallback — silent
//! fallbacks teach modders the wrong names.
//!
//! Reconciliation with the landed beauty regime (PR #13, the
//! solidity pass): mod props paint through the SAME city atlas the
//! base game uses, so they inherit the landed grade exactly —
//! softened region interiors, 2 px joints, per-brick tone drift.
//! No new atlas regions are allocated in v1 (see below), so there is
//! nothing to reconcile on the painter side: a mod prop cannot look
//! off-grade, because it passes through the same painters.
//!
//! Atlas allocation, honestly stated: the spec's §7 asks for dynamic
//! atlas allocation that reserves mod space. v1 does not build it,
//! because v1 mods define no new surfaces — every material name
//! resolves to a shipped region. Atlas share for a v1 mod prop is
//! therefore 0 new tiles by construction, and the budget verifier
//! enforces it (a declared atlas share other than 0 is an error).
//! Dynamic allocation arrives with the first phase that lets a mod
//! define a new surface; it can be added without breaking a single
//! v1 mod file, because no v1 file addresses coordinates.

use crate::props::{self, region};
use std::collections::BTreeMap;

/// The frozen v1 part vocabulary. This is the whole language: the
/// renderer pass freezes these four, and a part naming anything else
/// is a validation error (spec §8's fallback is a versioned
/// vocabulary, not an open one).
///
/// - `box`: an axis-aligned box, 12 tris.
/// - `wedge`: a triangular prism (right-triangle cross-section in
///   the local XY plane, extruded along Z), 8 tris.
/// - `cylinder`: an 8-sided prism along local Y, capped, 30 tris.
/// - `quad`: a flat panel, emitted double-sided (both windings),
///   4 tris — banners and signs need both faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    Box,
    Wedge,
    Cylinder,
    Quad,
}

impl Primitive {
    pub fn from_name(name: &str) -> Option<Primitive> {
        match name {
            "box" => Some(Primitive::Box),
            "wedge" => Some(Primitive::Wedge),
            "cylinder" => Some(Primitive::Cylinder),
            "quad" => Some(Primitive::Quad),
            _ => None,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Primitive::Box => "box",
            Primitive::Wedge => "wedge",
            Primitive::Cylinder => "cylinder",
            Primitive::Quad => "quad",
        }
    }
    /// Triangles one part of this primitive emits, always.
    pub fn triangles(&self) -> u64 {
        match self {
            Primitive::Box => 12,
            Primitive::Wedge => 8,
            Primitive::Cylinder => 30,
            Primitive::Quad => 4,
        }
    }
}

/// The closed list of part primitives, for error messages.
pub const PRIMITIVES: [&str; 4] = ["box", "wedge", "cylinder", "quad"];

/// A number in a recipe, or a reference to one of the prop's
/// parameters (`"$height"`). Direct substitution only — no
/// expressions. A shipped prop that needs real parametric behaviour
/// (a lamp whose pole grows with its height) is a Rust builder in
/// the registry, not a JSON recipe.
#[derive(Debug, Clone)]
pub enum NumOrParam {
    Num(f32),
    Param(String),
}

/// A parameter value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    Num(f32),
    Bool(bool),
}

/// A declared parameter: its default and, for numbers, its legal
/// range. An override outside the range is a validation error, never
/// a silent clamp.
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub default: ParamValue,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

impl ParamDef {
    pub fn num(default: f32, min: f32, max: f32) -> ParamDef {
        ParamDef {
            default: ParamValue::Num(default),
            min: Some(min),
            max: Some(max),
        }
    }
    pub fn flag(default: bool) -> ParamDef {
        ParamDef {
            default: ParamValue::Bool(default),
            min: None,
            max: None,
        }
    }
    /// Check an override value against this definition.
    pub fn check(&self, value: &ParamValue) -> Result<(), String> {
        match (self.default, value) {
            (ParamValue::Num(_), ParamValue::Num(n)) => {
                if !n.is_finite() {
                    return Err("parameter value must be finite".to_string());
                }
                if let Some(min) = self.min {
                    if *n < min {
                        return Err(format!(
                            "parameter value {n} is below the legal minimum {min}"
                        ));
                    }
                }
                if let Some(max) = self.max {
                    if *n > max {
                        return Err(format!(
                            "parameter value {n} is above the legal maximum {max}"
                        ));
                    }
                }
                Ok(())
            }
            (ParamValue::Bool(_), ParamValue::Bool(_)) => Ok(()),
            (ParamValue::Num(_), ParamValue::Bool(_)) => {
                Err("parameter expects a number, got a boolean".to_string())
            }
            (ParamValue::Bool(_), ParamValue::Num(_)) => {
                Err("parameter expects a boolean, got a number".to_string())
            }
        }
    }
}

/// A map of parameter values (defaults applied, overrides checked).
pub type ParamValues = BTreeMap<String, ParamValue>;

/// One part of a prop recipe, before resolution.
#[derive(Debug, Clone)]
pub struct PartRecipe {
    pub primitive: Primitive,
    /// Centre of the part in prop-local space (metres).
    pub position: [NumOrParam; 3],
    /// Size of the part (metres). Box: full extents. Wedge: extents
    /// of the bounding box. Cylinder: (diameter_x, height,
    /// diameter_z). Quad: (width, height, ignored).
    pub size: [NumOrParam; 3],
    /// Euler rotation in degrees, applied as Rz * Ry * Rx.
    pub rotation: [NumOrParam; 3],
    /// Material name from the registry (never atlas coordinates).
    pub material: String,
}

/// A part after parameter resolution: plain numbers, ready to emit.
#[derive(Debug, Clone)]
pub struct Part {
    pub primitive: Primitive,
    pub position: [f32; 3],
    pub size: [f32; 3],
    pub rotation: [f32; 3],
    /// Resolved atlas region + shader flag.
    pub region: (u32, u32, u32, u32),
    pub mat_flag: f32,
}

/// Collision classes for the placement contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionClass {
    /// Blocks movement.
    Solid,
    /// Walked through (banners, windows, dressing).
    PassThrough,
    /// Can be climbed (crates stacked as steps, low walls).
    Climbable,
}

impl CollisionClass {
    pub fn from_name(name: &str) -> Option<CollisionClass> {
        match name {
            "solid" => Some(CollisionClass::Solid),
            "pass-through" => Some(CollisionClass::PassThrough),
            "climbable" => Some(CollisionClass::Climbable),
            _ => None,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            CollisionClass::Solid => "solid",
            CollisionClass::PassThrough => "pass-through",
            CollisionClass::Climbable => "climbable",
        }
    }
}

/// Footstep surfaces: what sound the animator's events price when a
/// foot lands on the prop. `None` for props a foot never lands on
/// (wall-hung banners, windows, doors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Footstep {
    Wood,
    Metal,
    Concrete,
    WetStone,
    None,
}

impl Footstep {
    pub fn from_name(name: &str) -> Option<Footstep> {
        match name {
            "wood" => Some(Footstep::Wood),
            "metal" => Some(Footstep::Metal),
            "concrete" => Some(Footstep::Concrete),
            "wet-stone" => Some(Footstep::WetStone),
            "none" => Some(Footstep::None),
            _ => None,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Footstep::Wood => "wood",
            Footstep::Metal => "metal",
            Footstep::Concrete => "concrete",
            Footstep::WetStone => "wet-stone",
            Footstep::None => "none",
        }
    }
}

/// The placement contract: the prop's physical promise to the world.
/// A prop that omits placement is dressing only — no collision, no
/// anchor — and the validator says so in its report rather than
/// guessing.
#[derive(Debug, Clone)]
pub struct PlacementContract {
    /// Footprint in metres (width x depth), axis-aligned at yaw 0.
    pub footprint: [f32; 2],
    /// Height in metres at default parameters.
    pub height: f32,
    pub collision: CollisionClass,
    /// Whether the prop may serve as a grapple anchor.
    pub grapple_anchor: bool,
    /// Whether the prop occludes the light field.
    pub occluder: bool,
    pub footstep: Footstep,
}

/// Placement tags: the closed vocabulary a placement rule may use.
/// A prop tagged `wall-face` can be hung on a wall run; a prop
/// tagged `street` cannot, and the validator says so.
pub const TAGS: [&str; 6] = ["street", "kerb", "wall-face", "rooftop", "interior", "dockside"];

pub fn valid_tag(tag: &str) -> bool {
    TAGS.contains(&tag)
}

/// A shipped prop's builder: resolved parameter values in, parts
/// out. Builders assume their inputs were range-checked against the
/// declared `params` before the call.
pub type ShippedBuilder = fn(&ParamValues) -> Vec<Part>;

/// Where a prop definition's parts come from.
#[derive(Clone)]
pub enum Source {
    /// A Rust builder (shipped props with real parametric behaviour,
    /// e.g. a lamp whose pole grows with its height).
    Built(ShippedBuilder),
    /// A JSON recipe (mod props): `$param` references resolve
    /// against the prop's declared parameters.
    Recipe(Vec<PartRecipe>),
}

/// A prop definition: recipe + parameters + placement + tags.
/// Shipped props and mod props share this shape; the only difference
/// is where the parts come from (a Rust builder for shipped props
/// with real parametric behaviour, a JSON recipe for mod props).
#[derive(Clone)]
pub struct PropDef {
    /// Namespaced id, e.g. `greyport:lamp-post`.
    pub id: String,
    /// The mod that defined it, or `None` for shipped props.
    pub source_mod: Option<String>,
    /// Declared parameters (name -> definition).
    pub params: BTreeMap<String, ParamDef>,
    /// Parts at default parameters (built once at registration for
    /// shipped props, resolved once at load for mod props). The
    /// budget reads this; builds with overrides re-run the source.
    pub parts: Vec<Part>,
    /// The source the parts came from, for rebuilds with overrides.
    pub source: Source,
    pub placement: Option<PlacementContract>,
    pub tags: Vec<String>,
}

impl PropDef {
    /// Triangles at default parameters — the ledger's budget.
    pub fn triangles(&self) -> u64 {
        self.parts.iter().map(|p| p.primitive.triangles()).sum()
    }
    /// Atlas share in v1: always 0 new tiles (reuse only).
    pub fn atlas_tiles(&self) -> u64 {
        0
    }
}

/// A material name resolved to its painted region + shader flag.
/// Names are the registry; coordinates never leave this function.
pub fn material_region(name: &str) -> Option<((u32, u32, u32, u32), f32)> {
    use props::{MAT_EMISSIVE, MAT_STANDARD, MAT_WET};
    let (r, m): ((u32, u32, u32, u32), f32) = match name {
        "brick" => (region::BRICK, MAT_STANDARD),
        "brick-dark" => (region::BRICK_DARK, MAT_STANDARD),
        "stone" => (region::STONE, MAT_STANDARD),
        "concrete" => (region::CONCRETE, MAT_STANDARD),
        "asphalt" => (region::ASPHALT, MAT_STANDARD),
        "roller-shutter" => (region::ROLLER, MAT_STANDARD),
        "banner" => (region::BANNER, MAT_STANDARD),
        "door" => (region::DOOR, MAT_STANDARD),
        "window-lit" => (region::WINDOW_LIT, MAT_EMISSIVE),
        "window-dark" => (region::WINDOW_DARK, MAT_STANDARD),
        "lamp-glass" => (region::LAMP_GLASS, MAT_EMISSIVE),
        "sodium-lamp-glass" => (region::LAMP_GLASS, MAT_EMISSIVE),
        "painted-metal" => (region::METAL_DARK, MAT_STANDARD),
        "roof-tar" => (region::ROOF, MAT_STANDARD),
        "timber" => (region::TIMBER, MAT_STANDARD),
        "wet-streak" => (region::STREAK, MAT_EMISSIVE),
        // Semantic wet aliases: the same painted ground, flagged so
        // the wet pass computes view-dependent lamp specular.
        "wet-concrete" => (region::CONCRETE, MAT_WET),
        "wet-asphalt" => (region::ASPHALT, MAT_WET),
        _ => return None,
    };
    Some((r, m))
}

/// The closed material list, for error messages and docs.
pub const MATERIALS: [&str; 18] = [
    "brick",
    "brick-dark",
    "stone",
    "concrete",
    "asphalt",
    "roller-shutter",
    "banner",
    "door",
    "window-lit",
    "window-dark",
    "lamp-glass",
    "sodium-lamp-glass",
    "painted-metal",
    "roof-tar",
    "timber",
    "wet-streak",
    "wet-concrete",
    "wet-asphalt",
];

/// Validate a namespaced id: `namespace:name`, each side
/// `[a-z0-9][a-z0-9_-]*`, exactly one colon.
pub fn valid_id(id: &str) -> bool {
    let mut parts = id.split(':');
    let ns = match parts.next() {
        Some(s) => s,
        None => return false,
    };
    let name = match parts.next() {
        Some(s) => s,
        None => return false,
    };
    if parts.next().is_some() {
        return false;
    }
    fn side_ok(s: &str) -> bool {
        !s.is_empty()
            && s.chars().next().map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                .unwrap_or(false)
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    }
    side_ok(ns) && side_ok(name)
}

/// Validate a bare namespace: `[a-z0-9][a-z0-9_-]*`. A manifest
/// `id` IS a namespace (`my-street`), not a `namespace:name` id —
/// the namespace is what prop ids are checked against.
pub fn valid_namespace(ns: &str) -> bool {
    !ns.is_empty()
        && ns.chars()
            .next()
            .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            .unwrap_or(false)
        && ns.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}
pub fn split_id(id: &str) -> Option<(&str, &str)> {
    let (ns, name) = id.split_once(':')?;
    if valid_id(id) {
        Some((ns, name))
    } else {
        None
    }
}

/// Namespaces the engine owns. A mod claiming one is refused.
pub const RESERVED_NAMESPACES: [&str; 2] = ["greyport", "debrief"];

