//! The mod validator's command line: the `debrief --check-mod`
//! check path, engine side.
//!
//! Single-folder check:
//! ```sh
//! cargo run -p c35 --example check_mod -- crates/c35/example-mods/example-street
//! ```
//! Exit 0 when the mod is valid, 1 when it is not. The report
//! prints errors with file:line locations, or the valid line with
//! counts.
//!
//! Multi-mod load (with an anchor table for anchor placements):
//! ```sh
//! cargo run -p c35 --example check_mod -- --load mods/a mods/b \
//!   --anchor mercer-street:east-kerb:6:0:20
//! ```
//! Anchor syntax is `name:x:y:z:yaw` (the name may itself contain
//! colons; the last four fields are the numbers). The load report
//! prints load order, conflicts, quarantines, dropped placements,
//! placed instances, and the ledger line. Quarantines are normal
//! outcomes, not failures, so load mode always exits 0 on a
//! well-formed invocation.

use c35::modding::{check_mod_folder, load_mods};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> ! {
    eprintln!("usage:");
    eprintln!("  check_mod <folder>");
    eprintln!("  check_mod --load <folder>... [--anchor name:x:y:yaw ...]");
    std::process::exit(2);
}

fn parse_anchor(spec: &str) -> Option<(String, ([f32; 3], f32))> {
    let mut parts: Vec<&str> = spec.split(':').collect();
    if parts.len() < 5 {
        return None;
    }
    let yaw: f32 = parts.pop()?.parse().ok()?;
    let z: f32 = parts.pop()?.parse().ok()?;
    let y: f32 = parts.pop()?.parse().ok()?;
    let x: f32 = parts.pop()?.parse().ok()?;
    let name = parts.join(":");
    if name.is_empty() {
        return None;
    }
    Some((name, ([x, y, z], yaw)))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    if args[0] == "--load" {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut anchors: BTreeMap<String, ([f32; 3], f32)> = BTreeMap::new();
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--anchor" => {
                    i += 1;
                    let spec = args.get(i).unwrap_or_else(|| usage());
                    match parse_anchor(spec) {
                        Some((name, pos)) => {
                            anchors.insert(name, pos);
                        }
                        None => {
                            eprintln!("bad --anchor spec '{spec}' (want name:x:y:z:yaw)");
                            return ExitCode::from(2);
                        }
                    }
                }
                other => dirs.push(PathBuf::from(other)),
            }
            i += 1;
        }
        if dirs.is_empty() {
            usage();
        }
        let report = load_mods(&dirs, &anchors);
        println!("load order: {}", report.order.join(", "));
        println!("modded: {}", report.modded);
        println!("ledger: {}", report.ledger_line());
        for c in &report.conflicts {
            println!("conflict: {c}");
        }
        for q in &report.quarantined {
            println!("quarantined: {}", q.folder_name);
            for r in &q.reasons {
                println!("  reason: {r}");
            }
        }
        for d in &report.dropped_placements {
            println!("dropped: {d}");
        }
        for p in &report.placed {
            println!(
                "placed: {} at [{:.2}, {:.2}, {:.2}] yaw {:.1} ({} tris, from {})",
                p.prop, p.position[0], p.position[1], p.position[2], p.yaw, p.triangles, p.from
            );
        }
        println!("placed triangles: {}", report.placed_triangles);
        for w in &report.warnings {
            println!("warning: {w}");
        }
        ExitCode::SUCCESS
    } else {
        if args.len() != 1 {
            usage();
        }
        let report = check_mod_folder(&PathBuf::from(&args[0]));
        print!("{}", report.render());
        if report.valid {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        }
    }
}

