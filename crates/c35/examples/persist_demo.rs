//! Persistence demo: build a snapshot, save it, load
//! it back, and prove the rules — byte-identical
//! round-trip, refusal of a corrupted byte, refusal of
//! a future version, refusal of a truncation, and the
//! career boundary (journal sealed, Archive grown by
//! exactly the sealed entries, city carried over).
//!
//! Run: cargo run -q -p c35 --example persist_demo

use c35::persist::{CareerStatus, Snapshot};

fn build_snapshot() -> Snapshot {
    // Day 47 of career 3, in a city built from the
    // house seed. One prior Archive entry stands from
    // an earlier debrief, so the boundary math has a
    // baseline to grow from.
    let mut snap = Snapshot::new(0xC35_0001, 47, "Agent Three");
    snap.career.agent_no = 3;
    snap.world.deltas.push(c35::persist::WorldDelta {
        key: "body.moved.7".to_string(),
        value: "alley.north".to_string(),
    });
    snap.entities.push(c35::persist::EntityState {
        id: "operator".to_string(),
        kind: "operator".to_string(),
        x: 12.5,
        y: 0.0,
        z: -3.25,
        yaw: 1.57,
        state: "alive".to_string(),
        data: "street-kit".to_string(),
    });
    snap.entities.push(c35::persist::EntityState {
        id: "body.7".to_string(),
        kind: "body".to_string(),
        x: 40.0,
        y: 0.0,
        z: 8.0,
        yaw: 0.0,
        state: "dead".to_string(),
        data: "hidden.alley".to_string(),
    });
    snap.archive_append(1, 40, "Crossed the checkpoint clean.", &["crossing"]);
    snap.journal_push(11, 47, "Reached the dead drop.", &["drop"]);
    snap.journal_push(12, 47, "Witnessed in assault kit.", &["witnessed"]);
    snap.journal_push(13, 47, "Hound crossed the trail.", &["hound"]);
    snap
}

fn main() {
    let snap = build_snapshot();
    let bytes = snap.save();
    println!("snapshot bytes      : {}", bytes.len());

    // 1. Byte-identical round-trip.
    let loaded = Snapshot::load(&bytes).expect("valid save must load");
    let re_saved = loaded.save();
    println!("round-trip identical: {}", re_saved == bytes);
    println!("round-trip equal    : {}", loaded == snap);

    // 2. One corrupted byte is refused, never half-loaded.
    let mut corrupt = bytes.clone();
    let mid = corrupt.len() / 2;
    corrupt[mid] ^= 0xFF;
    match Snapshot::load(&corrupt) {
        Ok(_) => println!("corrupt byte refused: FALSE (loaded!)"),
        Err(e) => println!("corrupt byte refused: true ({})", e),
    }

    // 3. A future version is refused as a version error,
    // even though the checksum is now stale too — the
    // version gate fires before the checksum gate.
    let mut future = bytes.clone();
    future[4] = 2;
    future[5] = 0;
    match Snapshot::load(&future) {
        Ok(_) => println!("future version refused: FALSE (loaded!)"),
        Err(e) => println!("future version refused: true ({})", e),
    }

    // 4. A truncation is refused.
    let cut = &bytes[..bytes.len() - 5];
    match Snapshot::load(cut) {
        Ok(_) => println!("truncation refused  : FALSE (loaded!)"),
        Err(e) => println!("truncation refused  : true ({})", e),
    }

    // 5. The career boundary. Journal holds 3 entries,
    // Archive holds 1. After close_career: journal 0,
    // Archive 4 (grown by exactly the sealed 3), city
    // seed and entities untouched, new career active.
    let mut next = build_snapshot();
    let archive_before = next.archive.len();
    let journal_before = next.journal.len();
    let seed_before = next.world.seed;
    let entities_before = next.entities.len();
    let new_no = next.close_career("killed", "Agent Four");
    println!("career boundary:");
    println!("  journal {} -> {}", journal_before, next.journal.len());
    println!(
        "  archive {} -> {} (grew by {})",
        archive_before,
        next.archive.len(),
        next.archive.len() - archive_before
    );
    println!("  new agent_no      : {}", new_no);
    println!("  city seed carried : {}", next.world.seed == seed_before);
    println!(
        "  entities carried  : {}",
        next.entities.len() == entities_before
    );
    println!(
        "  sealed entries belong to career 3: {}",
        next.archive
            .iter()
            .skip(archive_before)
            .all(|e| e.career == 3)
    );
    println!(
        "  new career active : {}",
        matches!(next.career.status, CareerStatus::Active)
    );
    println!("  new career name   : {}", next.career.agent_name);

    // The boundary result itself saves and loads clean.
    let next_bytes = next.save();
    let next_loaded = Snapshot::load(&next_bytes).expect("boundary save must load");
    println!("boundary save loads : {}", next_loaded == next);
}
