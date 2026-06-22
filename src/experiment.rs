//! The falsifiable experiment: train memory, evaluate the three arms on
//! identical scenarios, print the comparison table and the twin-fidelity sweep,
//! then narrate one incident end-to-end.

use crate::decision::{Arm, DecisionContext, IncidentMemory, Policy};
use crate::model::{Action, Params, Symptom};
use crate::rng::Rng;
use crate::sim::{
    gen_scenario, observe, run_scenario, run_with_log, scenario_score, seed_for, SimOutcome,
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
}

fn pct(x: u32, n: u32) -> f64 {
    if n == 0 {
        0.0
    } else {
        x as f64 / n as f64 * 100.0
    }
}

/// Train operational memory by observing the real outcome of randomly-chosen
/// actions across many incidents — "what historically happened when we tried X".
fn train_memory(train_n: usize, seed: u64) -> IncidentMemory {
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

#[allow(clippy::too_many_arguments)]
fn eval_arm(
    arm: &Arm,
    n: usize,
    base_seed: u64,
    belief_seed: u64,
    fidelity: f64,
    mem: &IncidentMemory,
    pol: &Policy,
    twin: &Params,
) -> ArmStats {
    let p = Params::ground_truth();
    let mut st = ArmStats::default();
    for idx in 0..n {
        let mut sr = Rng::new(seed_for(base_seed, idx));
        let cfg = gen_scenario(&mut sr);
        let mut br = Rng::new(seed_for(belief_seed, idx));
        let outcome = run_scenario(&cfg, &p, |truth| {
            let belief = observe(truth, fidelity, &mut br);
            let ctx = DecisionContext {
                symptom: Symptom::BeaconLostBDrifting,
                belief,
                horizon: p.horizon,
                decision_tick: truth.decision_tick,
                twin_params: twin,
                memory: mem,
                policy: pol,
            };
            arm.decide(&ctx)
        });
        st.add(&outcome);
    }
    st
}

fn print_row(name: &str, st: &ArmStats) {
    let mttr = if st.mttr_n > 0 {
        format!("{:.1}", st.mttr_sum as f64 / st.mttr_n as f64)
    } else {
        "  -".to_string()
    };
    println!(
        "{:<13} {:>6.1} {:>9.1} {:>8.1} {:>8.2} {:>6}",
        name,
        pct(st.safe, st.n),
        pct(st.success, st.n),
        pct(st.danger, st.n),
        st.score / st.n as f64,
        mttr
    );
}

/// Entry point: run the whole experiment and print the report.
pub fn run(n_eval: usize, train_n: usize, seed: u64) {
    let policy = Policy;
    let twin = Params::ground_truth();
    let memory = train_memory(train_n, seed);

    println!("Aegis Fabric — simulate-before-act experiment");
    println!("  {n_eval} eval scenarios / {train_n} train scenarios | seed {seed:#x}\n");
    println!("Incident: shared charger faults -> A drains -> A drops the beacon ->");
    println!("          B (depends on A's beacon) loses localization -> fleet degrades.");
    println!("Each arm picks ONE recovery action.");
    println!("  Safe%    = no collision-risk state ever occurred");
    println!("  Success% = B ended back on task, well localized");
    println!("  Score    = mean of (safe? +1 : -2) + (success? +1 : 0)\n");

    println!(
        "{:<13} {:>6} {:>9} {:>8} {:>8} {:>6}",
        "Arm", "Safe%", "Success%", "Danger%", "Score", "MTTR"
    );
    println!("{}", "-".repeat(55));
    for arm in [Arm::Reactive, Arm::MemoryOnly, Arm::FullAegis] {
        let st = eval_arm(
            &arm, n_eval, seed, seed ^ 0xBEEF, 1.0, &memory, &policy, &twin,
        );
        print_row(arm.name(), &st);
    }

    println!("\nTwin fidelity sweep (Full Aegis) — the win is not a perfect-oracle artifact:");
    println!(
        "{:>9} {:>6} {:>9} {:>8} {:>8}",
        "Fidelity", "Safe%", "Success%", "Danger%", "Score"
    );
    println!("{}", "-".repeat(43));
    for f in [1.0, 0.9, 0.75, 0.5, 0.25] {
        let st = eval_arm(
            &Arm::FullAegis,
            n_eval,
            seed,
            seed ^ 0xBEEF,
            f,
            &memory,
            &policy,
            &twin,
        );
        println!(
            "{:>9.2} {:>6.1} {:>9.1} {:>8.1} {:>8.2}",
            f,
            pct(st.safe, st.n),
            pct(st.success, st.n),
            pct(st.danger, st.n),
            st.score / st.n as f64
        );
    }

    demo(seed, &memory, &policy, &twin);
}

/// Narrate one incident: the cascade if nothing is done, then each arm's choice.
fn demo(seed: u64, mem: &IncidentMemory, pol: &Policy, twin: &Params) {
    let p = Params::ground_truth();

    // Pick an incident where C is NOT ready — the case that separates the arms.
    let cfg = {
        let mut found = None;
        for k in 0..2000usize {
            let mut r = Rng::new(seed_for(seed ^ 0xD00D, k));
            let c = gen_scenario(&mut r);
            if !c.c_ready {
                found = Some(c);
                break;
            }
        }
        found.unwrap_or_else(|| {
            let mut r = Rng::new(seed);
            gen_scenario(&mut r)
        })
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

    println!("\nWhat each arm decides at the beacon-lost decision point:");
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
                twin_params: twin,
                memory: mem,
                policy: pol,
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
}
