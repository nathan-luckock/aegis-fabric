//! The falsifiable experiment: train memory, evaluate the strategies on
//! identical seeded incidents (single-step and closed-loop), print the
//! comparison tables, the twin-fidelity sweep, and a narrated incident.

use crate::decision::{Arm, DecisionContext, IncidentMemory, Policy};
use crate::model::{Action, Params, Symptom};
use crate::rng::Rng;
use crate::sim::{
    action_changes_state, gen_scenario, observe, run_controlled, run_scenario, run_with_log,
    scenario_score, seed_for, ControlConfig, SimOutcome,
};

#[derive(Default)]
struct ArmStats {
    n: u32,
    safe: u32,
    success: u32,
    danger: u32,
    score: f64,
    mttr_sum: u64,
    mttr_n: u32,
}

impl ArmStats {
    fn add(&mut self, o: &SimOutcome) {
        self.n += 1;
        if o.safe {
            self.safe += 1;
        } else {
            self.danger += 1;
        }
        if o.successful {
            self.success += 1;
            if let Some(m) = o.mttr {
                self.mttr_sum += m as u64;
                self.mttr_n += 1;
            }
        }
        self.score += scenario_score(o);
    }

    fn summary(&self) -> Summary {
        let pct = |x: u32| if self.n == 0 { 0.0 } else { x as f64 / self.n as f64 * 100.0 };
        Summary {
            n: self.n,
            safe: pct(self.safe),
            success: pct(self.success),
            danger: pct(self.danger),
            score: if self.n == 0 { 0.0 } else { self.score / self.n as f64 },
            mttr: if self.mttr_n == 0 {
                None
            } else {
                Some(self.mttr_sum as f64 / self.mttr_n as f64)
            },
        }
    }
}

/// Aggregate result of evaluating one strategy.
#[derive(Clone, Copy, Debug)]
pub struct Summary {
    pub n: u32,
    pub safe: f64,    // percent
    pub success: f64, // percent
    pub danger: f64,  // percent
    pub score: f64,   // mean
    pub mttr: Option<f64>,
}

/// Train operational memory by observing the real outcome of randomly-chosen
/// actions across many incidents — "what historically happened when we tried X".
pub fn train_memory(train_n: usize, seed: u64) -> IncidentMemory {
    let p = Params::ground_truth();
    let mut mem = IncidentMemory::new();
    let actions = Action::all();
    for idx in 0..train_n {
        let mut r = Rng::new(seed_for(seed ^ 0x00AB_CDEF, idx));
        let cfg = gen_scenario(&mut r);
        let a = actions[(r.next_u64() % 5) as usize];
        let outcome = run_scenario(&cfg, &p, |_truth| a);
        mem.record(Symptom::BeaconLostBDrifting, a, scenario_score(&outcome));
    }
    mem
}

fn eval(
    arm: &Arm,
    n: usize,
    base_seed: u64,
    belief_seed: u64,
    fidelity: f64,
    mem: &IncidentMemory,
    multistep: bool,
) -> ArmStats {
    let p = Params::ground_truth();
    let twin = Params::ground_truth();
    let policy = Policy;
    let control = ControlConfig::default_loop();
    let mut st = ArmStats::default();

    for idx in 0..n {
        let mut sr = Rng::new(seed_for(base_seed, idx));
        let cfg = gen_scenario(&mut sr);
        let mut br = Rng::new(seed_for(belief_seed, idx));

        let outcome = if multistep {
            run_controlled(&cfg, &p, &control, |state, sym| {
                let belief = observe(state, fidelity, &mut br);
                let ctx = DecisionContext {
                    symptom: sym,
                    belief,
                    horizon: p.horizon,
                    decision_tick: state.decision_tick,
                    twin_params: &twin,
                    memory: mem,
                    policy: &policy,
                };
                arm.decide(&ctx)
            })
        } else {
            run_scenario(&cfg, &p, |truth| {
                let belief = observe(truth, fidelity, &mut br);
                let ctx = DecisionContext {
                    symptom: Symptom::BeaconLostBDrifting,
                    belief,
                    horizon: p.horizon,
                    decision_tick: truth.decision_tick,
                    twin_params: &twin,
                    memory: mem,
                    policy: &policy,
                };
                arm.decide(&ctx)
            })
        };
        st.add(&outcome);
    }
    st
}

