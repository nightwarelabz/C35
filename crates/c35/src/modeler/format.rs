//! The `.c35m` model file: engine-native, text, diff-able.
//!
//! Grammar (one construct per line, `#` comments, blank lines
//! ignored):
//!
//! ```text
//! c35-model 1
//! name greyport-lamp-standard
//! material 0 painted-metal
//! material 1 lamp-glass
//! v 0.170000 0.000000 0.170000
//! f 0 4 0:0.000000,1.000000 1:1.000000,1.000000 ...
//! f 1 3 5:0.500000,0.500000 ... n 0.000000 -1.000000 0.000000
//! ```
//!
//! - The header is `c35-model <version>`; version 1 is the only
//!   dialect. A file from a newer dialect is refused, not
//!   misread.
//! - `material <index> <name>` declares materials in index
//!   order (indices must arrive 0, 1, 2, ...). Names resolve at
//!   bake time; the file stores no atlas coordinates.
//! - `v <x> <y> <z>` appends a vertex.
//! - `f <material> <corner-count> <vert>:<u>,<v> ...` appends
//!   a face; a trailing `n <x> <y> <z>` pins its explicit
//!   normal.
//!
//! Numbers print with Rust's shortest round-trip float
//! formatting, so a parsed value is bit-identical to the value
//! that was saved, and save → load → save is byte-identical.
//! The round-trip test proves both on the proof asset itself.
//! Errors carry their line number, in the house style.

use super::mesh::{Corner, Face, Mesh};
use std::fmt;

/// The only dialect this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelError {
    /// 1-based line number, or 0 for whole-file problems.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ModelError {}

fn err(line: usize, message: impl Into<String>) -> ModelError {
    ModelError {
        line,
        message: message.into(),
    }
}

fn fmt_f(x: f32) -> String {
    // Rust's Display for f32 is the shortest string that parses
    // back to the identical value — the round-trip guarantee.
    format!("{x}")
}

fn parse_f(tok: &str, line: usize) -> Result<f32, ModelError> {
    let v: f32 = tok
        .parse()
        .map_err(|_| err(line, format!("bad number '{tok}'")))?;
    if !v.is_finite() {
        return Err(err(line, format!("non-finite number '{tok}'")));
    }
    Ok(v)
}

