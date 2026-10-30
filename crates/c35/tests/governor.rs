use c35::governor::{quality_at, Governor, MAX_LEVEL};

#[test]
fn overload_walks_the_whole_ladder_and_stops() {
    let mut g = Governor::new(16.7);
    for _ in 0..2000 {
        g.observe(40.0);
    }
    assert_eq!(g.level(), MAX_LEVEL);
    assert_eq!(g.adjustments().len(), MAX_LEVEL);
    for a in g.adjustments() {
        assert_eq!(a.to_level, a.from_level + 1);
    }
}

#[test]
fn pinned_quality_is_absolute_and_unpin_restarts_cooldown() {
    let mut g = Governor::new(16.7);
    g.pin_to(2);
    for _ in 0..1000 {
        assert!(!g.observe(100.0));
    }
    assert_eq!(g.level(), 2);
    assert_eq!(g.pinned_overruns(), 1000);
    g.unpin();
    for _ in 0..59 {
        assert!(!g.observe(100.0));
    }
    // Cooldown is 60 frames, but decisions still respect the global
    // 30-frame evaluation cadence (unpin need not occur on that boundary).
    let changed = (0..30).any(|_| g.observe(100.0));
    assert!(changed);
    assert_eq!(g.level(), 3);
}

#[test]
fn recovery_needs_sustained_headroom() {
    let mut g = Governor::new(16.7);
    g.force_level(5);
    for _ in 0..299 {
        assert!(!g.observe(5.0));
    }
    assert!(g.observe(5.0));
    assert_eq!(g.level(), 4);
}

#[test]
fn invalid_timings_and_targets_do_not_poison_state() {
    let mut g = Governor::new(f64::NAN);
    assert!(g.target_ms().is_finite());
    g.observe(20.0);
    for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        g.set_target(bad);
        assert!(!g.observe(bad));
        assert_eq!(g.ema_ms(), 20.0);
        assert!(g.target_ms() > 0.0);
    }
}

#[test]
fn every_quality_step_reduces_a_cost() {
    for i in 0..MAX_LEVEL {
        let a = quality_at(i);
        let b = quality_at(i + 1);
        assert_ne!(a, b);
        assert!(b.render_scale <= a.render_scale && b.shadow_size <= a.shadow_size);
        assert!(b.detail_fade_mul <= a.detail_fade_mul && b.water_octaves <= a.water_octaves);
    }
    assert_eq!(quality_at(usize::MAX), quality_at(MAX_LEVEL));
}
