<div align="center">

# Aegis Fabric

**operational memory for autonomous fleets**

*A fleet hits a failure cascade at 3am with no one watching. Aegis Fabric remembers it, simulates every candidate fix against a calibrated twin before touching anything real, applies the safest one — and keeps the lesson for next time.*

[![Status](https://img.shields.io/badge/status-MVP%20%C2%B7%20pre--alpha-orange?style=flat-square)](docs/STATUS.md)
[![Safe recovery](https://img.shields.io/badge/safe%20recovery-100%25-3FB950?style=flat-square)](#the-numbers)
[![Closed loop](https://img.shields.io/badge/closed%20loop-100%25%20recovered-3FB950?style=flat-square)](#closing-the-loop)
[![Dependencies](https://img.shields.io/badge/dependencies-0-8957e5?style=flat-square&logo=rust&logoColor=white)](#)
[![Tests](https://img.shields.io/badge/tests-48%20passing-3FB950?style=flat-square&logo=rust&logoColor=white)](#why-the-numbers-hold)
[![Unsafe](https://img.shields.io/badge/unsafe-forbidden-CE422B?style=flat-square&logo=rust&logoColor=white)](#)
[![License](https://img.shields.io/badge/license-MIT-2F81F7?style=flat-square)](#license)

</div>

```
   power cascade ──┐
   beacon jam ─────┼──▶  B loses localization  ──▶  three faults, three different fixes
   brownout ───────┘        (one symptom)            (and a jam vs a brownout look the same)

        ┌─────────────────────────────┬─────────────────────────────┐
        ▼                             ▼                             ▼
     Reactive                    Memory-only                    Full Aegis
   one fixed move            diagnose, then recall           simulate, then act
   59% safe / 59% ok          100% safe / 60% ok             100% safe / 100% ok
```

> Three independent faults surface as the *same* symptom but need *different* root fixes — and two of them (a jammed channel and a degraded transmitter) look identical except for a noisy signal, so diagnosis must reason under ambiguity. Reacting blindly is fast and often dangerous. Remembering only works once you **diagnose** the root cause. Simulating the fix first is safe and effective on all three — and in a closed loop it recovers *every* incident.

---

## Run it

```bash
cargo run --release      # 4000 incidents × 3 strategies: the tables + narrated incidents
cargo test --release     # 48 tests (unit + seeded property/oracle sweeps)
```

Every incident is a pure function of one `u64` seed against the world, so the output regenerates exactly from this commit and any failure replays bit-for-bit.

## The numbers

Each incident carries one of **three independent root causes**, each needing a different fix:

- **power cascade** — a shared charger faults → robot **A** drains offline → its beacon dies. *Root fix: failover the charger.*
- **interference** — A is healthy, but its beacon channel is jammed. *Root fix: retune to a clear channel.*
- **brownout** — A is healthy and on a clear channel, but its transmitter has degraded. *Root fix: power-cycle the radio (failover).*

All three look identical on the surface ("B is losing localization"), and `failover` does nothing for a jam while `retune` does nothing for a dead transmitter — so the right move depends on the *diagnosed* root cause. Across 4,000 mixed incidents, one action each:

| Strategy | Safe% | Success% | Danger% | Score | What it does |
|---|--:|--:|--:|--:|---|
| Reactive | 59.3 | 59.3 | 40.7 | 0.37 | one fixed runbook move — fault-blind |
| Memory-only | 100.0 | 59.9 | 0.0 | 1.60 | diagnose the root cause, recall the best historical fix |
| **Full Aegis** | **100.0** | **91.9** | **0.0** | **1.92** | simulate every allowed fix, pick the safest viable |

Reactive can't win because no single fixed move is right for three faults. Memory-only does — *but only because it's keyed on the diagnosed root cause.*

### Diagnosis earns its keep

Key the exact same memory on the **coarse** "beacon is down" symptom instead of the diagnosed root cause, and it collapses to the safe-but-useless default (always halt). Keying on the diagnosis is what lets it recall the right fix per fault:

| Memory keyed on | Safe% | Success% |
|---|--:|--:|
| coarse "beacon down" | 100.0 | 0.0 |
| **diagnosed root cause** | **100.0** | **59.9** |

That 0 → 60% is the diagnosis layer doing real work — not a label that never changed an outcome.

### Diagnosis under ambiguity

Two of the faults — a jammed channel and a degraded transmitter — are *identical* in every obvious signal (A online, battery fine, beacon down). They're told apart only by a noisy `signal_reading`, so diagnosis is a real inference that misfires under observation noise — and a misdiagnosis sends the twin to simulate the *wrong* fault and apply the wrong fix:

| Twin fidelity | 1.00 | 0.90 | 0.75 | 0.50 | 0.25 |
|---|--:|--:|--:|--:|--:|
| Misdiagnosed% | 0.0 | 8.4 | 18.9 | 27.4 | 32.5 |
| Full-Aegis Danger% | 0.0 | 6.3 | 14.8 | 24.0 | 30.4 |

Danger tracks misdiagnosis almost one-for-one — the inference *is* the bottleneck. At perfect fidelity the call is always right, so simulate-before-act is still 100% safe; the floor only cracks once the signal blurs.

**The win is not a perfect-oracle artifact.** The twin runs on a deliberately-noisy *belief* of the world, governed by a fidelity knob. As fidelity drops, simulate-before-act degrades gracefully — and once the twin is wrong often enough, it stops beating plain memory. That crossover is the honest research frontier, not a number to bury:

| Twin fidelity | 1.00 | 0.90 | 0.75 | 0.50 | 0.25 |
|---|--:|--:|--:|--:|--:|
| Safe% | 100.0 | 93.7 | 85.2 | 76.0 | 69.6 |
| Score | 1.92 | 1.67 | 1.33 | 0.98 | 0.72 |

**Two ways the twin can be wrong, and they fail differently.** The sweep above is *observation* error — a noisy sensor. The other axis is *model* error: keep the observations perfect, but let the twin's physics drift optimistic (it thinks B drifts slower than it really does). The failure mode inverts — the twin greenlights the aggressive fix (`failover`), so it recovers *more* incidents but walks them through a danger window:

| Twin calibration | 1.00 | 0.80 | 0.60 | 0.40 | 0.20 |
|---|--:|--:|--:|--:|--:|
| Safe% | 100.0 | 93.7 | 81.9 | 80.1 | 80.1 |
| Success% | 91.9 | 94.5 | 99.4 | 100.0 | 100.0 |
| Danger% | 0.0 | 6.3 | 18.1 | 19.9 | 19.9 |

A *noisy* twin loses safety and success together. A *wrong* twin keeps — even raises — success while safety collapses: it's confident and reckless. That's the precise failure that makes simulate-before-act dangerous if you over-trust the model, and the knob says where it starts.

### Closing the loop

One action can't always both make B *safe* and *recover* it — when the spare robot isn't ready and the charger recharges slowly, the only safe single move is to halt B and strand it. The **closed-loop controller** (act → verify → re-decide) sequences moves instead: halt B to make it safe, fail the charger over to recover A, and let B auto-resume once the beacon is back. No single action gets there.

| Full Aegis | Safe% | Success% |
|---|--:|--:|
| Single-step | 100.0 | 91.9 |
| **Closed loop** | **100.0** | **100.0** |

Same safety, every stranded incident recovered. Reactive and Memory-only don't move — neither can sequence. The narrated power incident from `cargo run`:

```text
Single-step  Full Aegis →  halt-B                          safe ✓  recovered ✗
Closed loop  Full Aegis →  halt-B → failover-charger        safe ✓  recovered ✓
```

### Replay any incident

Every incident reconstructs from its seed into a tick-by-tick forensic timeline — A's battery, the beacon, B's localization, and what the controller did, frame by frame:

```bash
cargo run --release --bin replay -- 3 full       # scenario #3, Full Aegis (keyframes)
cargo run --release --bin replay -- 3 reactive   # the same incident, the reactive failure
cargo run --release --bin replay -- 3 full --all # every tick, not just keyframes
```

```text
scenario: fault=interference  C_ready=true  A_recharge=3.99/tick  A_start=52%
   t  A battery   beacon B localize   B       events / action
   6  [#####.]  76%  down   [#####.] 0.86  ok      beacon jammed (interference); beacon lost
   7  [#####.]  80%  up     [######] 1.00  ok      → switch-channel  |  channel cleared; beacon restored; recovered
verdict: safe=true  recovered=true  time-to-recover=1 ticks
```

A is fully charged — this is a jam, not a power loss — so Full Aegis retunes the channel rather than failing the charger over. It runs on the same `step` engine as the experiment, so a replay can never disagree with the run it reconstructs, and a test asserts exactly that.

## How it works

```text
   seeded scenario ─▶ ground-truth world  (power cascade | beacon jam | brownout)
                              │ appends to the event log
                              ▼
   beacon drops ─▶ observe ▸ noisy belief ─▶ diagnose ▸ root cause (may be wrong)
                              │                              │
                              ▼                              ▼
                            TWIN ◀── built from the diagnosis ── simulate each fix
                              │ (fidelity + calibration knobs)
                              ▼
        policy gate ◀──── deciders ▸ Reactive · Memory-only · Full Aegis
                              │ safest viable action
                              ▼
        apply ─▶ verify ─▶ (still unsafe? re-decide) ─▶ score ─▶ memory
```

The thing that makes it more than automation is the **memory**: every event, action, and outcome is durable, so diagnosis, simulation, and recovery all read from one history instead of three disconnected tools. The full thesis and the seven core laws live in [docs/scope.md](docs/scope.md).

## What's inside

| | |
|---|---|
| **Ground-truth world** | three faults (power cascade, beacon jam, brownout) on one deterministic tick engine ([`sim.rs`](src/sim.rs)) |
| **The twin** | the *same* dynamics on a noisy *belief*, with two knobs — observation *fidelity* and model *calibration* — a separate path, so "simulation helps" can't be a tautology |
| **Diagnosis** | infers the root cause behind a shared symptom from a noisy signal (`diagnose`); a misread sends the twin to the wrong fault |
| **Operational memory** | per-root-cause action outcomes; the compounding lesson store ([`decision.rs`](src/decision.rs)) |
| **Policy gate** | forbids high-risk actions in context (e.g. restart the beacon anchor while B is moving) |
| **Closed-loop controller** | act → verify → re-decide; sequences actions and auto-resumes a halted robot once it's safe |
| **Three strategies** | Reactive, Memory-only, Full Aegis — behind one `decide()` |

## Why the numbers hold

Nothing here is asserted — it's measured, and the measurement regenerates:

- **Deterministic simulation.** One `u64` seed *is* the incident, so every result replays exactly. The three strategies are scored on *identical* scenarios, for a fair paired comparison.
- **A separated twin.** The decider never sees ground truth, only a fidelity-controlled belief — and the fidelity sweep proves the advantage is real and bounded, not an oracle predicting itself.
- **48 hand-rolled tests.** Dense `#[test]` modules plus seeded oracle/property sweeps over thousands of cases ([`tests/properties.rs`](tests/properties.rs)): determinism & replay, *HaltB is never dangerous*, *each fix works for its fault and no other*, *a jam and a brownout differ only in the signal*, *diagnosis lifts memory's success*, *misdiagnosis rises with noise*, *a faithful twin never picks danger*, *a miscalibrated twin does*, *degrading the twin never helps*, and strategy ordering. No test-framework dependency; `#![forbid(unsafe_code)]`; clippy-clean.

## Honest scoping

This repo is the **MVP wedge** — a simulated fleet that proves the loop end-to-end. By design it does **not** yet solve:

- real-world twin calibration (here the twin's calibration is a synthetic knob, not learned from residuals),
- full causal inference (diagnosis here resolves three faults from one noisy signal — real, but not RCA over distributed, partially-observable evidence),
- real hardware, enterprise hardening, security/compliance.

Those are the frontier, tracked honestly in [docs/STATUS.md](docs/STATUS.md). Naming what isn't solved is the point — a green check that never ran is worth nothing.

## Docs

| | |
|---|---|
| [docs/STATUS.md](docs/STATUS.md) | what's built, what's tested, what's still ahead — the living tracker |
| [docs/scope.md](docs/scope.md) | the full thesis, the seven core laws, the layered architecture |
| [CLAUDE.md](CLAUDE.md) | working agreement and project memory for anyone picking this up |

## License

[MIT](LICENSE).
