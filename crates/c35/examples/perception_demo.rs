//! Perception demonstration — a scripted street scene.
//!
//! This is a demonstration, not a proof harness: it prints
//! behavior lines (states, postures, voices, records) and
//! asserts nothing. Chase judges by playing; this exists
//! so a reader can see the brains decide without a GPU.
//!
//! Scenes: a soldier's glimpse → search → patrol and a
//! full exposure → alarm; sound propagation to three
//! ears; a Hound on a trail through rain and a grace
//! break; a Weathervane report feeding the alert budget;
//! a Neighbour writing an assault description; a
//! checkpoint graduating Question → Detention → Force;
//! both mecha governors; and a sense view that carries
//! no scent and no icons, by construction.

use c35::perception::economy::ConfigKind;
use c35::perception::enemies::{BehaviourNote, HoundBrain, NeighbourBrain, WeathervaneBrain};
use c35::perception::field::{Emitter, LightField, Occluder, Tone};
use c35::perception::mecha::{MechaComponent, Type1Governor, Type2Governor};
use c35::perception::pricing::{visibility, Posture};
use c35::perception::scent::ScentField;
use c35::perception::soldier::{PapersState, SoldierBrain, SoldierInput, SoldierState};
use c35::perception::sound::propagate;
use c35::perception::visor::SenseView;
use c35::perception::{AlertBudget, District, WitnessLedger};
use c35::traversal::events::{StimulusEvent, StimulusKind};
use c35::traversal::surface::{SurfaceKind, SurfaceRecord};
use glam::Vec3;

fn base_input() -> SoldierInput {
    SoldierInput {
        visibility: 0.0,
        target_pos: None,
        target_posture: None,
        heard: Vec::new(),
        body_seen: false,
        at_checkpoint: false,
        papers: PapersState::Valid,
        compliant: true,
        mecha_near: false,
        mecha_in_arc: false,
        grace: 0.0,
        rain: 0.0,
        cold: 0.0,
    }
}

