<div align="center">

# Aegis Fabric — status

The living tracker: what is built, what is tested, and what is still ahead.
Update this every working session.

[Overview](../README.md) &nbsp;·&nbsp; [Scope](scope.md) &nbsp;·&nbsp; [Working agreement](../CLAUDE.md)

**Last updated:** 2026-06-22 &nbsp;·&nbsp; **Phase:** MVP wedge proven &nbsp;·&nbsp; **Build:** `cargo run --release` green

</div>

---

## Legend

| Mark | Meaning |
|---|---|
| ✅ | built **and** validated (behaviour exercised by the experiment) |
| 🧪 | built, exercised end-to-end, but **no dedicated unit/property test yet** |
| 🟡 | partially built / simplified placeholder |
| ⬜ | not started |
| ⏸ | deliberately deferred (see [frontier](#the-frontier-not-done-stated-plainly)) |

> Honesty rule: a component is only ✅ if its behaviour is actually checked, not
> merely compiled. Today the **experiment is the test**; there are no `cargo test`
> unit tests yet, so most components sit at 🧪. That gap is tracked below.

---

## Component status

| Component | File | State | Notes |
|---|---|:--:|---|
| Deterministic RNG (SplitMix64) | `src/rng.rs` | 🧪 | seeded, reproducible; underpins replay law #5 |
| Domain model (robots, actions, symptoms, params) | `src/model.rs` | ✅ | exercised across every scenario |
| Append-only event log | `src/event.rs` | 🧪 | records the incident timeline; used by the demo |
| Ground-truth world + tick dynamics | `src/sim.rs` | ✅ | charger→battery→beacon→localization cascade |
| The twin (noisy belief + fidelity knob) | `src/sim.rs` | ✅ | **separate** from ground truth; fidelity sweep proves it |
| Incident memory (per-symptom action stats) | `src/decision.rs` | ✅ | trained over 8k scenarios; drives Memory-only |
| Policy gate | `src/decision.rs` | 🧪 | one rule (no restart-A while B moves); needs more |
| Reactive strategy | `src/decision.rs` | ✅ | fixed runbook baseline |
| Memory-only strategy | `src/decision.rs` | ✅ | historical-best action |
| Full Aegis strategy | `src/decision.rs` | ✅ | simulate-before-act, policy-gated |
| Experiment harness (3-arm, paired, seeded) | `src/experiment.rs` | ✅ | identical scenarios across arms |
| Twin-fidelity sweep | `src/experiment.rs` | ✅ | 1.00 → 0.25, the anti-oracle proof |
| Narrated incident / replay demo | `src/experiment.rs` | 🧪 | prints the cascade + each arm's choice |
| Diagnosis as a real module | — | 🟡 | symptom is hardcoded at the decision point |
| Multi-step remediation loop | — | ⬜ | one action per incident today |
| Replay viewer (scrubable timeline) | — | ⬜ | event log exists; no UI |
| Unit / property tests (`cargo test`) | — | ⬜ | **the main testing gap** |
| CI (fmt + clippy + test) | — | ⬜ | not set up |

---

## Current result

Seed `0x5151`, 4,000 evaluation scenarios per strategy (reproduce with `cargo run --release`):

| Strategy | Safe% | Success% | Danger% | Score | MTTR |
|---|--:|--:|--:|--:|--:|
| Reactive | 60.8 | 60.8 | 39.2 | 0.43 | 1.0 |
| Memory-only | 100.0 | 0.0 | 0.0 | 1.00 | — |
| **Full Aegis** | **100.0** | **81.3** | **0.0** | **1.81** | 2.1 |

Twin-fidelity sweep (Full Aegis): `1.00 → 1.81`, `0.90 → 1.61`, `0.75 → 1.35`,
`0.50 → 0.85`, `0.25 → 0.31`. Degrades gracefully; below ~0.5 it is no longer
worth simulating — the honest threshold.

**Reading:** memory buys safety (learns the safe default), simulation buys safety
**and** effectiveness. The Reactive→Memory delta isolates what *remembering*
buys; the Memory→Full delta isolates what *simulating* buys on top.

---

## Phase roadmap (capability thresholds)

| Phase | Capability | Status |
|---|---|:--:|
| 0 | Thesis, laws, event model, causal demo, safe/dangerous definitions | ✅ |
| 1 | Memory: ingest + append-only history + entity identity | 🟡 in-sim only |
| 2 | Replay: reconstruct + deterministic replay + timeline viewer | 🟡 log + narration; no viewer |
| 3 | Diagnosis: detect, infer cause, rank explanations | 🟡 hardcoded symptom |
| 4 | Twin: simulate interventions, compare, reject risky | ✅ |
| 5 | Controlled remediation: apply one safe fix, verify, store | ✅ single-step |
| 6 | Knowledge accumulation: improve ranking, reuse memory | ✅ |
| 7 | Expansion: more fleet types, real hardware, more policy | ⏸ |

The MVP cleared Phase 0 and the core of Phases 4–6 in one slice. Phases 1–3 are
real but simplified inside the simulator; the next builds harden them.

---

## Next thresholds (recommended order)

1. **Multi-step remediation loop** — act → verify → re-decide, instead of one-shot.
   Biggest jump in "this feels alive"; the architecture already supports it.
2. **Replay viewer** — turn the event log into a scrubable incident timeline.
3. **Richer failure space + a real diagnosis module** — more fault modes so memory
   and root-cause inference have to work, instead of one fixed symptom.
4. **Second fidelity axis** — miscalibrate the twin's *physics* (not just its
   observations) to map where simulation stops paying off.
5. **Tests + CI** — unit/property tests and a fmt+clippy+test workflow, so 🧪 → ✅.

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

- **No automated tests.** The experiment is the only behavioural check; there are
  no `cargo test` unit or property tests. This is the top debt item.
- **Diagnosis is hardcoded** to the single `BeaconLostBDrifting` symptom.
- **One decision point per incident** — no closed-loop, multi-step recovery.
- **Twin imperfection is belief-noise only**; the twin's physics is not yet
  miscalibrated, so the fidelity sweep covers observation error but not model error.
- **No CI**, so `fmt`/`clippy` are not enforced on push.
