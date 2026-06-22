//! Property & oracle tests over the public API.
//!
//! Picklejar style: each test is a seeded sweep over thousands of cases, every
//! one asserting an invariant against a named oracle. No proptest dependency —
//! the project's own deterministic RNG drives the cases, so the binary stays
//! zero-dependency and every failure replays from its index.

use aegis::decision::{Arm, IncidentMemory};
use aegis::experiment::{evaluate, train_memory};
use aegis::model::Action;
use aegis::model::Params;
use aegis::rng::Rng;
use aegis::sim::{gen_scenario, run_scenario, run_with_log, seed_for};

fn p() -> Params {
    Params::ground_truth()
}

/// Oracle: a scenario is a pure function of its seed, so the same incident
/// replays bit-for-bit — the same events at the same ticks, the same outcome.
#[test]
fn incidents_replay_deterministically() {
    for idx in 0..3000usize {
        let mut r = Rng::new(seed_for(101, idx));
        let cfg = gen_scenario(&mut r);
        let (o1, log1) = run_with_log(&cfg, Action::FailoverCharger, &p());
        let (o2, log2) = run_with_log(&cfg, Action::FailoverCharger, &p());
        assert_eq!(
            (o1.safe, o1.successful, o1.mttr),
            (o2.safe, o2.successful, o2.mttr)
        );
        assert_eq!(log1.len(), log2.len());
        for (a, b) in log1.events.iter().zip(log2.events.iter()) {
            assert_eq!(a.tick, b.tick);
            assert_eq!(a.kind.describe(), b.kind.describe());
        }
    }
}

/// Oracle: halting B parks it safely; it can never enter a collision-risk state.
#[test]
fn halt_b_is_never_dangerous() {
    for idx in 0..20_000usize {
        let mut r = Rng::new(seed_for(202, idx));
        let cfg = gen_scenario(&mut r);
        let o = run_scenario(&cfg, &p(), |_| Action::HaltB);
        assert!(o.safe, "HaltB was dangerous at idx {idx}");
    }
}

/// Oracle: do-nothing always lets the cascade reach a dangerous state (the
/// incident is, by construction, an actual emergency).
#[test]
fn do_nothing_always_ends_in_danger() {
    for idx in 0..5000usize {
        let mut r = Rng::new(seed_for(303, idx));
        let cfg = gen_scenario(&mut r);
        let o = run_scenario(&cfg, &p(), |_| Action::DoNothing);
        assert!(
            !o.safe && !o.successful,
            "do-nothing should be unsafe at idx {idx}"
        );
    }
}

/// Oracle: with a faithful twin (fidelity 1.0), Full Aegis must always choose a
/// safe action — there is always at least HaltB, and the twin predicts it true.
#[test]
fn full_aegis_is_always_safe_at_full_fidelity() {
    let mem = IncidentMemory::new();
    let s = evaluate(&Arm::FullAegis, 8000, 0x5151, 1.0, &mem, false);
    assert_eq!(
        s.danger, 0.0,
        "a faithful twin must never lead Full Aegis into danger"
    );
}

/// Oracle: the three arms are strictly ordered — remembering beats reacting,
/// and simulating beats remembering.
#[test]
fn strategies_are_strictly_ordered() {
    let mem = train_memory(8000, 0x5151);
    let r = evaluate(&Arm::Reactive, 4000, 0x5151, 1.0, &mem, false);
    let m = evaluate(&Arm::MemoryOnly, 4000, 0x5151, 1.0, &mem, false);
    let f = evaluate(&Arm::FullAegis, 4000, 0x5151, 1.0, &mem, false);
    assert!(m.score > r.score, "memory > reactive");
    assert!(f.score > m.score, "simulation > memory");
    assert!(
        f.success > m.success,
        "simulation recovers more than memory's safe default"
    );
}

/// Oracle: the closed loop lifts recovery without sacrificing safety.
#[test]
fn the_closed_loop_recovers_more_at_equal_safety() {
    let mem = train_memory(8000, 0x5151);
    let one = evaluate(&Arm::FullAegis, 4000, 0x5151, 1.0, &mem, false);
    let many = evaluate(&Arm::FullAegis, 4000, 0x5151, 1.0, &mem, true);
    assert!(many.safe >= 99.0);
    assert!(
        many.success > one.success,
        "multi-step should recover more incidents"
    );
}

/// Oracle: degrading the twin never helps — safety and score fall as fidelity
/// drops. This is the anti-circularity guarantee, stated as a test.
#[test]
fn degrading_the_twin_never_helps() {
    let mem = train_memory(8000, 0x5151);
    let levels = [1.0, 0.75, 0.5, 0.25];
    let mut prev = evaluate(&Arm::FullAegis, 4000, 0x5151, levels[0], &mem, false);
    for &f in &levels[1..] {
        let cur = evaluate(&Arm::FullAegis, 4000, 0x5151, f, &mem, false);
        assert!(
            cur.score <= prev.score + 1e-9,
            "score should not rise as fidelity falls"
        );
        assert!(
            cur.safe <= prev.safe + 1e-9,
            "safety should not rise as fidelity falls"
        );
        prev = cur;
    }
}
