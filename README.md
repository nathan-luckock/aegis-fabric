<div align="center">

# Aegis Fabric

**operational memory for autonomous fleets**

*A fleet hits a failure cascade at 3am with no one watching. Aegis Fabric remembers it, simulates every candidate fix against a calibrated twin before touching anything real, applies the safest one — and keeps the lesson for next time.*

[![Status](https://img.shields.io/badge/status-MVP%20%C2%B7%20pre--alpha-orange?style=flat-square)](docs/STATUS.md)
[![Safe recovery](https://img.shields.io/badge/safe%20recovery-100%25-3FB950?style=flat-square)](#the-numbers)
[![Closed loop](https://img.shields.io/badge/closed%20loop-100%25%20recovered-3FB950?style=flat-square)](#closing-the-loop)
[![Dependencies](https://img.shields.io/badge/dependencies-0-8957e5?style=flat-square&logo=rust&logoColor=white)](#)
[![Tests](https://img.shields.io/badge/tests-45%20passing-3FB950?style=flat-square&logo=rust&logoColor=white)](#why-the-numbers-hold)
[![Unsafe](https://img.shields.io/badge/unsafe-forbidden-CE422B?style=flat-square&logo=rust&logoColor=white)](#)
[![License](https://img.shields.io/badge/license-MIT-2F81F7?style=flat-square)](#license)

</div>

```
   power cascade ──┐
                   ├──▶  B loses localization  ──▶  but the right fix differs:
   beacon jam ─────┘        (one symptom)            failover the charger ≠ retune the beacon

        ┌─────────────────────────────┬─────────────────────────────┐
        ▼                             ▼                             ▼
     Reactive                    Memory-only                    Full Aegis
   one fixed move            diagnose, then recall           simulate, then act
   59% safe / 59% ok          100% safe / 49% ok             100% safe / 100% ok
```

> Two independent faults — a power cascade and a jammed beacon — surface as the *same* symptom but need *different* root fixes. Reacting blindly is fast and often dangerous. Remembering only works once you **diagnose** the root cause. Simulating the fix first is safe and effective on both — and in a closed loop it recovers *every* incident.

---

## Run it

```bash
cargo run --release      # 4000 incidents × 3 strategies: the tables + a narrated incident
cargo test --release     # 34 tests (unit + seeded property/oracle sweeps)
```

Every incident is a pure function of one `u64` seed against the world, so the output regenerates exactly from this commit and any failure replays bit-for-bit.

## The numbers

Each incident carries one of **two independent root causes**, 50/50:

- **power cascade** — a shared charger faults → robot **A** drains → A drops the **beacon** → robot **B** (which localizes off A's beacon) drifts. *Root fix: failover the charger.*
- **interference** — A is perfectly healthy, but radio interference jams its beacon channel, so B drifts just the same. *Root fix: retune the beacon to a clear channel.*

Both look identical on the surface ("B is losing localization"), but `failover` does nothing for a jam and `retune` does nothing for a dead battery — so the right move depends on the *diagnosed* root cause. Across 4,000 mixed incidents, one action each:

| Strategy | Safe% | Success% | Danger% | Score | What it does |
|---|--:|--:|--:|--:|---|
| Reactive | 59.3 | 59.3 | 40.7 | 0.37 | one fixed runbook move — fault-blind |
| Memory-only | 100.0 | 49.4 | 0.0 | 1.49 | diagnose the root cause, recall the best historical fix |
| **Full Aegis** | **100.0** | **89.7** | **0.0** | **1.90** | simulate every allowed fix, pick the safest viable |

Reactive can't win because no single fixed move is right for both faults. Memory-only does — *but only because it's keyed on the diagnosed root cause.*

### Diagnosis earns its keep

Key the exact same memory on the **coarse** "beacon is down" symptom instead of the diagnosed root cause, and it collapses to the safe-but-useless default (always halt). Keying on the diagnosis is what lets it recall `failover` for power and `retune` for interference:

| Memory keyed on | Safe% | Success% |
|---|--:|--:|
| coarse "beacon down" | 100.0 | 0.0 |
| **diagnosed root cause** | **100.0** | **49.4** |

That 0 → 49% is the diagnosis layer doing real work — not a label that never changed an outcome.

**The win is not a perfect-oracle artifact.** The twin runs on a deliberately-noisy *belief* of the world, governed by a fidelity knob. As fidelity drops, simulate-before-act degrades gracefully — and once the twin is wrong often enough, it stops beating plain memory. That crossover is the honest research frontier, not a number to bury:

| Twin fidelity | 1.00 | 0.90 | 0.75 | 0.50 | 0.25 |
|---|--:|--:|--:|--:|--:|
| Safe% | 100.0 | 98.5 | 95.6 | 90.6 | 86.2 |
| Score | 1.90 | 1.83 | 1.73 | 1.54 | 1.37 |

**Two ways the twin can be wrong, and they fail differently.** The sweep above is *observation* error — a noisy sensor. The other axis is *model* error: keep the observations perfect, but let the twin's physics drift optimistic (it thinks B drifts slower than it really does). The failure mode inverts — the twin greenlights the aggressive fix (`failover`), so it recovers *more* incidents but walks them through a danger window:

| Twin calibration | 1.00 | 0.80 | 0.60 | 0.40 | 0.20 |
|---|--:|--:|--:|--:|--:|
| Safe% | 100.0 | 91.8 | 76.9 | 74.6 | 74.6 |
| Success% | 89.7 | 93.2 | 99.2 | 100.0 | 100.0 |
| Danger% | 0.0 | 8.2 | 23.1 | 25.4 | 25.4 |

A *noisy* twin loses safety and success together. A *wrong* twin keeps — even raises — success while safety collapses: it's confident and reckless. That's the precise failure that makes simulate-before-act dangerous if you over-trust the model, and the knob says where it starts.

### Closing the loop

One action can't always both make B *safe* and *recover* it — when the spare robot isn't ready and the charger recharges slowly, the only safe single move is to halt B and strand it. The **closed-loop controller** (act → verify → re-decide) sequences moves instead: halt B to make it safe, fail the charger over to recover A, and let B auto-resume once the beacon is back. No single action gets there.

| Full Aegis | Safe% | Success% |
|---|--:|--:|
| Single-step | 100.0 | 89.7 |
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
   seeded scenario ─▶ ground-truth world  (power cascade | beacon jam)
                              │ appends to the event log
                              ▼
   beacon drops ─▶ diagnose ▸ root cause ─▶ observe ▸ noisy belief ─▶ TWIN  (fidelity knob)
                              │                                          │ simulate each fix
                              ▼                                          ▼
        policy gate ◀──── deciders ▸ Reactive · Memory-only · Full Aegis
                              │ safest viable action
                              ▼
        apply ─▶ verify ─▶ (still unsafe? re-decide) ─▶ score ─▶ memory
```

The thing that makes it more than automation is the **memory**: every event, action, and outcome is durable, so diagnosis, simulation, and recovery all read from one history instead of three disconnected tools. The full thesis and the seven core laws live in [docs/scope.md](docs/scope.md).

## What's inside

| | |
|---|---|
| **Ground-truth world** | two faults (power cascade, beacon jam) on one deterministic tick engine ([`sim.rs`](src/sim.rs)) |
| **The twin** | the *same* dynamics on a noisy *belief*, with two knobs — observation *fidelity* and model *calibration* — a separate path, so "simulation helps" can't be a tautology |
| **Diagnosis** | infers the root cause behind a shared symptom (`diagnose`) — the key that makes memory pick the right fix |
| **Operational memory** | per-root-cause action outcomes; the compounding lesson store ([`decision.rs`](src/decision.rs)) |
| **Policy gate** | forbids high-risk actions in context (e.g. restart the beacon anchor while B is moving) |
| **Closed-loop controller** | act → verify → re-decide; sequences actions and auto-resumes a halted robot once it's safe |
| **Three strategies** | Reactive, Memory-only, Full Aegis — behind one `decide()` |

## Why the numbers hold

Nothing here is asserted — it's measured, and the measurement regenerates:

- **Deterministic simulation.** One `u64` seed *is* the incident, so every result replays exactly. The three strategies are scored on *identical* scenarios, for a fair paired comparison.
- **A separated twin.** The decider never sees ground truth, only a fidelity-controlled belief — and the fidelity sweep proves the advantage is real and bounded, not an oracle predicting itself.
- **45 hand-rolled tests.** Dense `#[test]` modules plus seeded oracle/property sweeps over thousands of cases ([`tests/properties.rs`](tests/properties.rs)): determinism & replay, *HaltB is never dangerous*, *retune fixes a jam but not a power loss (and vice-versa)*, *diagnosis lifts memory's success*, *a faithful twin never picks danger*, *a miscalibrated twin does*, *degrading the twin never helps*, *replay agrees with the run*, and strategy ordering. No test-framework dependency; `#![forbid(unsafe_code)]`; clippy-clean.

## Honest scoping

This repo is the **MVP wedge** — a simulated fleet that proves the loop end-to-end. By design it does **not** yet solve:

- real-world twin calibration (here the twin is faithful-enough by construction),
- causal inference from noisy, distributed, partially-observable signals (diagnosis here distinguishes two clean root causes, not a real RCA),
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
