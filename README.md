<div align="center">

# Aegis Fabric

**operational memory for autonomous fleets**

*A fleet hits a failure cascade at 3am with no one watching. Aegis Fabric remembers it, simulates every candidate fix against a calibrated twin before touching anything real, applies the safest one — and keeps the lesson for next time.*

[![Status](https://img.shields.io/badge/status-MVP%20%C2%B7%20pre--alpha-orange?style=flat-square)](docs/STATUS.md)
[![Safe recovery](https://img.shields.io/badge/safe%20recovery-100%25-3FB950?style=flat-square)](#the-numbers)
[![Closed loop](https://img.shields.io/badge/closed%20loop-100%25%20recovered-3FB950?style=flat-square)](#closing-the-loop)
[![Dependencies](https://img.shields.io/badge/dependencies-0-8957e5?style=flat-square&logo=rust&logoColor=white)](#)
[![Tests](https://img.shields.io/badge/tests-37%20passing-3FB950?style=flat-square&logo=rust&logoColor=white)](#why-the-numbers-hold)
[![Unsafe](https://img.shields.io/badge/unsafe-forbidden-CE422B?style=flat-square&logo=rust&logoColor=white)](#)
[![License](https://img.shields.io/badge/license-MIT-2F81F7?style=flat-square)](#license)

</div>

```
         shared charger faults
                  │
                  ▼
   robot A drains ──▶ A drops the beacon ──▶ robot B loses localization ──▶ fleet degrades
                                                       │
                                          the runtime must pick a fix
                                                       │
        ┌──────────────────────────────────┬──────────────────────────────────┐
        ▼                                   ▼                                   ▼
     Reactive                          Memory-only                         Full Aegis
   a fixed runbook                    the safe default                  simulate, then act
   61% safe / 61% ok                 100% safe / 0% ok                 100% safe / 100% ok
```

> Three strategies face the same 4,000 seeded incidents. Reacting blindly is fast and often dangerous. Remembering the safe default is always safe but strands the robot. Only **simulating the fix first** is both safe and effective — and once it can sequence actions in a closed loop, it recovers *every* incident.

---

## Run it

```bash
cargo run --release      # 4000 incidents × 3 strategies: the tables + a narrated incident
cargo test --release     # 34 tests (unit + seeded property/oracle sweeps)
```

Every incident is a pure function of one `u64` seed against the world, so the output regenerates exactly from this commit and any failure replays bit-for-bit.

## The numbers

The incident is a *causal cascade*, not three unrelated faults: a shared charger faults → robot **A** drains → A drops the **beacon** → robot **B** (which localizes off A's beacon) drifts → the fleet degrades. When B starts drifting, each strategy picks **one** recovery action:

| Strategy | Safe% | Success% | Danger% | Score | What it does |
|---|--:|--:|--:|--:|---|
| Reactive | 60.8 | 60.8 | 39.2 | 0.43 | fixed runbook rule — no memory, no simulation |
| Memory-only | 100.0 | 0.0 | 0.0 | 1.00 | best *historical* action — learns, can't adapt |
| **Full Aegis** | **100.0** | **81.3** | **0.0** | **1.81** | simulate every allowed fix, pick the safest viable |

Two deltas tell the whole story. Reactive → Memory-only is what *remembering* buys: safety. Memory-only → Full Aegis is what *simulating* buys on top: it keeps the safety and adds effectiveness, because it tests the context-specific fix before committing.

**The win is not a perfect-oracle artifact.** The twin runs on a deliberately-noisy *belief* of the world, governed by a fidelity knob. As fidelity drops, simulate-before-act degrades gracefully — and below ~0.5 it stops being worth it. That threshold is the honest research frontier, not a number to bury:

| Twin fidelity | 1.00 | 0.90 | 0.75 | 0.50 | 0.25 |
|---|--:|--:|--:|--:|--:|
| Safe% | 100.0 | 95.1 | 88.8 | 76.5 | 63.0 |
| Score | 1.81 | 1.61 | 1.35 | 0.85 | 0.31 |

### Closing the loop

One action can't always both make B *safe* and *recover* it — when the spare robot isn't ready and the charger recharges slowly, the only safe single move is to halt B and strand it. The **closed-loop controller** (act → verify → re-decide) sequences moves instead: halt B to make it safe, fail the charger over to recover A, and let B auto-resume once the beacon is back. No single action gets there.

| Full Aegis | Safe% | Success% |
|---|--:|--:|
| Single-step | 100.0 | 81.3 |
| **Closed loop** | **100.0** | **100.0** |

Same safety, every stranded incident recovered. Reactive and Memory-only don't move — neither can sequence. The narrated incident from `cargo run`:

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
   t  A battery   beacon B localize   B       events / action
  13  [#.....]  19%  down   [#####.] 0.86  ok      robot A dropped offline; beacon lost
  14  [#.....]  23%  down   [####..] 0.72  drift   → failover-charger
  17  [##....]  33%  up     [####..] 0.64  halted  → halt-B  |  beacon restored; B halted
  19  [##....]  40%  up     [######] 1.00  ok      B resumed; fleet recovered
verdict: safe=true  recovered=true  time-to-recover=6 ticks
```

It runs on the same `step` engine as the experiment, so a replay can never disagree with the run it reconstructs — and a test asserts exactly that.

## How it works

```text
   seeded scenario ─▶ ground-truth world  (charger ▸ battery ▸ beacon ▸ localization)
                              │ appends to the event log
                              ▼
   beacon drops ─▶ observe ▸ noisy belief ─▶ TWIN  (same dynamics, fidelity knob)
                              │                    │ simulate every policy-allowed fix
                              ▼                    ▼
        policy gate ◀──── deciders ▸ Reactive · Memory-only · Full Aegis
                              │ safest viable action
                              ▼
        apply ─▶ verify ─▶ (still unsafe? re-decide) ─▶ score ─▶ memory
```

The thing that makes it more than automation is the **memory**: every event, action, and outcome is durable, so diagnosis, simulation, and recovery all read from one history instead of three disconnected tools. The full thesis and the seven core laws live in [docs/scope.md](docs/scope.md).

## What's inside

| | |
|---|---|
| **Ground-truth world** | the charger→battery→beacon→localization cascade on a deterministic tick engine ([`sim.rs`](src/sim.rs)) |
| **The twin** | the *same* dynamics on a noisy *belief* of the world with a fidelity knob — a separate input path, so "simulation helps" can't be a tautology |
| **Operational memory** | per-symptom action outcomes; the compounding lesson store ([`decision.rs`](src/decision.rs)) |
| **Policy gate** | forbids high-risk actions in context (e.g. restart the beacon anchor while B is moving) |
| **Closed-loop controller** | act → verify → re-decide; sequences actions and auto-resumes a halted robot once it's safe |
| **Three strategies** | Reactive, Memory-only, Full Aegis — behind one `decide()` |

## Why the numbers hold

Nothing here is asserted — it's measured, and the measurement regenerates:

- **Deterministic simulation.** One `u64` seed *is* the incident, so every result replays exactly. The three strategies are scored on *identical* scenarios, for a fair paired comparison.
- **A separated twin.** The decider never sees ground truth, only a fidelity-controlled belief — and the fidelity sweep proves the advantage is real and bounded, not an oracle predicting itself.
- **37 hand-rolled tests.** Dense `#[test]` modules plus seeded oracle/property sweeps over thousands of cases ([`tests/properties.rs`](tests/properties.rs)): determinism & replay, *HaltB is never dangerous*, *do-nothing always ends in danger*, *a faithful twin never picks danger*, *degrading the twin never helps*, *replay agrees with the run*, and strategy ordering. No test-framework dependency; `#![forbid(unsafe_code)]`; clippy-clean.

## Honest scoping

This repo is the **MVP wedge** — a simulated fleet that proves the loop end-to-end. By design it does **not** yet solve:

- real-world twin calibration (here the twin is faithful-enough by construction),
- causal inference from noisy, distributed, partially-observable signals,
- a real diagnosis module (the symptom is still coarse),
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