fn main() {
    // ---- The field: night ambient, one sodium pool, one wall.
    let mut field = LightField::new(0.12);
    field.add_emitter(Emitter {
        pos: Vec3::new(10.0, 3.0, 0.0),
        radius: 12.0,
        intensity: 1.0,
        tone: Tone::Sodium,
    });
    field.add_occluder(Occluder {
        min: Vec3::new(20.0, 0.0, -5.0),
        max: Vec3::new(21.0, 4.0, 5.0),
    });
    let lamp_light = field.sample(Vec3::new(10.0, 0.0, 0.0)).level;
    let shadow_light = field.sample(Vec3::new(0.0, 0.0, 8.0)).level;
    println!("field: lamp pool light {lamp_light:.2}, shadow light {shadow_light:.2}");

    // ---- Scene A: a glimpse → suspicious → search → patrol.
    let mut soldier =
        SoldierBrain::new(1, Vec3::new(0.0, 0.0, 0.0), vec![Vec3::new(0.0, 0.0, 0.0)]);
    let mut prev = soldier.state;
    let player_spot = Vec3::new(10.0, 0.0, 0.0);
    for _ in 0..15 {
        let mut input = base_input();
        // Moderate sighting: lamp pool, upright, walking.
        input.visibility = visibility(lamp_light, 2.0, Posture::Upright);
        input.target_pos = Some(player_spot);
        input.target_posture = Some(Posture::Upright);
        let out = soldier.tick(input);
        if out.state != prev {
            println!("A glimpse: state {:?} -> {:?}", prev, out.state);
            prev = out.state;
        }
    }
    for _ in 0..14_000 {
        let out = soldier.tick(base_input());
        if out.state != prev {
            println!("A search decays: state {:?} -> {:?}", prev, out.state);
            prev = out.state;
        }
        if out.state == SoldierState::Patrol && soldier.memory.lkp.is_none() {
            break;
        }
    }
    println!(
        "A final: state {:?}, attention {:.2}",
        soldier.state, soldier.attention.level
    );

    // ---- Scene A2: full exposure → alarm → body found.
    let mut soldier2 = SoldierBrain::new(2, Vec3::new(0.0, 0.0, 0.0), vec![]);
    for _ in 0..30 {
        let mut input = base_input();
        input.visibility = visibility(lamp_light, 5.8, Posture::Upright);
        input.target_pos = Some(player_spot);
        soldier2.tick(input);
    }
    println!(
        "A2 exposed: state {:?}, attention {:.2}",
        soldier2.state, soldier2.attention.level
    );
    let mut input = base_input();
    input.body_seen = true;
    let out = soldier2.tick(input);
    println!(
        "A2 body found: state {:?}, voice {:?}",
        out.state, out.voice
    );

    // ---- Scene B: one footstep, three ears.
    let event = StimulusEvent {
        tick: 0,
        kind: StimulusKind::Footstep,
        pos: Vec3::new(15.0, 0.0, 0.0),
        surface: SurfaceRecord::dry(SurfaceKind::Concrete),
        magnitude: 0.9,
    };
    for (name, ear) in [
        ("near ear", Vec3::new(18.0, 0.0, 0.0)),
        ("far ear", Vec3::new(40.0, 0.0, 0.0)),
        ("walled ear", Vec3::new(25.0, 0.0, 0.0)),
    ] {
        let heard = propagate(std::slice::from_ref(&event), ear, &field);
        match heard.first() {
            Some(h) => println!("B {name}: heard {:.2} from {:?}", h.heard, h.dir),
            None => println!("B {name}: heard nothing"),
        }
    }

    // ---- Scene C: the Hound on a trail, through rain, to a break.
    let mut scent = ScentField::new();
    for x in 0..21 {
        scent.deposit(Vec3::new(x as f32, 0.0, 0.0), 1.0);
    }
    let mut hound = HoundBrain::new(Vec3::new(-2.0, 0.0, 0.0), 15.0);
    let handler = Vec3::new(-3.0, 0.0, 0.0);
    for _ in 0..400 {
        let out = hound.follow(&scent, handler, false);
        if let Some(dir) = out.move_dir {
            hound.pos += dir * 0.02;
        }
    }
    println!(
        "C hound: state {:?}, lock {:.2}, pos {:?}",
        hound.state, hound.lock, hound.pos
    );
    scent.rain = 1.0;
    for _ in 0..2400 {
        scent.tick();
    }
    let sample = scent.sample(hound.pos, 3.0);
    let out = hound.tick(sample, handler, false);
    println!(
        "C after rain: sample {:?}, hound {:?}",
        sample.map(|(_, s)| s),
        out.state
    );
    for x in 0..21 {
        scent.deposit(Vec3::new(x as f32, 0.0, 0.0), 1.0);
    }
    let broken = scent.break_radius(Vec3::new(10.0, 0.0, 0.0), 3.0);
    println!(
        "C grace break: {broken} segments broken, {} remain",
        scent.segments.len()
    );

    // ---- Scene D: Weathervane report → alert budget.
    let mut weather = WeathervaneBrain::new(vec![Vec3::new(0.0, 10.0, 0.0)]);
    let mut budget = AlertBudget::new();
    for _ in 0..600 {
        if let Some(report) = weather.tick(true, Some(Vec3::new(5.0, 0.0, 5.0))) {
            println!(
                "D weathervane reported: confidence {:.2}",
                report.confidence
            );
            budget.report_received();
        }
    }
    println!(
        "D alert level {:.1}, patrol tier {}",
        budget.level,
        budget.patrol_tier()
    );

    // ---- Scene D2: Neighbour writes an assault description.
    let mut neighbour = NeighbourBrain::new();
    let mut written = None;
    for _ in 0..400 {
        written = neighbour.tick(BehaviourNote {
            suspicious: true,
            config: ConfigKind::Assault,
            distance: 15.0,
        });
        if written.is_some() {
            break;
        }
    }
    let mut witnesses = WitnessLedger::new();
    if let Some(d) = written {
        witnesses.file(d);
        println!(
            "D2 description written: {:?} at {:.2}, strictness {:.2}, blocks wanderer {}",
            d.config,
            d.confidence,
            witnesses.checkpoint_strictness(),
            witnesses.blocks_wanderer()
        );
    }

    // ---- Scene E: checkpoint graduation.
    let mut guard = SoldierBrain::new(3, Vec3::new(0.0, 0.0, 0.0), vec![]);
    let mut prev_phase = guard.checkpoint_phase;
    for _ in 0..500 {
        let mut input = base_input();
        input.at_checkpoint = true;
        input.papers = PapersState::Invalid;
        input.compliant = true;
        guard.tick(input);
        if guard.checkpoint_phase != prev_phase {
            println!(
                "E checkpoint: phase {:?} -> {:?}",
                prev_phase, guard.checkpoint_phase
            );
            prev_phase = guard.checkpoint_phase;
        }
    }
    let mut input = base_input();
    input.at_checkpoint = true;
    input.papers = PapersState::Invalid;
    input.compliant = false;
    guard.tick(input);
    println!("E non-compliance: phase {:?}", guard.checkpoint_phase);

    // ---- Scene F: both mecha governors.
    let mut t1 = Type1Governor::new();
    t1.damage(MechaComponent::Sensors);
    t1.damage(MechaComponent::Sensors);
    println!(
        "F type1 blinded: ladder {}, audio reliance {:.1}",
        t1.ladder_stage(),
        t1.audio_reliance()
    );
    t1.damage(MechaComponent::Coolant);
    t1.damage(MechaComponent::Coolant);
    println!(
        "F type1 breached: dead {}, ladder {}",
        t1.is_dead(),
        t1.ladder_stage()
    );
    let mut t2 = Type2Governor::new();
    println!(
        "F type2 unbought trigger: {}",
        t2.trigger_charge(Vec3::X, false)
    );
    println!(
        "F type2 bought trigger: {}",
        t2.trigger_charge(Vec3::X, true)
    );
    for _ in 0..240 {
        t2.tick();
    }
    println!(
        "F type2 after windup: state {:?}, gib {}",
        t2.state,
        t2.contact_gib_capable()
    );
    if let Some(scream) = t2.scream(Vec3::new(0.0, 0.0, 0.0)) {
        println!(
            "F type2 scream heard at 100m: {:.2}",
            scream.heard_at(Vec3::new(100.0, 0.0, 0.0))
        );
    }
    t2.silence_voice();
    println!(
        "F type2 silenced: scream present {}",
        t2.scream(Vec3::ZERO).is_some()
    );
    t2.end_charge();
    for _ in 0..800 {
        t2.tick();
    }
    println!("F type2 after overshoot+recovery: state {:?}", t2.state);

    // ---- Scene G: the sense view — no scent, no icons.
    let mut district = District::new(0.12);
    district.light.add_emitter(Emitter {
        pos: Vec3::new(10.0, 3.0, 0.0),
        radius: 12.0,
        intensity: 1.0,
        tone: Tone::Sodium,
    });
    let eye = Vec3::new(10.0, 1.62, 0.0);
    let view = SenseView::build(
        &district.light,
        eye,
        &[(Vec3::new(1.0, 0.0, 0.0), soldier2.attention.level)],
        &[(Vec3::new(12.0, 0.0, 0.0), &t1)],
    );
    println!(
        "G sense view: {} blooms, {} tones, {} mecha reads, rim {:.2}, scent fields: none (by construction)",
        view.blooms.len(),
        view.tones.len(),
        view.mechas.len(),
        view.rim
    );
    for m in &view.mechas {
        println!(
            "G mecha read: sensors {:?}, loco {:?}, cannon {:?}, coolant {:?}",
            m.sensors, m.locomotion, m.cannon, m.coolant
        );
    }
}
