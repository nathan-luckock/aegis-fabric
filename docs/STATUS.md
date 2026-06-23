<div align="center">

# Aegis Fabric — status

The living tracker: what is built, what is tested, and what is still ahead.
Update this every working session.

[Overview](../README.md) &nbsp;·&nbsp; [Scope](scope.md) &nbsp;·&nbsp; [Working agreement](../CLAUDE.md)

**Last updated:** 2026-06-22 &nbsp;·&nbsp; **Phase:** CI · online learning · confidence-aware diagnosis &nbsp;·&nbsp; **Build:** `cargo test` green (54 tests), CI enforced

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
> merely compiled. The suite is **54 tests** — dense `#[test]` modules plus
> seeded oracle/property sweeps in `tests/properties.rs`, hand-rolled (no test
> framework dependency, `#![forbid(unsafe_code)]`), enforced by CI on every push.

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
| Incident memory (offline mean + online EMA) | `src/decision.rs` | ✅ | `record`/`learn`; EMA decays stale lessons |
| Online learning (epsilon-greedy, adapts to drift) | `src/experiment.rs` | ✅ | cold-start curve + static-vs-online drift test |
| Policy gate | `src/decision.rs` | ✅ | one rule (no restart-A while B moves), tested |
| Reactive / Memory-only / Full Aegis / Full Aegis+ | `src/decision.rs` | ✅ | 4 arms; ordering, safety, hedging tests |
| Confidence-aware diagnosis (hedging) | `src/sim.rs` `observe_with_confidence`, `decision.rs` | ✅ | calibrated confidence; cuts danger under ambiguity |
| Experiment harness (single + multi-step, ablation, sweeps) | `src/experiment.rs` | ✅ | identical seeds across arms |
| Replay / forensics timeline | `src/replay.rs`, `src/bin/replay.rs` | ✅ | tick-by-tick reconstruction; agrees with the run |
| CI (fmt + clippy `-D warnings` + test) | `.github/workflows/ci.yml` | ✅ | runs on every push / PR |

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

**Confidence-aware hedging** (Full Aegis+ vs Full Aegis): identical at fidelity
1.0; at 0.75 hedging lifts safety `85.2% → 94.6%` for under 1 point of success;
under extreme noise it over-hedges (honest trade).

**Online learning:** a cold (empty) memory climbs `38% → ~58%` success over 8k
incidents. Under drift (recharge speeds up so `failover` beats `halt` for power),
a static memory stays at `~60%` while an online learner with decay re-learns to
`~90%`.

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
| 6 | Knowledge accumulation: improve ranking, reuse memory, learn online | ✅ |
| 7 | Expansion: more fleet types, real hardware, more policy | ⏸ |

---

## Next thresholds (recommended order)

1. **Durable persistence** — write the event log and memory to disk so they
   survive a restart (closes Phase 1 "append-only history + entity identity"
   beyond in-process).
2. **A second asset type** — generalise past the A/B/C beacon fleet (e.g. a
   compute node with its own faults/fixes) so the runtime is domain-agnostic.
3. **Operator console** — a TUI that streams the live world model + incident feed
   and lets an operator approve/deny a proposed action.

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

- **Everything is in-process.** Memory and the event log are not persisted to
  disk, so nothing survives a restart yet (Phase 1 is in-sim only).
- **Twin calibration is a synthetic knob,** not learned from reality — it drifts
  the model on a single dial, not from a residual against observed outcomes.
- **Confidence is binary-ish.** Hedging keys off a calibrated coin-flip vs
  certain split; a graded posterior would hedge more proportionately.
