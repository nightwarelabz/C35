//! Op demo: one full op lifecycle, printed end to
//! end — brief, two objectives complete, one failed,
//! alert tier risen mid-op, debrief grade with a
//! per-factor breakdown, and a debrief record filed
//! into a snapshot journal and then into the Archive.
//!
//! Run: cargo run -q -p c35 --example op_demo
//!
//! The band word at the end is DEMO POLICY ONLY —
//! the engine assigns no band or rank; what a score
//! means for progression is the host's decision.

use c35::op::{grade, DebriefRecord, GradeWeights, OpOutcome, OpRecord, OpTick};
use c35::persist::model::Snapshot;

fn main() {
    // Brief: open the op and define its objectives.
    // Kinds are host words; the engine never reads
    // them as instructions.
    let mut op = OpRecord::open("op.greyport.07", "Cross the checkpoint");
    op.objectives
        .add("obj.cross", "cross", "Cross the checkpoint")
        .unwrap();
    op.objectives
        .add("obj.retrieve", "retrieve", "Retrieve the ledger")
        .unwrap();
    op.objectives
        .add("obj.observe", "observe", "Observe the patrol")
        .unwrap();
    println!("op {} opened: {:?}", op.id, op.state);

    // Active: begin, then work the objectives.
    op.begin().unwrap();
    op.objectives.activate("obj.cross").unwrap();
    op.objectives.activate("obj.retrieve").unwrap();
    op.objectives.activate("obj.observe").unwrap();
    op.objectives.complete("obj.cross").unwrap();
    op.objectives.complete("obj.retrieve").unwrap();
    op.objectives.fail("obj.observe").unwrap();

    // Ticks: the host reports its own readings. The
    // third tick crosses both the Weathervane and the
    // Type 2 thresholds upward — reported as facts,
    // never acted on here.
    let ticks = [
        OpTick {
            dt_s: 120.0,
            alert_level: 8.0,
            alert_tier: 0,
            descriptions_filed: 0,
            witness_price: 0.0,
        },
        OpTick {
            dt_s: 240.0,
            alert_level: 30.0,
            alert_tier: 1,
            descriptions_filed: 1,
            witness_price: 1.0,
        },
        OpTick {
            dt_s: 180.0,
            alert_level: 78.0,
            alert_tier: 2,
            descriptions_filed: 2,
            witness_price: 4.0,
        },
    ];
    for t in ticks {
        op.tick(t).unwrap();
        let e = op.escalation();
        println!(
            "tick: level {:.0} tier {} | patrol tier {} type2 available {} crossed patrol {} weathervane {} type2 {}",
            op.alert_level, op.alert_tier, e.patrol_tier, e.type2_available,
            e.crossed_patrol, e.crossed_weathervane, e.crossed_type2
        );
    }
    op.set_casualties(0);
    println!(
        "objectives: {:?} | elapsed {:.0}s | peak level {:.0}",
        op.objectives.counts(),
        op.elapsed_s,
        op.peak_level
    );

    // Close: the host declares the outcome. Failed
    // objectives do not auto-fail the op — the host
    // closed it Completed with one objective failed.
    op.close(OpOutcome::Completed).unwrap();
    println!("op closed: {:?}", op.state);
    match op.tick(ticks[0]) {
        Ok(()) => println!("tick after close: ACCEPTED (bug)"),
        Err(_) => println!("tick after close: refused (terminal is final)"),
    }

    // Grade: explicit host weights, every factor shown
    // even at zero contribution.
    let weights = GradeWeights {
        per_objective_complete: 10.0,
        per_objective_failed: -15.0,
        per_objective_unclosed: -5.0,
        per_alert_tier: -8.0,
        per_description: -4.0,
        per_witness_price: -2.0,
        per_casualty: -20.0,
        per_second: -0.01,
    };
    let g = grade(&op, weights);
    println!("grade score: {:.2}", g.score);
    for f in &g.factors {
        println!(
            "  factor {:<22} {:>8.2} x {:>7.2} = {:>8.2}",
            f.name, f.quantity, f.weight, f.contribution
        );
    }
    // Demo policy only, not engine truth.
    let band = if g.score >= 0.0 { "clean" } else { "costly" };
    println!("demo band (host policy, not engine): {}", band);

    // Debrief record: freeze, print, and file into a
    // snapshot journal, then file the journal into
    // the Archive — the persist seam, no glue.
    let record = DebriefRecord::from_op(&op, g);
    let mut snap = Snapshot::new(0xC35_0001, 100, "agent");
    let mut seq: u64 = 1;
    let pushed = record.file_into(&mut snap, 100, &mut seq);
    let filed = snap.file_journal(100);
    println!("debrief lines pushed: {} filed: {}", pushed, filed);
    for a in snap.archive.iter() {
        println!(
            "  archive [{} career {}]: {}",
            a.filed_day, a.career, a.text
        );
    }
}
