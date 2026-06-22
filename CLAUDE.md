# Aegis Fabric — working agreement & project memory

For anyone (or any agent) picking this up. Read this first, then
[docs/STATUS.md](docs/STATUS.md) for what's done vs. not.

---

## What this is, in one paragraph

An **operational-memory runtime for autonomous fleets**: record every meaningful
event, project it into a world model, diagnose failure, **simulate candidate
repairs against a calibrated twin before acting**, apply the safest
policy-allowed one, and keep the outcome. This repo is the **MVP wedge** — a
simulated fleet that proves the loop end-to-end and settles one falsifiable
question: does simulate-before-act beat a reactive baseline? (It does:
100% safe / 81% success vs 61/61. See [docs/STATUS.md](docs/STATUS.md).)

Full thesis and the seven laws: [docs/scope.md](docs/scope.md).

---

## Conventions — non-negotiable

1. **No AI / Claude attribution. Anywhere.** No `Co-Authored-By` trailers, no
   "Generated with…" notes, in commits, PR descriptions, code comments, or docs.
   Author and committer are Nathan only. This is a hard requirement.
2. **Git remote is HTTPS, not SSH.** The machine's SSH key maps to the
   `NathanLuckock` account, which lacks push access; the repo is owned by
   `nathan-luckock`. The active `gh` token *is* `nathan-luckock` (ADMIN), so push
   over `https://github.com/nathan-luckock/aegis-fabric.git` with the gh
   credential helper (`gh auth setup-git`).
   - **If a push 403s as `NathanLuckock`:** the active gh account flipped. Run
     `gh auth switch -h github.com -u Nathan7108` (that account's token resolves
     to `nathan-luckock`) and retry. Confirm with `gh api user --jq .login`.
   - Git Credential Manager (`manager`) can cache the wrong credential. Bypass it
     with an inline token helper:
     `git -c credential.helper= -c credential.helper='!f(){ echo username=x-access-token; echo password=$(gh auth token); }; f' push`.
3. **Keep [docs/STATUS.md](docs/STATUS.md) current every session.** Update the
   component table and the result block as things land. A component is only ✅
   when its behaviour is actually checked, not merely compiled.
4. **Honest scoping.** Name what is *not* solved. Never present a green check that
   never ran. This ethos is the project's credibility.
5. **Thin vertical slices over broad scaffolding.** Build the smallest thing that
   produces a real result; don't create empty layer folders ahead of need.

---

## The anti-circularity rule — the one that matters most

The **twin must stay a separate, intentionally-imperfect model of the
ground-truth world.** In code: the decider never sees ground truth — it observes
a *noisy belief* (`sim::observe`, governed by a `fidelity` knob) and simulates
from that. If the twin ever becomes the simulator (fidelity-1 oracle used as the
real world), then "simulate-before-act helps" collapses into a tautology and the
whole experiment is worthless. The fidelity sweep exists to prove the advantage
is real and bounded. **Do not collapse the twin into the world.**

---

## Architecture map (`src/`)

| File | Responsibility |
|---|---|
| `rng.rs` | deterministic SplitMix64 PRNG; the basis of replayability |
| `model.rs` | domain types: `RobotId`, `Action`, `Symptom`, world `Params` |
| `event.rs` | append-only `EventLog` — the source-of-truth timeline |
| `sim.rs` | ground-truth world (two faults: power cascade + beacon jam) + shared tick engine, scenario gen, the twin (`observe`), `simulate_from`, single-step `run_scenario`, closed-loop `run_controlled`, root-cause `diagnose` / `diagnose_coarse` |
| `decision.rs` | `IncidentMemory`, `Policy` gate, the three `Arm`s and their `decide()` |
| `replay.rs` | deterministic replay → tick-by-tick forensic `Trace`; `src/bin/replay.rs` renders it as a timeline |
| `experiment.rs` | trains memory, evaluates arms (single + multi-step) on identical seeds, prints tables + fidelity sweep + narrated incident; `pub evaluate`/`train_memory`/`Summary` for tests |
| `main.rs` | arg parsing → `experiment::run` |

Zero external (runtime **and** dev) dependencies — pure `std`, `#![forbid(unsafe_code)]`.
Tests are hand-rolled seeded sweeps, not proptest. Keep it that way unless there's
a strong reason; it makes the build instant and every run deterministic.

---

## How to run

```bash
cargo run --release              # full report: tables + fidelity sweep + demo
cargo run --release -- 20000     # n_eval = 20000 (tighter estimates)
cargo run --release -- 4000 8000 # n_eval, n_train explicit
cargo test --release             # 42 tests (unit + seeded property/oracle sweeps)
cargo run --release --bin replay -- 3 full       # forensic timeline of incident #3 (interference)
cargo run --release --bin replay -- 3 reactive --all  # any seed/strategy, every tick
cargo fmt && cargo clippy --all-targets   # before committing (CI not yet wired)
```

The decision problem: shared charger faults → A drains → A drops the beacon →
B loses localization. Each arm picks one recovery action; `safe` = no
collision-risk state, `success` = B back on task and well-localized.

---

## Current state (keep this short; details in STATUS)

- ✅ MVP loop + closed-loop controller; 3-arm experiment proves the thesis with a
  clean fidelity sweep. Multi-step Full Aegis: 100% safe / 100% success.
- ✅ Two faults (power cascade + beacon jam) with *different* root fixes; `diagnose`
  distinguishes them and lifts memory success 0→49% (the diagnosis ablation).
- ✅ Deterministic replay/forensics (`replay.rs` + `bin/replay.rs`): tick-by-tick timeline.
- ✅ 42 tests (unit + seeded property/oracle sweeps), clippy-clean, no-unsafe.
- 🟡 Diagnosis reads a clean flag; twin imperfection is belief-noise only.
- ⏸ Real twin calibration, noisy causal inference, real hardware — the frontier.

## Next thresholds (see STATUS for the full list)

1. Twin *physics* miscalibration (a second fidelity axis) — **recommended next**.
2. A third fault with overlapping symptoms (diagnosis under ambiguity).
3. CI (fmt + clippy + test workflow).
