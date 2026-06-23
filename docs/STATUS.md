<div align="center">

# Aegis Fabric — status

The living tracker: what is built, what is tested, and what is still ahead.
Update this every working session.

[Overview](../README.md) &nbsp;·&nbsp; [Scope](scope.md) &nbsp;·&nbsp; [Working agreement](../CLAUDE.md)

**Last updated:** 2026-06-22 &nbsp;·&nbsp; **Phase:** twin model error (2nd fidelity axis) &nbsp;·&nbsp; **Build:** `cargo test` green (45 tests)

</div>

---

## Legend

| Mark | Meaning |
|---|---|
| ✅ | built **and** tested (unit/property tests assert its behaviour) |
| 🧪 | built, exercised end-to-end, but no dedicated test yet |
| 🟡 | partially built / simplified placeholder |
| ⬜ | not started |
| ⏸ | deliberately deferred (see [frontier](#the-frontier-not-done-stated-plainly)) |

> Honesty rule: a component is only ✅ if its behaviour is actually checked, not
> merely compiled. The suite is **45 tests** — dense `#[test]` modules plus
> seeded oracle/property sweeps in `tests/properties.rs`, hand-rolled (no test
> framework dependency, `#![forbid(unsafe_code)]`).

---

## Component status

| Component | File | State | Notes |
|---|---|:--:|---|
| Deterministic RNG (SplitMix64) | `src/rng.rs` | ✅ | determinism + bounds tested |
| Domain model (robots, actions, faults, symptoms) | `src/model.rs` | ✅ | enum + param-ordering tests |
| Append-only event log | `src/event.rs` | ✅ | ordering + describe tested |
| Two faults: power cascade + beacon interference | `src/sim.rs` | ✅ | independent root causes, same surface symptom |
| Ground-truth world + tick dynamics | `src/sim.rs` | ✅ | battery/beacon/jam/localization on one `step` |
| Safe-mode auto-resume | `src/sim.rs` | ✅ | halted robot re-localizes + resumes when safe |
| The twin: observation fidelity + model calibration | `src/sim.rs`, `Params::twin` | ✅ | two axes; fidelity sweep + physics-miscalibration sweep; faithful@1.0 |
| Closed-loop controller (act→verify→re-decide) | `src/sim.rs` | ✅ | `run_controlled`; sequences actions |
| Diagnosis (root-cause inference) | `src/sim.rs` `diagnose` | ✅ | distinguishes power vs interference; lifts memory 0→49% |
| Incident memory (per-root-cause action stats) | `src/decision.rs` | ✅ | trained over 8k scenarios |
| Policy gate | `src/decision.rs` | ✅ | one rule (no restart-A while B moves), tested |
| Reactive / Memory-only / Full Aegis | `src/decision.rs` | ✅ | ordering + safety tests |
| Experiment harness (single + multi-step, ablation, sweep) | `src/experiment.rs` | ✅ | identical seeds across arms |
| Replay / forensics timeline | `src/replay.rs`, `src/bin/replay.rs` | ✅ | tick-by-tick reconstruction; agrees with the run |
| CI (fmt + clippy + test) | — | ⬜ | runs locally; no workflow yet |

---

## Current result

Seed `0x5151`, 4,000 mixed-fault scenarios per strategy (`cargo run --release`):

| Strategy | Safe% | Success% | Danger% | Score |
|---|--:|--:|--:|--:|
| Reactive | 59.3 | 59.3 | 40.7 | 0.37 |
| Memory-only (diagnosed) | 100.0 | 49.4 | 0.0 | 1.49 |
| Full Aegis (single-step) | 100.0 | 89.7 | 0.0 | 1.90 |
| **Full Aegis (closed loop)** | **100.0** | **100.0** | **0.0** | — |

**Diagnosis ablation** (single-step memory): coarse "beacon down" key → 0.0%
success; diagnosed root-cause key → 49.4% success. Diagnosis is doing real work.

Twin-fidelity sweep (observation noise, single-step Full Aegis): `1.00 → 1.90`,
`0.75 → 1.73`, `0.50 → 1.54`, `0.25 → 1.37`. Degrades gracefully.

Twin-**calibration** sweep (model error, perfect observations): danger rises
`0% → 8% → 23% → 25%` as the twin drifts optimistic — it greenlights the
aggressive fix, so success climbs to 100% while safety collapses to ~75%. A
*wrong* twin is reckless, not merely ineffective.

---

## Phase roadmap (capability thresholds)

| Phase | Capability | Status |
|---|---|:--:|
| 0 | Thesis, laws, event model, causal demo, safe/dangerous definitions | ✅ |
| 1 | Memory: ingest + append-only history + entity identity | 🟡 in-sim only |
| 2 | Replay: reconstruct + deterministic replay + timeline viewer | ✅ |
| 3 | Diagnosis: detect, infer cause, rank explanations | ✅ two root causes |
| 4 | Twin: simulate interventions, compare, reject risky | ✅ |
| 5 | Controlled remediation: apply fixes, verify, store | ✅ multi-step |
| 6 | Knowledge accumulation: improve ranking, reuse memory | ✅ |
| 7 | Expansion: more fleet types, real hardware, more policy | ⏸ |

---

## Next thresholds (recommended order)

1. **A third fault + harder diagnosis** — faults whose symptoms overlap so
   diagnosis must reason under ambiguity, not read a clean flag.
2. **CI** — a fmt + clippy + test workflow so green stays green.
3. **Memory consolidation / online learning** — let memory update *during* a run
   and decay stale lessons, instead of a fixed offline training pass.

---

## The frontier (not done, stated plainly)

Deliberately out of scope for the MVP — naming them is the point:

- **Real-world twin calibration.** The twin is faithful-enough by construction here.
- **Causal inference under noise.** `diagnose` distinguishes two *clean* root
  causes; it is not RCA over noisy, distributed, partially-observable signals.
- **Real hardware / HIL.** Everything is simulated.
- **Enterprise hardening, security, compliance.**
- **A large SDK / plugin ecosystem.**

---

## Known gaps & debt

- **Diagnosis reads a clean signal.** It keys off observable beacon/battery
  state; the two root causes don't yet have overlapping or noisy symptoms.
- **Twin calibration is a synthetic knob,** not learned from reality — it drifts
  the model on a single dial, not from a residual against observed outcomes.
- **No CI.** `fmt`/`clippy`/`test` are run locally, not enforced on push.
