//! The decision layer: incident memory, the policy gate, and the three arms.
//!
//! - Reactive: a fixed runbook rule. No memory, no simulation.
//! - Memory-only: picks the action with the best historical average. Learns,
//!   but cannot adapt to the specifics of the current incident.
//! - Full Aegis: simulates every policy-allowed action against the twin and
//!   picks the safest viable one. Memory + simulation.

use std::collections::HashMap;

use crate::model::{Action, Params, Symptom};
use crate::sim::{scenario_score, simulate_from, SimState};

/// The running estimate of scenario score per (symptom, action). `value` is
/// updated either as a plain running mean (`record`, for offline training) or as
/// a decayed exponential moving average (`learn`, for online operation).
#[derive(Default, Clone, Copy)]
struct Stat {
    n: u32,
    value: f64,
}

/// Operational memory: what historically happened when an action was taken for
/// a given symptom. The compounding knowledge asset, in miniature.
#[derive(Clone)]
pub struct IncidentMemory {
    table: HashMap<(Symptom, Action), Stat>,
}

impl IncidentMemory {
    pub fn new() -> Self {
        IncidentMemory {
            table: HashMap::new(),
        }
    }

    /// Offline update: fold `score` into the running mean.
    pub fn record(&mut self, s: Symptom, a: Action, score: f64) {
        let e = self.table.entry((s, a)).or_default();
        e.n += 1;
        e.value += (score - e.value) / e.n as f64;
    }

    /// Online update: fold `score` in as an exponential moving average with
    /// learning rate `alpha`. Recent outcomes weigh more, so stale lessons decay
    /// — the memory adapts when the world drifts under it.
    pub fn learn(&mut self, s: Symptom, a: Action, score: f64, alpha: f64) {
        let e = self.table.entry((s, a)).or_default();
        if e.n == 0 {
            e.value = score;
        } else {
            e.value += alpha * (score - e.value);
        }
        e.n += 1;
    }

    pub fn mean(&self, s: Symptom, a: Action) -> Option<f64> {
        self.table.get(&(s, a)).filter(|x| x.n > 0).map(|x| x.value)
    }
}

impl Default for IncidentMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// The policy gate: what the runtime is allowed to do (core law #3).
pub struct Policy;

impl Policy {
    pub fn allows(&self, a: Action, s: &SimState) -> bool {
        match a {
            // Restarting the beacon anchor while B is actively moving flaps the
            // beacon and endangers B. Prohibited unless B is already halted.
            Action::RestartRobotA => {
                let b_actively_moving = s.b_in_motion && !s.b_halted;
                !b_actively_moving
            }
            _ => true,
        }
    }
}

/// Everything a decider needs at a decision point.
pub struct DecisionContext<'a> {
    pub symptom: Symptom,
    pub belief: SimState,
    /// Confidence in [0.5, 1.0] that `belief`'s jam-vs-brownout call is correct.
    /// 1.0 when the diagnosis is unambiguous; the robust arm hedges when it's low.
    pub confidence: f64,
    pub horizon: u32,
    pub decision_tick: u32,
    pub twin_params: &'a Params,
    pub memory: &'a IncidentMemory,
    pub policy: &'a Policy,
}

pub enum Arm {
    Reactive,
    MemoryOnly,
    FullAegis,
    /// Full Aegis that is *confidence-aware*: under an uncertain diagnosis it
    /// weighs each action across both fault hypotheses and hedges, rather than
    /// committing to the single most-likely guess.
    FullAegisRobust,
}

impl Arm {
    pub fn name(&self) -> &'static str {
        match self {
            Arm::Reactive => "Reactive",
            Arm::MemoryOnly => "Memory-only",
            Arm::FullAegis => "Full Aegis",
            Arm::FullAegisRobust => "Full Aegis+",
        }
    }

    pub fn decide(&self, ctx: &DecisionContext) -> Action {
        match self {
            // Fixed runbook: "beacon lost → restore a beacon via C." Reasonable,
            // but blind to whether C is actually ready.
            Arm::Reactive => Action::PromoteCToBeacon,
            Arm::MemoryOnly => best_from_memory(ctx),
            Arm::FullAegis => full_aegis_decide(ctx),
            Arm::FullAegisRobust => full_aegis_robust_decide(ctx),
        }
    }
}

fn best_from_memory(ctx: &DecisionContext) -> Action {
    best_memory_action(ctx.memory, ctx.symptom, ctx.policy, &ctx.belief)
}