/// Serialize a mesh. Deterministic: the same mesh always
/// produces the same bytes.
pub fn save(mesh: &Mesh) -> String {
    let mut out = String::new();
    out.push_str(&format!("c35-model {FORMAT_VERSION}\n"));
    out.push_str(&format!("name {}\n", mesh.name));
    for (i, name) in mesh.materials.iter().enumerate() {
        out.push_str(&format!("material {i} {name}\n"));
    }
    for v in &mesh.verts {
        out.push_str(&format!(
            "v {} {} {}\n",
            fmt_f(v[0]),
            fmt_f(v[1]),
            fmt_f(v[2])
        ));
    }
    for f in &mesh.faces {
        let mut line = format!("f {} {}", f.material, f.corners.len());
        for c in &f.corners {
            line.push_str(&format!(" {}:{},{}", c.v, fmt_f(c.uv[0]), fmt_f(c.uv[1])));
        }
        if let Some(n) = f.normal {
            line.push_str(&format!(
                " n {} {} {}",
                fmt_f(n[0]),
                fmt_f(n[1]),
                fmt_f(n[2])
            ));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Parse a `.c35m` document. Strict: unknown directives,
/// out-of-order material indices, dangling vertex references,
/// and trailing junk are all errors with line numbers.
pub fn load(text: &str) -> Result<Mesh, ModelError> {
    let mut lines = text.lines().enumerate();
    // Header: the first non-blank, non-comment line.
    let mut mesh: Option<Mesh> = None;
    for (li, raw) in &mut lines {
        let line_no = li + 1;
        let s = raw.trim();
        if (s.is_empty() || s.starts_with('#')) && mesh.is_none() {
            continue;
        }
        if mesh.is_none() {
            let mut it = s.split_whitespace();
            match (it.next(), it.next(), it.next()) {
                (Some("c35-model"), Some(v), None) => {
                    let ver: u32 = v
                        .parse()
                        .map_err(|_| err(line_no, format!("bad version '{v}'")))?;
                    if ver != FORMAT_VERSION {
                        return Err(err(
                            line_no,
                            format!(
                                "unsupported c35-model version {ver} (this build reads {FORMAT_VERSION})"
                            ),
                        ));
                    }
                    mesh = Some(Mesh::new(""));
                    continue;
                }
                _ => return Err(err(line_no, "expected 'c35-model <version>' header")),
            }
        }
        let m = mesh.as_mut().unwrap();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let toks: Vec<&str> = s.split_whitespace().collect();
        match toks[0] {
            "name" => {
                // The name is the rest of the line, verbatim.
                m.name = s[4..].trim().to_string();
                if m.name.is_empty() {
                    return Err(err(line_no, "empty model name"));
                }
            }
            "material" => {
                if toks.len() != 3 {
                    return Err(err(line_no, "material wants: material <index> <name>"));
                }
                let idx: usize = toks[1]
                    .parse()
                    .map_err(|_| err(line_no, format!("bad material index '{}'", toks[1])))?;
                if idx != m.materials.len() {
                    return Err(err(
                        line_no,
                        format!(
                            "material index {idx} out of order (expected {})",
                            m.materials.len()
                        ),
                    ));
                }
                m.add_material(toks[2]);
            }
            "v" => {
                if toks.len() != 4 {
                    return Err(err(line_no, "v wants: v <x> <y> <z>"));
                }
                let p = [
                    parse_f(toks[1], line_no)?,
                    parse_f(toks[2], line_no)?,
                    parse_f(toks[3], line_no)?,
                ];
                m.push_vert(p);
            }
            "f" => {
                if toks.len() < 3 {
                    return Err(err(line_no, "f wants: f <material> <corners> ..."));
                }
                let mat: u16 = toks[1]
                    .parse()
                    .map_err(|_| err(line_no, format!("bad material index '{}'", toks[1])))?;
                if (mat as usize) >= m.materials.len() {
                    return Err(err(
                        line_no,
                        format!("face names undeclared material {mat}"),
                    ));
                }
                let count: usize = toks[2]
                    .parse()
                    .map_err(|_| err(line_no, format!("bad corner count '{}'", toks[2])))?;
                if count < 3 {
                    return Err(err(line_no, "a face needs at least 3 corners"));
                }
                if toks.len() < 3 + count {
                    return Err(err(line_no, "face has fewer corners than declared"));
                }
                let mut corners = Vec::with_capacity(count);
                for tok in &toks[3..3 + count] {
                    let (vtok, uvtok) = tok
                        .split_once(':')
                        .ok_or_else(|| err(line_no, format!("bad corner '{tok}'")))?;
                    let v: u32 = vtok
                        .parse()
                        .map_err(|_| err(line_no, format!("bad vertex index '{vtok}'")))?;
                    if (v as usize) >= m.verts.len() {
                        return Err(err(line_no, format!("corner names missing vertex {v}")));
                    }
                    let (utok, vtok2) = uvtok
                        .split_once(',')
                        .ok_or_else(|| err(line_no, format!("bad corner uv '{tok}'")))?;
                    corners.push(Corner {
                        v,
                        uv: [parse_f(utok, line_no)?, parse_f(vtok2, line_no)?],
                    });
                }
                let mut normal = None;
                let rest = &toks[3 + count..];
                if !rest.is_empty() {
                    if rest.len() == 4 && rest[0] == "n" {
                        normal = Some([
                            parse_f(rest[1], line_no)?,
                            parse_f(rest[2], line_no)?,
                            parse_f(rest[3], line_no)?,
                        ]);
                    } else {
                        return Err(err(line_no, "trailing junk after face corners"));
                    }
                }
                m.faces.push(Face {
                    corners,
                    material: mat,
                    normal,
                });
            }
            other => {
                return Err(err(line_no, format!("unknown directive '{other}'")));
            }
        }
    }
    mesh.ok_or_else(|| err(0, "empty file: no c35-model header"))
}

/// Save to a path. I/O failures are line-0 errors.
pub fn save_to(mesh: &Mesh, path: &std::path::Path) -> Result<(), ModelError> {
    std::fs::write(path, save(mesh)).map_err(|e| err(0, format!("writing {}: {e}", path.display())))
}

/// Load from a path.
pub fn load_from(path: &std::path::Path) -> Result<Mesh, ModelError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| err(0, format!("reading {}: {e}", path.display())))?;
    load(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modeler::assets;

    #[test]
    fn round_trip_is_byte_identical() {
        let m = assets::lamp_standard();
        let text1 = save(&m);
        let back = load(&text1).unwrap();
        assert_eq!(back, m, "parsed mesh differs from the source");
        let text2 = save(&back);
        assert_eq!(text1, text2, "re-save differs byte for byte");
    }

    #[test]
    fn explicit_normal_round_trips() {
        let mut m = Mesh::new("n");
        let mat = m.add_material("painted-metal");
        let a = m.push_vert([0.0, 0.0, 0.0]);
        let b = m.push_vert([1.0, 0.0, 0.0]);
        let c = m.push_vert([0.0, 1.0, 0.0]);
        let f = m.push_face(&[a, b, c], mat);
        m.faces[f].normal = Some([0.0, 0.0, 1.0]);
        let back = load(&save(&m)).unwrap();
        assert_eq!(back.faces[0].normal, Some([0.0, 0.0, 1.0]));
    }

    #[test]
    fn wrong_version_is_refused_with_a_line() {
        let e = load("c35-model 99\nname x\n").unwrap_err();
        assert_eq!(e.line, 1);
        assert!(e.message.contains("99"), "{}", e.message);
    }

    #[test]
    fn dangling_vertex_is_refused_with_a_line() {
        let text = "c35-model 1\nname x\nmaterial 0 painted-metal\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 3 0:0,0 1:1,0 7:0,1\n";
        let e = load(text).unwrap_err();
        assert_eq!(e.line, 7);
    }

    #[test]
    fn unknown_directive_is_refused() {
        let e = load("c35-model 1\nname x\nspin 45\n").unwrap_err();
        assert_eq!(e.line, 3);
    }
}
