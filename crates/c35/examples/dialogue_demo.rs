//! Dialogue demo: two small host-data conversations,
//! printed end to end — linear advance with More /
//! End cues, a choice point, an option gated by a
//! number fact (hidden until the host raises the
//! fact), a gated line skipped while its bool fact
//! is unset and shown once the host sets it, and
//! the refusals (start over an unfinished
//! conversation, choose outside the presented list,
//! advance after the finish).
//!
//! Run: cargo run -q -p c35 --example dialogue_demo
//!
//! The conversations below are DEMO DATA ONLY — a
//! generic exchange written for this demo. The
//! engine bakes in no story content; conversations
//! are host data, and none of this is canon.

use c35::dialogue::{ChoiceOption, Current, Dialogue, FactValue, Gate, Line};

fn show(tag: &str, cur: &Current) {
    println!(
        "{} [{}] {} {} (cue {:?})",
        tag, cur.id, cur.speaker, cur.text, cur.cue
    );
    for (i, opt) in cur.options.iter().enumerate() {
        println!("    {}. {}", i, opt);
    }
}

fn main() {
    let mut dlg = Dialogue::new();

    // Facts are the host's: a clearance level
    // (number) and a trust flag (bool). The engine
    // only compares them against gates.
    dlg.set_fact("clearance", FactValue::Number(1.0));
    println!(
        "facts: clearance = {:?}, trusted = {:?}",
        dlg.fact("clearance"),
        dlg.fact("trusted")
    );

    // Conversation 1: host data. The side-gate
    // option needs clearance >= 2; the note line
    // needs trusted == true.
    dlg.start(vec![
        Line::new("open", "CONTROL", "Check-in. Report when ready."),
        Line::new("status", "FIELD", "In position. The street is quiet.").choices(vec![
            ChoiceOption::new("Ask about the patrol", "patrol"),
            ChoiceOption::gated(
                "Ask about the side gate",
                "gate",
                Gate::at_least("clearance", 2.0),
            ),
            ChoiceOption::new("Sign off", "signoff"),
        ]),
        Line::new("patrol", "CONTROL", "The patrol passes every ten minutes.").then("note"),
        Line::new("gate", "CONTROL", "The side gate sticks after rain.").then("note"),
        Line::new("note", "CONTROL", "One more thing: the ledger page is torn.")
            .gated(Gate::flag("trusted", true))
            .then("wrap"),
        Line::new("wrap", "FIELD", "Copy. Moving on."),
        Line::new("signoff", "CONTROL", "Acknowledged. Out."),
    ])
    .unwrap();
    show("start:", &dlg.current().unwrap());

    // Refusal: a second conversation over an
    // unfinished one.
    match dlg.start(vec![Line::new("x", "X", "x")]) {
        Ok(()) => println!("start over active: NOT refused (unexpected)"),
        Err(e) => println!("start over active refused: {:?}", e),
    }

    // Linear advance into the choice point. At
    // clearance 1 the side-gate option is gated
    // off, so only two options are presented.
    dlg.advance().unwrap();
    show("advance:", &dlg.current().unwrap());

    // Refusal: an index outside the presented list.
    match dlg.choose(9) {
        Ok(()) => println!("choose(9): NOT refused (unexpected)"),
        Err(e) => println!("choose(9) refused: {:?}", e),
    }

    // The host raises clearance; the gated option
    // appears in the same line's presentation.
    dlg.set_fact("clearance", FactValue::Number(2.0));
    show("clearance 2:", &dlg.current().unwrap());

    // Take the newly available branch: index 1 in
    // the presented order.
    dlg.choose(1).unwrap();
    show("choose(1):", &dlg.current().unwrap());

    // Advance: the note line's gate (trusted) is
    // unset, so flow skips it and lands on wrap.
    dlg.advance().unwrap();
    show("advance (note skipped):", &dlg.current().unwrap());

    // Advance to the last line (End cue), then past
    // it: the conversation finishes.
    dlg.advance().unwrap();
    show("advance:", &dlg.current().unwrap());
    dlg.advance().unwrap();
    println!("is_finished: {}", dlg.is_finished());
    println!("current after finish: {:?}", dlg.current().map(|c| c.id));

    // Refusal: advancing a finished conversation.
    match dlg.advance() {
        Ok(()) => println!("advance after finish: NOT refused (unexpected)"),
        Err(e) => println!("advance after finish refused: {:?}", e),
    }

    // Conversation 2: the session may start again
    // once the previous conversation finished.
    // With trusted now set, the gated line shows.
    dlg.set_fact("trusted", FactValue::Bool(true));
    dlg.start(vec![
        Line::new("again", "CONTROL", "One question remains."),
        Line::new("detail", "CONTROL", "The ledger page was torn out cleanly.")
            .gated(Gate::flag("trusted", true)),
        Line::new("done", "CONTROL", "That is all."),
    ])
    .unwrap();
    show("start 2:", &dlg.current().unwrap());
    dlg.advance().unwrap();
    show("advance (note shown):", &dlg.current().unwrap());
    dlg.advance().unwrap();
    show("advance:", &dlg.current().unwrap());
    dlg.advance().unwrap();
    println!("is_finished: {}", dlg.is_finished());
    println!(
        "facts at end: clearance = {:?}, trusted = {:?}",
        dlg.fact("clearance"),
        dlg.fact("trusted")
    );
}