/// Evaluate a strategy over `n` seeded incidents and summarise the result.
pub fn evaluate(
    arm: &Arm,
    n: usize,
    seed: u64,
    fidelity: f64,
    mem: &IncidentMemory,
    multistep: bool,
) -> Summary {
    eval(arm, n, seed, seed ^ 0xBEEF, fidelity, mem, multistep).summary()
}

fn print_row(name: &str, s: &Summary) {
    let mttr = s.mttr.map(|m| format!("{m:.1}")).unwrap_or_else(|| "  -".to_string());
    println!(
        "{:<13} {:>6.1} {:>9.1} {:>8.1} {:>8.2} {:>6}",
        name, s.safe, s.success, s.danger, s.score, mttr
    );
}

/// Entry point: run the whole experiment and print the report.
pub fn run(n_eval: usize, train_n: usize, seed: u64) {
    let memory = train_memory(train_n, seed);

    println!("Aegis Fabric — simulate-before-act experiment");
    println!("  {n_eval} eval scenarios / {train_n} train scenarios | seed {seed:#x}\n");
    println!("Incident: shared charger faults -> A drains -> A drops the beacon ->");
    println!("          B (depends on A's beacon) loses localization -> fleet degrades.");
    println!("Each arm picks recovery actions. Safe = no collision-risk state ever;");
    println!("Success = B ended back on task, well localized.\n");

    println!(
        "{:<13} {:>6} {:>9} {:>8} {:>8} {:>6}",
        "Arm", "Safe%", "Success%", "Danger%", "Score", "MTTR"
    );
    println!("{}", "-".repeat(55));
    for arm in [Arm::Reactive, Arm::MemoryOnly, Arm::FullAegis] {
        print_row(arm.name(), &evaluate(&arm, n_eval, seed, 1.0, &memory, false));
    }

    println!("\nTwin fidelity sweep (Full Aegis) — the win is not a perfect-oracle artifact:");
    println!("{:>9} {:>6} {:>9} {:>8} {:>8}", "Fidelity", "Safe%", "Success%", "Danger%", "Score");
    println!("{}", "-".repeat(43));
    for f in [1.0, 0.9, 0.75, 0.5, 0.25] {
        let s = evaluate(&Arm::FullAegis, n_eval, seed, f, &memory, false);
        println!(
            "{:>9.2} {:>6.1} {:>9.1} {:>8.1} {:>8.2}",
            f, s.safe, s.success, s.danger, s.score
        );
    }

    println!("\nMulti-step remediation (act -> verify -> re-decide) vs single-step, fidelity 1.0:");
    println!(
        "{:<13} {:>10} {:>10} {:>10} {:>10}",
        "Strategy", "Safe% (1)", "Succ% (1)", "Safe% (N)", "Succ% (N)"
    );
    println!("{}", "-".repeat(57));
    for arm in [Arm::Reactive, Arm::MemoryOnly, Arm::FullAegis] {
        let one = evaluate(&arm, n_eval, seed, 1.0, &memory, false);
        let many = evaluate(&arm, n_eval, seed, 1.0, &memory, true);
        println!(
            "{:<13} {:>10.1} {:>10.1} {:>10.1} {:>10.1}",
            arm.name(),
            one.safe,
            one.success,
            many.safe,
            many.success
        );
    }

    demo(seed, &memory);
}

