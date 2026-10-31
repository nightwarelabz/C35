//! Assets authored WITH the modeler, shipped as build scripts.
//!
//! These are the modeler's proof and its first production
//! shelf: a build script stands where a modeling session will
//! sit — the ops below are the session transcript, replayable
//! and diff-able, and the mesh it produces is plain data the
//! file format round-trips.

use super::mesh::Mesh;

/// The Greyport lamp standard, modeled — the proof asset.
///
/// Why this piece: the shipped recipe lamp (`greyport:lamp-post`)
/// is six boxes — a square pole, a straight horizontal arm box,
/// a head made of stacked slabs. A real Greyport standard has a
/// SWAN NECK: the arm leaves the pole vertically and curves over
/// the street. No arrangement of axis boxes expresses that curve;
/// it is exactly the class of silhouette the owner ordered the
/// modeler for. The A/B is one variable — same spot, same
/// camera, same night — so the curve is the whole difference.
///
/// The build, as a session transcript:
/// - a bevelled plinth box with a tapered socket extruded up;
/// - an octagonal pole (prism) with two collar rings;
/// - the neck: five extrusions of the pole's cap, each turning
///   a little further over, tapering as it goes;
/// - a short neck extrusion flaring into a two-step bell shade;
/// - the shade's rim bevelled in metal, its cap glassed.
pub fn lamp_standard() -> Mesh {
    let mut m = Mesh::new("greyport-lamp-standard");
    let metal = m.add_material("painted-metal");
    let glass = m.add_material("lamp-glass");
    let phase = std::f32::consts::FRAC_PI_8;

    // Plinth: bevelled box, socket extruded and tapered.
    let plinth = m.add_box([-0.17, 0.0, -0.17], [0.17, 0.34, 0.17], metal);
    let plinth_top = plinth[2]; // +Y in add_box's face order
    m.bevel_faces(&[plinth_top], 0.035, 1)
        .expect("plinth bevel");
    m.extrude_faces(&[plinth_top], [0.0, 0.10, 0.0], 0.30)
        .expect("plinth socket");

    // Pole with two collar rings (separate shells, overlapping
    // the pole — the house building style).
    let pole = m.add_prism([0.0, 0.0], 0.062, 0.30, 5.30, 8, phase, metal);
    m.add_prism([0.0, 0.0], 0.082, 0.52, 0.66, 8, phase, metal);
    m.add_prism([0.0, 0.0], 0.078, 4.86, 4.98, 8, phase, metal);

    // The swan neck: the cap walks up and over the street.
    let cap = pole.top;
    let neck: [([f32; 3], f32); 5] = [
        ([0.10, 0.34, 0.0], 0.05),
        ([0.22, 0.27, 0.0], 0.05),
        ([0.30, 0.16, 0.0], 0.05),
        ([0.33, 0.04, 0.0], 0.05),
        ([0.29, -0.09, 0.0], 0.05),
    ];
    for (off, taper) in neck {
        m.extrude_faces(&[cap], off, taper).expect("neck segment");
    }
    // Head: a short neck, then the bell shade in two flares.
    // Each extrusion re-orients its ring to face its offset, so
    // these directions are also the shade's aim: it ends facing
    // down and a touch streetward, the way a lamp throws light.
    m.extrude_faces(&[cap], [0.16, -0.06, 0.0], -0.20)
        .expect("head neck");
    m.extrude_faces(&[cap], [0.10, -0.10, 0.0], -1.60)
        .expect("shade flare");
    m.extrude_faces(&[cap], [0.02, -0.09, 0.0], -0.55)
        .expect("shade rim step");
    // The rim bevels in metal; only then does the cap become
    // glass, centred on the glass region's hot core (planar
    // projection is world-based, so the offset centres the
    // cap's own centroid in the region).
    m.bevel_faces(&[cap], 0.025, 1).expect("rim bevel");
    m.faces[cap].material = glass;
    let n = m.face_normal(cap);
    let c = m.face_centroid(cap);
    m.project_planar(&[cap], n, 2.0, [0.5 - c[0] * 2.0, 0.5 - c[2] * 2.0])
        .expect("glass uv");

    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lamp_standard_is_watertight() {
        let m = lamp_standard();
        let faults = m.validate();
        assert!(faults.is_empty(), "faults: {faults:?}");
    }

    #[test]
    fn lamp_standard_has_one_glass_face_over_the_street() {
        let m = lamp_standard();
        let glass_idx = m.materials.iter().position(|x| x == "lamp-glass").unwrap() as u16;
        let glass: Vec<usize> = (0..m.faces.len())
            .filter(|&fi| m.faces[fi].material == glass_idx)
            .collect();
        assert_eq!(glass.len(), 1);
        let c = m.face_centroid(glass[0]);
        // The glass hangs high and out over the street (+X),
        // where the neck carried it — not atop the pole.
        assert!(c[1] > 5.0 && c[0] > 1.0, "glass centroid {c:?}");
        // And it faces down-street, not up: the shade's whole
        // point is throwing light down.
        let n = m.face_normal(glass[0]);
        assert!(n[1] < -0.5, "glass normal {n:?}");
    }

    #[test]
    fn lamp_standard_outweighs_the_primitive() {
        // The shipped recipe lamp is 64 triangles (registry
        // census). The modeled standard must carry visibly more
        // geometry — the curve is made of faces.
        let m = lamp_standard();
        assert!(m.triangle_count() > 64, "tris {}", m.triangle_count());
    }
}
