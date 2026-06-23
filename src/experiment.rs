//! The falsifiable experiment: train memory, evaluate the strategies on
//! identical seeded incidents (single-step and closed-loop, across both fault
//! types), print the comparison tables, the diagnosis ablation, the
//! twin-fidelity sweep, and two narrated incidents.

use crate::decision::{Arm, DecisionContext, IncidentMemory, Policy};
use crate::model::{Action, Fault, Params, Symptom};
use crate::rng::Rng;
use crate::sim::{
    action_changes_state, diagnose, diagnose_coarse, gen_scenario, observe, run_controlled,
    run_scenario, run_with_log, scenario_score, seed_for, ControlConfig, ScenarioCfg, SimOutcome,
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
        let pct = |x: u32| {
            if self.n == 0 {
                0.0
            } else {
                x as f64 / self.n as f64 * 100.0
            }
        };
        Summary {
            n: self.n,
            safe: pct(self.safe),
            success: pct(self.success),
            danger: pct(self.danger),
            score: if self.n == 0 {
                0.0
            } else {
                self.score / self.n as f64
            },
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
/// actions across many incidents. `diagnosed` controls the memory key: the
/// diagnosed root cause, or the coarse "beacon down" symptom (the baseline).
fn train_mem(train_n: usize, seed: u64, diagnosed: bool) -> IncidentMemory {
    let p = Params::ground_truth();
    let mut mem = IncidentMemory::new();
    let actions = Action::all();
    for idx in 0..train_n {
        let mut r = Rng::new(seed_for(seed ^ 0x00AB_CDEF, idx));
        let cfg = gen_scenario(&mut r);
        let a = actions[(r.next_u64() % actions.len() as u64) as usize];
        let mut sym = Symptom::Nominal;
        let outcome = run_scenario(&cfg, &p, |truth| {
            sym = if diagnosed {
                diagnose(truth)
            } else {
                diagnose_coarse(truth)
            };
            a
        });
        mem.record(sym, a, scenario_score(&outcome));
    }
    mem
}

/// Train memory keyed on the diagnosed root cause (the production keying).
pub fn train_memory(train_n: usize, seed: u64) -> IncidentMemory {
    train_mem(train_n, seed, true)
}

#[allow(clippy::too_many_arguments)]
fn eval(
    arm: &Arm,
    n: usize,
    base_seed: u64,
    belief_seed: u64,
    fidelity: f64,
    calibration: f64,
    mem: &IncidentMemory,
    multistep: bool,
    diagnosed: bool,
) -> ArmStats {
    let p = Params::ground_truth();
    // The twin's model: `calibration` 1.0 is a perfect copy of ground truth,
    // lower values drift its dynamics (model error, separate from belief noise).
    let twin = Params::twin(calibration);
    let policy = Policy;
    let control = ControlConfig::default_loop();
    let mut st = ArmStats::default();

    for idx in 0..n {
        let cfg = gen_scenario(&mut Rng::new(seed_for(base_seed, idx)));
        let mut br = Rng::new(seed_for(belief_seed, idx));

        let outcome = if multistep {
            run_controlled(&cfg, &p, &control, |state, _sym| {
                // Diagnose from the *belief* (noisy), not the truth: a
                // misdiagnosis must mislead both the memory key and the twin.
                let belief = observe(state, fidelity, &mut br);
                let sym = diagnose(&belief);
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
                let sym = if diagnosed {
                    diagnose(&belief)
                } else {
                    diagnose_coarse(&belief)
                };
                let ctx = DecisionContext {
                    symptom: sym,
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

/// Evaluate a strategy over `n` seeded incidents (diagnosed keying).
pub fn evaluate(
    arm: &Arm,
    n: usize,
    seed: u64,
    fidelity: f64,
    mem: &IncidentMemory,
    multistep: bool,
) -> Summary {
    eval(
        arm,
        n,
        seed,
        seed ^ 0xBEEF,
        fidelity,
        1.0,
        mem,
        multistep,
        true,
    )
    .summary()
}

fn print_row(name: &str, s: &Summary) {
    let mttr = s
        .mttr
        .map(|m| format!("{m:.1}"))
        .unwrap_or_else(|| "  -".to_string());
    println!(
        "{:<13} {:>6.1} {:>9.1} {:>8.1} {:>8.2} {:>6}",
        name, s.safe, s.success, s.danger, s.score, mttr
    );
}

/// Measure how often the runtime misreads a jam as a brownout (or vice versa) —
/// the ambiguous pair — at a given observation fidelity.
fn misdiagnosis_rate(n: usize, seed: u64, fidelity: f64) -> f64 {
    let p = Params::ground_truth();
    let (mut ambiguous, mut wrong) = (0u32, 0u32);
    for idx in 0..n {
        let cfg = gen_scenario(&mut Rng::new(seed_for(seed, idx)));
        let mut br = Rng::new(seed_for(seed ^ 0xBEEF, idx));
        run_scenario(&cfg, &p, |truth| {
            // The ambiguous case: the beacon is down while A is still online.
            if truth.a_online && !truth.beacon_up {
                ambiguous += 1;
                let belief = observe(truth, fidelity, &mut br);
                if diagnose(&belief) != diagnose(truth) {
                    wrong += 1;
                }
            }
            Action::HaltB
        });
    }
    if ambiguous == 0 {
        0.0
    } else {
        wrong as f64 / ambiguous as f64 * 100.0
    }
}

/// Entry point: run the whole experiment and print the report.
pub fn run(n_eval: usize, train_n: usize, seed: u64) {
    let mem = train_memory(train_n, seed);
    let mem_coarse = train_mem(train_n, seed, false);

    println!("Aegis Fabric — simulate-before-act experiment");
    println!("  {n_eval} eval scenarios / {train_n} train scenarios | seed {seed:#x}\n");
    println!("Three faults, all surfacing as 'B loses localization', each needing a");
    println!("different root fix:");
    println!("  - power cascade: charger faults, A drains offline      -> failover");
    println!("  - interference:  A healthy, beacon channel jammed      -> switch-channel");
    println!("  - brownout:      A healthy, transmitter degraded       -> failover");
    println!("Interference and brownout look identical but for a noisy signal reading,");
    println!("so diagnosis must reason under ambiguity.\n");

    println!(
        "{:<13} {:>6} {:>9} {:>8} {:>8} {:>6}",
        "Arm", "Safe%", "Success%", "Danger%", "Score", "MTTR"
    );
    println!("{}", "-".repeat(55));
    for arm in [Arm::Reactive, Arm::MemoryOnly, Arm::FullAegis] {
        print_row(arm.name(), &evaluate(&arm, n_eval, seed, 1.0, &mem, false));
    }

    println!("\nDoes diagnosis earn its keep? Memory keyed on the diagnosed root cause");
    println!("vs the coarse 'beacon down' symptom (single-step, fidelity 1.0):");
    println!(
        "{:<24} {:>6} {:>9} {:>8}",
        "Memory keyed on", "Safe%", "Success%", "Score"
    );
    println!("{}", "-".repeat(49));
    let coarse = eval(
        &Arm::MemoryOnly,
        n_eval,
        seed,
        seed ^ 0xBEEF,
        1.0,
        1.0,
        &mem_coarse,
        false,
        false,
    )
    .summary();
    let diag = eval(
        &Arm::MemoryOnly,
        n_eval,
        seed,
        seed ^ 0xBEEF,
        1.0,
        1.0,
        &mem,
        false,
        true,
    )
    .summary();
    println!(
        "{:<24} {:>6.1} {:>9.1} {:>8.2}",
        "coarse (no diagnosis)", coarse.safe, coarse.success, coarse.score
    );
    println!(
        "{:<24} {:>6.1} {:>9.1} {:>8.2}",
        "diagnosed root cause", diag.safe, diag.success, diag.score
    );

    println!("\nTwin fidelity sweep (Full Aegis) — the win is not a perfect-oracle artifact:");
    println!(
        "{:>9} {:>6} {:>9} {:>8} {:>8}",
        "Fidelity", "Safe%", "Success%", "Danger%", "Score"
    );
    println!("{}", "-".repeat(43));
    for f in [1.0, 0.9, 0.75, 0.5, 0.25] {
        let s = evaluate(&Arm::FullAegis, n_eval, seed, f, &mem, false);
        println!(
            "{:>9.2} {:>6.1} {:>9.1} {:>8.1} {:>8.2}",
            f, s.safe, s.success, s.danger, s.score
        );
    }

    println!("\nTwin *physics* miscalibration (Full Aegis, perfect observations) — model");
    println!("error, not observation noise: an optimistic twin rates risk as safe, so");
    println!("danger *rises*. A wrong twin is more dangerous than a merely noisy one:");
    println!(
        "{:>11} {:>6} {:>9} {:>8} {:>8}",
        "Calibration", "Safe%", "Success%", "Danger%", "Score"
    );
    println!("{}", "-".repeat(45));
    for cal in [1.0, 0.8, 0.6, 0.4, 0.2] {
        let s = eval(
            &Arm::FullAegis,
            n_eval,
            seed,
            seed ^ 0xBEEF,
            1.0,
            cal,
            &mem,
            false,
            true,
        )
        .summary();
        println!(
            "{:>11.2} {:>6.1} {:>9.1} {:>8.1} {:>8.2}",
            cal, s.safe, s.success, s.danger, s.score
        );
    }

    println!("\nDiagnosis under ambiguity — a jam and a brownout are identical except for a");
    println!("noisy signal reading. As observation fidelity drops the runtime misreads one");
    println!("for the other, and Full Aegis is led to simulate (and apply) the wrong fix:");
    println!(
        "{:>9} {:>14} {:>20}",
        "Fidelity", "Misdiagnosed%", "Full-Aegis Danger%"
    );
    println!("{}", "-".repeat(45));
    for f in [1.0, 0.9, 0.75, 0.5, 0.25] {
        let mis = misdiagnosis_rate(n_eval, seed, f);
        let danger = evaluate(&Arm::FullAegis, n_eval, seed, f, &mem, false).danger;
        println!("{:>9.2} {:>14.1} {:>20.1}", f, mis, danger);
    }

    println!("\nMulti-step remediation (act -> verify -> re-decide) vs single-step, fidelity 1.0:");
    println!(
        "{:<13} {:>10} {:>10} {:>10} {:>10}",
        "Strategy", "Safe% (1)", "Succ% (1)", "Safe% (N)", "Succ% (N)"
    );
    println!("{}", "-".repeat(57));
    for arm in [Arm::Reactive, Arm::MemoryOnly, Arm::FullAegis] {
        let one = evaluate(&arm, n_eval, seed, 1.0, &mem, false);
        let many = evaluate(&arm, n_eval, seed, 1.0, &mem, true);
        println!(
            "{:<13} {:>10.1} {:>10.1} {:>10.1} {:>10.1}",
            arm.name(),
            one.safe,
            one.success,
            many.safe,
            many.success
        );
    }

    narrate(
        "Power cascade — C not ready, slow charge",
        &find(seed ^ 0xD00D, |c| {
            c.fault == Fault::PowerCascade && !c.c_ready && c.charge_rate < 2.2
        }),
        &mem,
    );
    narrate(
        "Interference — C not ready",
        &find(seed ^ 0xBEAD, |c| {
            c.fault == Fault::Interference && !c.c_ready
        }),
        &mem,
    );
    narrate(
        "Brownout — C not ready (looks like interference, needs failover)",
        &find(seed ^ 0xB202, |c| c.fault == Fault::Brownout && !c.c_ready),
        &mem,
    );
}

/// Find the first seeded scenario matching `pred`.
fn find(base: u64, pred: impl Fn(&ScenarioCfg) -> bool) -> ScenarioCfg {
    for k in 0..20_000usize {
        let c = gen_scenario(&mut Rng::new(seed_for(base, k)));
        if pred(&c) {
            return c;
        }
    }
    gen_scenario(&mut Rng::new(base))
}

/// Narrate one incident end-to-end: the do-nothing cascade, the single-step
/// choices, and the closed-loop sequence.
fn narrate(title: &str, cfg: &ScenarioCfg, mem: &IncidentMemory) {
    let p = Params::ground_truth();
    let twin = Params::ground_truth();
    let policy = Policy;

    println!("\n== {title} ==");
    println!(
        "fault={}  C_ready={}  A_recharge={:.2}  A_start={:.0}%",
        cfg.fault.label(),
        cfg.c_ready,
        cfg.charge_rate,
        cfg.a_init
    );

    let (out0, log) = run_with_log(cfg, Action::DoNothing, &p);
    println!("if nothing is done:");
    for e in &log.events {
        println!("  t={:>2}  {}", e.tick, e.kind.describe());
    }
    println!("  => safe={}, recovered={}", out0.safe, out0.successful);

    println!("single-step — one action each:");
    for arm in [Arm::Reactive, Arm::MemoryOnly, Arm::FullAegis] {
        let mut br = Rng::new(7);
        let mut chosen = Action::DoNothing;
        let outcome = run_scenario(cfg, &p, |truth| {
            let belief = observe(truth, 1.0, &mut br);
            let ctx = DecisionContext {
                symptom: diagnose(truth),
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

    let mut seq: Vec<Action> = Vec::new();
    let mut br = Rng::new(11);
    let outcome = run_controlled(cfg, &p, &ControlConfig::default_loop(), |state, sym| {
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
        if action_changes_state(a, state) {
            seq.push(a);
        }
        a
    });
    let path: Vec<&str> = seq.iter().map(|a| a.label()).collect();
    println!(
        "closed loop  Full Aegis -> [{}]  safe={} success={}",
        path.join(" then "),
        outcome.safe,
        outcome.successful
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_learns_the_right_fix_per_root_cause() {
        let mem = train_memory(8000, 0x5151);
        // Power cascade: HaltB is the safest single action (the loop recovers later).
        let halt = mem
            .mean(Symptom::BeaconLostPower, Action::HaltB)
            .expect("seen");
        for a in Action::all() {
            if a != Action::HaltB {
                if let Some(m) = mem.mean(Symptom::BeaconLostPower, a) {
                    assert!(halt >= m, "power: HaltB should be best, but {a:?} = {m}");
                }
            }
        }
        // Interference: switching the channel is the clear winner.
        let switch = mem
            .mean(Symptom::BeaconLostInterference, Action::SwitchBeaconChannel)
            .expect("seen");
        for a in Action::all() {
            if a != Action::SwitchBeaconChannel {
                if let Some(m) = mem.mean(Symptom::BeaconLostInterference, a) {
                    assert!(
                        switch >= m,
                        "interference: switch-channel should be best, but {a:?} = {m}"
                    );
                }
            }
        }
        // Brownout: failover (power-cycle the radio) is the winner — and notably
        // NOT switch-channel, the fix for its look-alike, interference.
        let failover = mem
            .mean(Symptom::BeaconLostBrownout, Action::FailoverCharger)
            .expect("seen");
        for a in Action::all() {
            if a != Action::FailoverCharger {
                if let Some(m) = mem.mean(Symptom::BeaconLostBrownout, a) {
                    assert!(
                        failover >= m,
                        "brownout: failover should be best, but {a:?} = {m}"
                    );
                }
            }
        }
    }

    #[test]
    fn diagnosis_lifts_memory_success() {
        let mem_diag = train_memory(8000, 0x5151);
        let mem_coarse = train_mem(8000, 0x5151, false);
        let coarse = eval(
            &Arm::MemoryOnly,
            4000,
            0x5151,
            0x5151 ^ 0xBEEF,
            1.0,
            1.0,
            &mem_coarse,
            false,
            false,
        )
        .summary();
        let diag = eval(
            &Arm::MemoryOnly,
            4000,
            0x5151,
            0x5151 ^ 0xBEEF,
            1.0,
            1.0,
            &mem_diag,
            false,
            true,
        )
        .summary();
        assert!(coarse.safe >= 99.0 && diag.safe >= 99.0, "both stay safe");
        assert!(
            diag.success > coarse.success + 10.0,
            "diagnosis should materially lift memory's success ({} vs {})",
            diag.success,
            coarse.success
        );
    }

    #[test]
    fn ambiguity_misdiagnosis_rises_with_observation_noise() {
        let clean = misdiagnosis_rate(4000, 0x5151, 1.0);
        let noisy = misdiagnosis_rate(4000, 0x5151, 0.25);
        assert_eq!(
            clean, 0.0,
            "perfect observation -> a perfect jam/brownout call"
        );
        assert!(
            noisy > 5.0,
            "noise should cause real misdiagnosis ({noisy}% vs {clean}%)"
        );
    }

    #[test]
    fn strategies_are_ordered_reactive_memory_full() {
        let mem = train_memory(8000, 0x5151);
        let r = evaluate(&Arm::Reactive, 3000, 0x5151, 1.0, &mem, false);
        let m = evaluate(&Arm::MemoryOnly, 3000, 0x5151, 1.0, &mem, false);
        let f = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, false);
        assert!(
            f.score > m.score,
            "simulation should beat memory ({} vs {})",
            f.score,
            m.score
        );
        assert!(
            m.score > r.score,
            "memory should beat reactive ({} vs {})",
            m.score,
            r.score
        );
        assert!(
            m.safe >= 99.0 && f.safe >= 99.0,
            "memory and full aegis are safe"
        );
        assert!(
            f.success > m.success,
            "simulation recovers more than memory's safe default"
        );
    }

    #[test]
    fn multistep_lifts_full_aegis_success_at_equal_safety() {
        let mem = train_memory(8000, 0x5151);
        let one = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, false);
        let many = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, true);
        assert!(many.safe >= 99.0, "closed loop stays safe ({})", many.safe);
        assert!(
            many.success > one.success + 3.0,
            "closed loop should recover materially more ({} vs {})",
            many.success,
            one.success
        );
    }

    #[test]
    fn fidelity_degrades_full_aegis() {
        let mem = train_memory(8000, 0x5151);
        let hi = evaluate(&Arm::FullAegis, 3000, 0x5151, 1.0, &mem, false);
        let lo = evaluate(&Arm::FullAegis, 3000, 0x5151, 0.25, &mem, false);
        assert!(
            hi.score > lo.score,
            "lower twin fidelity should not help ({} vs {})",
            hi.score,
            lo.score
        );
        assert!(
            hi.danger <= lo.danger,
            "lower fidelity should not reduce danger"
        );
    }

    #[test]
    fn a_miscalibrated_twin_is_dangerous_even_with_perfect_observations() {
        let mem = train_memory(8000, 0x5151);
        // Perfect twin + perfect observations = oracle = always safe.
        let good = eval(
            &Arm::FullAegis,
            4000,
            0x5151,
            0x5151 ^ 0xBEEF,
            1.0,
            1.0,
            &mem,
            false,
            true,
        )
        .summary();
        // Same perfect observations, but a drifting (optimistic) twin model.
        let bad = eval(
            &Arm::FullAegis,
            4000,
            0x5151,
            0x5151 ^ 0xBEEF,
            1.0,
            0.2,
            &mem,
            false,
            true,
        )
        .summary();
        assert_eq!(good.danger, 0.0, "a faithful twin never picks danger");
        assert!(
            bad.danger > good.danger,
            "model error should introduce danger ({} vs {})",
            bad.danger,
            good.danger
        );
        assert!(
            bad.score < good.score,
            "a wrong twin should lower the score"
        );
    }
}
