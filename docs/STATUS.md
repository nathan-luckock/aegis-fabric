<div align="center">

# Aegis Fabric — status

The living tracker: what is built, what is tested, and what is still ahead.
Update this every working session.

[Overview](../README.md) &nbsp;·&nbsp; [Scope](scope.md) &nbsp;·&nbsp; [Working agreement](../CLAUDE.md)

**Last updated:** 2026-06-22 &nbsp;·&nbsp; **Phase:** diagnosis under ambiguity (3 faults) &nbsp;·&nbsp; **Build:** `cargo test` green (48 tests)

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
> merely compiled. The suite is **48 tests** — dense `#[test]` modules plus
> seeded oracle/property sweeps in `tests/properties.rs`, hand-rolled (no test
> framework dependency, `#![forbid(unsafe_code)]`).

---

## Component status

| Component | File | State | Notes |
|---|---|:--:|---|
| Deterministic RNG (SplitMix64) | `src/rng.rs` | ✅ | determinism + bounds tested |
| Domain model (robots, actions, faults, symptoms) | `src/model.rs` | ✅ | enum + param-ordering tests |
| Append-only event log | `src/event.rs` | ✅ | ordering + describe tested |
| Three faults: power cascade + interference + brownout | `src/sim.rs` | ✅ | independent root causes; jam & brownout share a surface signature |
| Ground-truth world + tick dynamics | `src/sim.rs` | ✅ | battery/beacon/jam/degrade/signal/localization on one `step` |
| Safe-mode auto-resume | `src/sim.rs` | ✅ | halted robot re-localizes + resumes when safe |
| The twin: observation fidelity + model calibration | `src/sim.rs`, `Params::twin` | ✅ | two axes; fidelity sweep + physics-miscalibration sweep; faithful@1.0 |
| Closed-loop controller (act→verify→re-decide) | `src/sim.rs` | ✅ | `run_controlled`; sequences actions |
| Diagnosis under ambiguity | `src/sim.rs` `diagnose` + `observe` | ✅ | infers root cause from a noisy signal; lifts memory 0→60%; misdiagnosis drives Full-Aegis danger |
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
| Memory-only (diagnosed) | 100.0 | 59.9 | 0.0 | 1.60 |
| Full Aegis (single-step) | 100.0 | 91.9 | 0.0 | 1.92 |
| **Full Aegis (closed loop)** | **100.0** | **100.0** | **0.0** | — |

**Diagnosis ablation** (single-step memory): coarse "beacon down" key → 0.0%
success; diagnosed root-cause key → 59.9% success. Diagnosis is doing real work.

**Diagnosis under ambiguity** (jam vs brownout, told apart only by a noisy
signal): as observation fidelity drops `1.0 → 0.25`, misdiagnosis rises
`0% → 8% → 19% → 27% → 33%` and Full-Aegis danger tracks it `0% → 6% → 15% →
24% → 30%` — the misread *is* the bottleneck; at perfect fidelity the call is
always right, so it stays 100% safe.

Twin-**calibration** sweep (model error, perfect observations): danger rises
`0% → 6% → 18% → 20%` as the twin drifts optimistic — it greenlights the
aggressive fix, so success climbs to 100% while safety collapses to ~80%. A
*wrong* twin is reckless, not merely ineffective.

---

## Phase roadmap (capability thresholds)

| Phase | Capability | Status |
|---|---|:--:|
| 0 | Thesis, laws, event model, causal demo, safe/dangerous definitions | ✅ |
| 1 | Memory: ingest + append-only history + entity identity | 🟡 in-sim only |
| 2 | Replay: reconstruct + deterministic replay + timeline viewer | ✅ |
| 3 | Diagnosis: detect, infer cause, rank explanations | ✅ 3 faults, under ambiguity |
| 4 | Twin: simulate interventions, compare, reject risky | ✅ |
| 5 | Controlled remediation: apply fixes, verify, store | ✅ multi-step |
| 6 | Knowledge accumulation: improve ranking, reuse memory | ✅ |
| 7 | Expansion: more fleet types, real hardware, more policy | ⏸ |

---

## Next thresholds (recommended order)

1. **CI** — a fmt + clippy + test GitHub Actions workflow so green stays green.
2. **Memory consolidation / online learning** — let memory update *during* a run
   and decay stale lessons, instead of a fixed offline training pass.
3. **Confidence-aware diagnosis** — when the signal is ambiguous, output a
   distribution and let Full Aegis hedge (e.g. prefer a fix that's safe under
   *either* fault) instead of committing to a single guess.

---

## The frontier (not done, stated plainly)

Deliberately out of scope for the MVP — naming them is the point:

- **Real-world twin calibration.** The twin is faithful-enough by construction here.
- **Full causal inference.** `diagnose` resolves three faults from one noisy
  signal; it is not RCA over distributed, partially-observable evidence.
- **Real hardware / HIL.** Everything is simulated.
- **Enterprise hardening, security, compliance.**
- **A large SDK / plugin ecosystem.**

---

## Known gaps & debt

- **Diagnosis commits to a single guess.** Under ambiguity it picks the
  most-likely fault rather than carrying a distribution and hedging — so a
  borderline signal is a coin-flip, not a "play it safe under either fault".
- **Twin calibration is a synthetic knob,** not learned from reality — it drifts
  the model on a single dial, not from a residual against observed outcomes.
- **No CI.** `fmt`/`clippy`/`test` are run locally, not enforced on push.