/// The policy-allowed action with the best remembered score for `symptom`, or
/// the safe default (halt) if memory has nothing. Shared by the Memory-only arm
/// and the online learner.
pub fn best_memory_action(
    mem: &IncidentMemory,
    symptom: Symptom,
    policy: &Policy,
    state: &SimState,
) -> Action {
    let mut best = Action::HaltB; // safe fallback if memory is empty
    let mut best_mean = f64::NEG_INFINITY;
    for a in Action::all() {
        if !policy.allows(a, state) {
            continue;
        }
        if let Some(m) = mem.mean(symptom, a) {
            if m > best_mean {
                best_mean = m;
                best = a;
            }
        }
    }
    best
}

fn full_aegis_decide(ctx: &DecisionContext) -> Action {
    let mut best = Action::HaltB;
    let mut best_score = f64::NEG_INFINITY;
    for a in Action::all() {
        if !ctx.policy.allows(a, &ctx.belief) {
            continue;
        }
        // Roll the action forward against the twin (the belief state).
        let mut s = ctx.belief.clone();
        s.decision_tick = ctx.decision_tick;
        let outcome = simulate_from(s, a, ctx.twin_params, ctx.horizon);
        let mut score = scenario_score(&outcome);
        // Break ties toward what memory says has worked historically.
        if let Some(m) = ctx.memory.mean(ctx.symptom, a) {
            score += 1e-3 * m;
        }
        if score > best_score {
            best_score = score;
            best = a;
        }
    }
    best
}

