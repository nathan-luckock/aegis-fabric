<div align="center">

# Aegis Fabric — status

The living tracker: what is built, what is tested, and what is still ahead.
Update this every working session.

[Overview](../README.md) &nbsp;·&nbsp; [Scope](scope.md) &nbsp;·&nbsp; [Working agreement](../CLAUDE.md)

**Last updated:** 2026-06-22 &nbsp;·&nbsp; **Phase:** replay & forensics &nbsp;·&nbsp; **Build:** `cargo test` green (37 tests)

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
> merely compiled. The suite is **37 tests** — dense `#[test]` modules plus
> seeded oracle/property sweeps in `tests/properties.rs`, hand-rolled (no test
> framework dependency, `#![forbid(unsafe_code)]`).

---

## Component status

| Component | File | State | Notes |
|---|---|:--:|---|
| Deterministic RNG (SplitMix64) | `src/rng.rs` | ✅ | determinism + bounds tested |
| Domain model (robots, actions, symptoms, params) | `src/model.rs` | ✅ | enum + param-ordering tests |
| Append-only event log | `src/event.rs` | ✅ | ordering + describe tested |
| Ground-truth world + tick dynamics | `src/sim.rs` | ✅ | charger→battery→beacon→localization cascade |
| Safe-mode auto-resume | `src/sim.rs` | ✅ | halted robot re-localizes + resumes when safe |
| The twin (noisy belief + fidelity knob) | `src/sim.rs` | ✅ | separate from truth; fidelity sweep + faithful@1.0 test |
| Closed-loop controller (act→verify→re-decide) | `src/sim.rs` | ✅ | `run_controlled`; sequences actions |
| Incident memory (per-symptom action stats) | `src/decision.rs` | ✅ | trained over 8k scenarios |
| Policy gate | `src/decision.rs` | ✅ | one rule (no restart-A while B moves), tested |
| Reactive / Memory-only / Full Aegis | `src/decision.rs` | ✅ | all three, with ordering + safety tests |
| Experiment harness (single + multi-step, fidelity sweep) | `src/experiment.rs` | ✅ | identical seeds across arms |
| Narrated incident / replay demo | `src/experiment.rs` | ✅ | cascade + per-arm choice + the closed-loop sequence |
| Replay / forensics timeline | `src/replay.rs`, `src/bin/replay.rs` | ✅ | tick-by-tick reconstruction + keyframes; agrees with the run |
| Diagnosis | `src/sim.rs` `diagnose` | 🟡 | coarse (beacon-down / battery-draining); no real RCA |
| CI (fmt + clippy + test) | — | ⬜ | runs locally; no workflow yet |

---

## Current result

Seed `0x5151`, 4,000 evaluation scenarios per strategy (`cargo run --release`):

| Strategy | Safe% | Success% | Danger% | Score |
|---|--:|--:|--:|--:|
| Reactive | 60.8 | 60.8 | 39.2 | 0.43 |
| Memory-only | 100.0 | 0.0 | 0.0 | 1.00 |
| Full Aegis (single-step) | 100.0 | 81.3 | 0.0 | 1.81 |
| **Full Aegis (closed loop)** | **100.0** | **100.0** | **0.0** | — |

Twin-fidelity sweep (single-step Full Aegis): `1.00 → 1.81`, `0.90 → 1.61`,
`0.75 → 1.35`, `0.50 → 0.85`, `0.25 → 0.31`. Degrades gracefully; below ~0.5 it
is no longer worth simulating.

**Reading:** memory buys safety; simulation buys safety + effectiveness; the
closed loop buys *full* recovery by sequencing actions (halt → failover → resume)
in regimes no single action can solve.

---

## Phase roadmap (capability thresholds)

| Phase | Capability | Status |
|---|---|:--:|
| 0 | Thesis, laws, event model, causal demo, safe/dangerous definitions | ✅ |
| 1 | Memory: ingest + append-only history + entity identity | 🟡 in-sim only |
| 2 | Replay: reconstruct + deterministic replay + timeline viewer | ✅ |
| 3 | Diagnosis: detect, infer cause, rank explanations | 🟡 coarse `diagnose` |
| 4 | Twin: simulate interventions, compare, reject risky | ✅ |
| 5 | Controlled remediation: apply fixes, verify, store | ✅ now multi-step |
| 6 | Knowledge accumulation: improve ranking, reuse memory | ✅ |
| 7 | Expansion: more fleet types, real hardware, more policy | ⏸ |

---

## Next thresholds (recommended order)

1. **Richer failure space + a real diagnosis module** — more fault modes so memory
   and root-cause inference have to work, instead of one near-fixed symptom.
2. **Second fidelity axis** — miscalibrate the twin's *physics* (not just its
   observations) to map where simulation stops paying off.
3. **CI** — a fmt + clippy + test workflow so green stays green.

---

## The frontier (not done, stated plainly)

Deliberately out of scope for the MVP — naming them is the point:

- **Real-world twin calibration.** The twin is faithful-enough by construction here.
- **Causal inference from noisy, distributed, partially-observable signals.**
- **Real hardware / HIL.** Everything is simulated.
- **Enterprise hardening, security, compliance.**
- **A large SDK / plugin ecosystem.**

---

## Known gaps & debt

- **Diagnosis is coarse.** `diagnose` keys off beacon/battery state, not a real
  root-cause inference; the memory symptom space is effectively one symptom.
- **Twin imperfection is belief-noise only.** The twin's *physics* is not yet
  miscalibrated, so the fidelity sweep covers observation error but not model error.
- **No CI.** `fmt`/`clippy`/`test` are run locally, not enforced on push.
