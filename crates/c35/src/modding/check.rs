//! The validator's report path: what `debrief --check-mod` prints.
//!
//! Single-folder only, by design: the check must agree with load,
//! and load-time anchor resolution belongs to the city, not the
//! check. Anchor placements are counted and noted as resolving at
//! load; absolute positions are warned as fragile; dressing-only
//! props (no placement contract) are noted, not guessed at.
//!
//! The game binary's `--check-mod` flag (DeBrief repo, game side)
//! is a thin wrapper over [`check_mod_folder`]: same function, same
//! report, same exit contract (0 valid, 1 invalid).

use crate::modding::loader::{load_mod_folder, Target};
use std::path::Path;

/// The validator's verdict for one mod folder.
pub struct CheckReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub props: usize,
    pub placements: usize,
    pub triangles: u64,
}

impl CheckReport {
    /// The report text: errors with file:line locations, or the
    /// valid line with counts — the loop a modder iterates on.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if self.valid {
            out.push_str(&format!(
                "mod valid: {} props, {} placements, {} triangles, 0 new atlas tiles\n",
                self.props, self.placements, self.triangles
            ));
            out.push_str(
                "(v1 reuses shipped atlas regions; a mod prop adds no new tiles by construction)\n",
            );
        } else {
            out.push_str("mod INVALID:\n");
            for e in &self.errors {
                out.push_str(&format!("  error: {e}\n"));
            }
        }
        for w in &self.warnings {
            out.push_str(&format!("  warning: {w}\n"));
        }
        out
    }
}

/// Validate one mod folder. Anchor placements are structurally
/// valid here; their resolution against the city's anchor table
/// happens at load, and is noted in the warnings.
pub fn check_mod_folder(dir: &Path) -> CheckReport {
    match load_mod_folder(dir) {
        Ok(m) => {
            let mut warnings = m.warnings.clone();
            for entry in &m.placements {
                if let Target::Anchor(name) = &entry.target {
                    warnings.push(format!(
                        "placement of '{}' uses anchor '{name}'; it resolves at load against the city's anchor table",
                        entry.prop
                    ));
                }
            }
            let triangles: u64 = m.props.iter().map(|p| p.triangles()).sum();
            CheckReport {
                valid: true,
                errors: Vec::new(),
                warnings,
                props: m.props.len(),
                placements: m.placements.len(),
                triangles,
            }
        }
        Err(q) => CheckReport {
            valid: false,
            errors: q.reasons,
            warnings: Vec::new(),
            props: 0,
            placements: 0,
            triangles: 0,
        },
    }
}

