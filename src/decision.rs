//! The decision layer: incident memory, the policy gate, and the three arms.
//!
//! - Reactive   — a fixed runbook rule. No memory, no simulation.
//! - Memory-only — picks the action with the best *historical* average. Learns,
//!                 but cannot adapt to the specifics of the current incident.
//! - Full Aegis  — simulates every policy-allowed action against the twin and
//!                 picks the safest viable one. Memory + simulation.

use std::collections::HashMap;

use crate::model::{Action, Params, Symptom};
use crate::sim::{scenario_score, simulate_from, SimState};

/// Running mean of scenario scores per (symptom, action).
#[derive(Default, Clone, Copy)]
struct Stat {
    n: u32,
    sum: f64,
}

/// Operational memory: what historically happened when an action was taken for
/// a given symptom. The compounding knowledge asset, in miniature.
pub struct IncidentMemory {
    table: HashMap<(Symptom, Action), Stat>,
}

impl IncidentMemory {
    pub fn new() -> Self {
        IncidentMemory { table: HashMap::new() }
    }

    pub fn record(&mut self, s: Symptom, a: Action, score: f64) {
        let e = self.table.entry((s, a)).or_default();
        e.n += 1;
        e.sum += score;
    }

    pub fn mean(&self, s: Symptom, a: Action) -> Option<f64> {
        self.table
            .get(&(s, a))
            .filter(|x| x.n > 0)
            .map(|x| x.sum / x.n as f64)
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
            Action::RestartRobotA => !(s.b_in_motion && !s.b_halted),
            _ => true,
        }
    }
}

/// Everything a decider needs at a decision point.
pub struct DecisionContext<'a> {
    pub symptom: Symptom,
    pub belief: SimState,
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
}

impl Arm {
    pub fn name(&self) -> &'static str {
        match self {
            Arm::Reactive => "Reactive",
            Arm::MemoryOnly => "Memory-only",
            Arm::FullAegis => "Full Aegis",
        }
    }

    pub fn decide(&self, ctx: &DecisionContext) -> Action {
        match self {
            // Fixed runbook: "beacon lost → restore a beacon via C." Reasonable,
            // but blind to whether C is actually ready.
            Arm::Reactive => Action::PromoteCToBeacon,
            Arm::MemoryOnly => best_from_memory(ctx),
            Arm::FullAegis => full_aegis_decide(ctx),
        }
    }
}

fn best_from_memory(ctx: &DecisionContext) -> Action {
    let mut best = Action::HaltB; // safe fallback if memory is empty
    let mut best_mean = f64::NEG_INFINITY;
    for a in Action::all() {
        if !ctx.policy.allows(a, &ctx.belief) {
            continue;
        }
        if let Some(m) = ctx.memory.mean(ctx.symptom, a) {
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