/// Narrate one incident: the cascade if nothing is done, then the single-step
/// choices, then the closed-loop sequence.
fn demo(seed: u64, mem: &IncidentMemory) {
    let p = Params::ground_truth();
    let twin = Params::ground_truth();
    let policy = Policy;

    // Pick the hardest regime: C not ready AND slow recharge. No single action
    // both makes B safe and recovers it — only a sequence does.
    let cfg = {
        let mut found = None;
        for k in 0..5000usize {
            let mut r = Rng::new(seed_for(seed ^ 0xD00D, k));
            let c = gen_scenario(&mut r);
            if !c.c_ready && c.charge_rate < 2.2 {
                found = Some(c);
                break;
            }
        }
        found.unwrap_or_else(|| gen_scenario(&mut Rng::new(seed)))
    };

    println!("\n== Demo incident ==");
    println!(
        "Scenario: C_ready={}, A_recharge_rate={:.2}/tick, A_start_battery={:.0}%",
        cfg.c_ready, cfg.charge_rate, cfg.a_init
    );

    let (out0, log) = run_with_log(&cfg, Action::DoNothing, &p);
    println!("\nIf nothing is done (do-nothing):");
    for e in &log.events {
        println!("  t={:>2}  {}", e.tick, e.kind.describe());
    }
    println!("  => safe={}, recovered={}", out0.safe, out0.successful);

    println!("\nSingle-step — each strategy gets ONE action:");
    for arm in [Arm::Reactive, Arm::MemoryOnly, Arm::FullAegis] {
        let mut br = Rng::new(seed_for(seed ^ 0xF00D, 0));
        let mut chosen = Action::DoNothing;
        let outcome = run_scenario(&cfg, &p, |truth| {
            let belief = observe(truth, 1.0, &mut br);
            let ctx = DecisionContext {
                symptom: Symptom::BeaconLostBDrifting,
                belief,
                horizon: p.horizon,
                decision_tick: truth.decision_tick,
                twin_params: &twin,
                memory: mem,
                policy: &policy,
            };
            let a = arm.decide(&ctx);
            chosen = a;
            a
        });
        println!(
            "  {:<12} -> {:<17} safe={:<5} success={}",
            arm.name(),
            chosen.label(),
            outcome.safe,
            outcome.successful
        );
    }

    println!("\nClosed loop — Full Aegis may sequence actions:");
    let mut seq: Vec<Action> = Vec::new();
    let mut br = Rng::new(seed_for(seed ^ 0xF00D, 1));
    let outcome = run_controlled(&cfg, &p, &ControlConfig::default_loop(), |state, sym| {
        let belief = observe(state, 1.0, &mut br);
        let ctx = DecisionContext {
            symptom: sym,
            belief,
            horizon: p.horizon,
            decision_tick: state.decision_tick,
            twin_params: &twin,
            memory: mem,
            policy: &policy,
        };
        let a = Arm::FullAegis.decide(&ctx);
        // Narrate only actions that actually change the world.
        if action_changes_state(a, state) {
            seq.push(a);
        }
        a
    });
    let path: Vec<&str> = seq.iter().map(|a| a.label()).collect();
    println!("  Full Aegis   -> [{}]", path.join(" then "));
    println!("  => safe={}, success={}", outcome.safe, outcome.successful);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_learns_the_safe_default() {
        let mem = train_memory(8000, 0x5151);
        let sym = Symptom::BeaconLostBDrifting;
        let halt = mem.mean(sym, Action::HaltB).expect("halt seen");
        for a in [Action::DoNothing, Action::FailoverCharger, Action::PromoteCToBeacon] {
            let m = mem.mean(sym, a).expect("action seen");
            assert!(halt >= m, "HaltB should be the best historical action ({a:?} = {m})");
        }
    }

    #[test]
    fn strategies_are_ordered_reactive_memory_full() {
        let mem = train_memory(8000, 0x5151);
        let r = evaluate(&Arm::Reactive, 3000, 0x5151, 1.0, &mem, false);
        let m = evaluate(&Arm::MemoryOnly, 3000, 0x5151, 1.0, &mem, false);
        let f = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, false);
        assert!(f.score > m.score, "simulation should beat memory ({} vs {})", f.score, m.score);
        assert!(m.score > r.score, "memory should beat reactive ({} vs {})", m.score, r.score);
        assert!(m.safe >= 99.0 && f.safe >= 99.0, "memory and full aegis are safe");
        assert!(f.success > m.success, "simulation recovers more than memory's safe default");
    }

    #[test]
    fn multistep_lifts_full_aegis_success_at_equal_safety() {
        let mem = train_memory(8000, 0x5151);
        let one = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, false);
        let many = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, true);
        assert!(many.safe >= 99.0, "closed loop stays safe ({})", many.safe);
        assert!(
            many.success > one.success + 5.0,
            "closed loop should recover materially more ({} vs {})",
            many.success,
            one.success
        );
    }

    #[test]
    fn fidelity_degrades_full_aegis_monotonically_ish() {
        let mem = train_memory(8000, 0x5151);
        let hi = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, false);
        let lo = evaluate(&Arm::FullAegis, 3000, 0x5151, 0.25, &mem, false);
        assert!(hi.score > lo.score, "lower twin fidelity should not help ({} vs {})", hi.score, lo.score);
        assert!(hi.danger <= lo.danger, "lower fidelity should not reduce danger");
    }
}