/// Confidence-aware Full Aegis. Under an ambiguous diagnosis it does not commit
/// to the single most-likely fault: it scores each action as the
/// confidence-weighted *expectation* over both hypotheses (the believed fault
/// and its swap), so it prefers an action that is good — or at least safe —
/// whichever fault it really is. When confidence is 1.0 this is exactly Full
/// Aegis (the alternative hypothesis gets zero weight).
fn full_aegis_robust_decide(ctx: &DecisionContext) -> Action {
    let conf = ctx.confidence;
    // The alternative hypothesis: the same world with the jam/brownout call flipped.
    let mut alt = ctx.belief.clone();
    std::mem::swap(&mut alt.beacon_jammed, &mut alt.tx_degraded);

    let mut best = Action::HaltB;
    let mut best_score = f64::NEG_INFINITY;
    for a in Action::all() {
        if !ctx.policy.allows(a, &ctx.belief) {
            continue;
        }
        let mut primary = ctx.belief.clone();
        primary.decision_tick = ctx.decision_tick;
        let o_primary = simulate_from(primary, a, ctx.twin_params, ctx.horizon);

        let mut alternate = alt.clone();
        alternate.decision_tick = ctx.decision_tick;
        let o_alt = simulate_from(alternate, a, ctx.twin_params, ctx.horizon);

        let mut score = conf * scenario_score(&o_primary) + (1.0 - conf) * scenario_score(&o_alt);
        if let Some(m) = ctx.memory.mean(ctx.symptom, a) {
            score += 1e-3 * m;
        }
        if score > best_score {
            best_score = score;
            best = a;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Params;
    use crate::rng::Rng;
    use crate::sim::{gen_scenario, observe, run_scenario, seed_for, ScenarioCfg, SimState};

    fn ctx_for<'a>(
        belief: SimState,
        mem: &'a IncidentMemory,
        pol: &'a Policy,
        twin: &'a Params,
    ) -> DecisionContext<'a> {
        DecisionContext {
            symptom: Symptom::BeaconLostBDrifting,
            decision_tick: belief.decision_tick,
            belief,
            confidence: 1.0,
            horizon: twin.horizon,
            twin_params: twin,
            memory: mem,
            policy: pol,
        }
    }

    #[test]
    fn memory_means_are_correct() {
        let mut m = IncidentMemory::new();
        let sym = Symptom::BeaconLostBDrifting;
        m.record(sym, Action::HaltB, 1.0);
        m.record(sym, Action::HaltB, 1.0);
        m.record(sym, Action::DoNothing, -2.0);
        assert_eq!(m.mean(sym, Action::HaltB), Some(1.0));
        assert_eq!(m.mean(sym, Action::DoNothing), Some(-2.0));
        assert_eq!(m.mean(sym, Action::FailoverCharger), None);
    }

    #[test]
    fn ema_learning_decays_stale_lessons() {
        let mut m = IncidentMemory::new();
        let sym = Symptom::BeaconLostPower;
        // Learn that an action scores +2 ...
        for _ in 0..200 {
            m.learn(sym, Action::FailoverCharger, 2.0, 0.2);
        }
        assert!((m.mean(sym, Action::FailoverCharger).unwrap() - 2.0).abs() < 0.01);
        // ... then the world drifts and it now scores -2: the EMA forgets the
        // stale lesson and tracks the new reality.
        for _ in 0..200 {
            m.learn(sym, Action::FailoverCharger, -2.0, 0.2);
        }
        assert!(m.mean(sym, Action::FailoverCharger).unwrap() < -1.9);
    }

    #[test]
    fn policy_forbids_restart_while_b_moves() {
        let cfg = ScenarioCfg::power(true, 3.0, 50.0);
        let mut s = SimState::initial(&cfg);
        let pol = Policy;
        assert!(
            !pol.allows(Action::RestartRobotA, &s),
            "B is moving -> restart forbidden"
        );
        s.b_halted = true;
        assert!(
            pol.allows(Action::RestartRobotA, &s),
            "B halted -> restart allowed"
        );
        assert!(pol.allows(Action::HaltB, &s));
    }

    #[test]
    fn best_from_memory_skips_forbidden_and_picks_max() {
        let mut m = IncidentMemory::new();
        let sym = Symptom::BeaconLostBDrifting;
        m.record(sym, Action::RestartRobotA, 5.0); // best mean, but forbidden
        m.record(sym, Action::HaltB, 1.0);
        m.record(sym, Action::DoNothing, -2.0);
        let cfg = ScenarioCfg::power(false, 2.0, 40.0);
        let belief = SimState::initial(&cfg); // B moving -> restart forbidden
        let pol = Policy;
        let p = Params::ground_truth();
        let ctx = ctx_for(belief, &m, &pol, &p);
        assert_eq!(Arm::MemoryOnly.decide(&ctx), Action::HaltB);
    }

    #[test]
    fn full_aegis_never_picks_a_forbidden_action() {
        let p = Params::ground_truth();
        let mem = IncidentMemory::new();
        let pol = Policy;
        for idx in 0..3000usize {
            let mut r = Rng::new(seed_for(5, idx));
            let cfg = gen_scenario(&mut r);
            let mut br = Rng::new(seed_for(50, idx));
            let mut captured = Action::DoNothing;
            run_scenario(&cfg, &p, |truth| {
                let belief = observe(truth, 1.0, &mut br);
                let a = Arm::FullAegis.decide(&ctx_for(belief, &mem, &pol, &p));
                captured = a;
                a
            });
            assert_ne!(
                captured,
                Action::RestartRobotA,
                "B moves at decision -> never restart"
            );
        }
    }

    #[test]
    fn full_aegis_with_faithful_twin_is_always_safe() {
        let p = Params::ground_truth();
        let mem = IncidentMemory::new();
        let pol = Policy;
        for idx in 0..4000usize {
            let mut r = Rng::new(seed_for(13, idx));
            let cfg = gen_scenario(&mut r);
            let mut br = Rng::new(seed_for(130, idx));
            let o = run_scenario(&cfg, &p, |truth| {
                let belief = observe(truth, 1.0, &mut br);
                Arm::FullAegis.decide(&ctx_for(belief, &mem, &pol, &p))
            });
            assert!(
                o.safe,
                "a faithful twin must yield a safe choice (idx {idx})"
            );
        }
    }

    #[test]
    fn full_aegis_promotes_c_when_c_is_ready() {
        let p = Params::ground_truth();
        let mem = IncidentMemory::new();
        let pol = Policy;
        // C ready AND slow charge: promoting C is the *uniquely* optimal action
        // (failover would leave a danger window while A recharges slowly).
        let mut idx = 0usize;
        let cfg = loop {
            let mut r = Rng::new(seed_for(21, idx));
            let c = gen_scenario(&mut r);
            if c.c_ready && c.charge_rate < 2.2 {
                break c;
            }
            idx += 1;
        };
        let mut br = Rng::new(7);
        let mut captured = Action::DoNothing;
        run_scenario(&cfg, &p, |truth| {
            let belief = observe(truth, 1.0, &mut br);
            let a = Arm::FullAegis.decide(&ctx_for(belief, &mem, &pol, &p));
            captured = a;
            a
        });
        assert_eq!(captured, Action::PromoteCToBeacon);
    }

    #[test]
    fn robust_arm_hedges_to_a_safe_action_when_unsure() {
        // A jam-or-brownout with C not ready: committing to either specific fix
        // is a coin-flip, so the confidence-aware arm hedges to HaltB (safe under
        // *both* hypotheses) rather than gamble.
        let p = Params::ground_truth();
        let mem = IncidentMemory::new();
        let pol = Policy;
        let cfg = ScenarioCfg::interference(false, 5); // C not ready
        let mut captured = Action::DoNothing;
        run_scenario(&cfg, &p, |truth| {
            let ctx = DecisionContext {
                symptom: Symptom::BeaconLostInterference,
                decision_tick: truth.decision_tick,
                belief: truth.clone(),
                confidence: 0.5, // maximally ambiguous
                horizon: p.horizon,
                twin_params: &p,
                memory: &mem,
                policy: &pol,
            };
            captured = Arm::FullAegisRobust.decide(&ctx);
            Action::HaltB
        });
        assert_eq!(captured, Action::HaltB);
    }
}
