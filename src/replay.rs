//! Deterministic replay & forensics (scope Layer 9).
//!
//! An incident is a pure function of its seed, so any one can be reconstructed
//! exactly and rendered as a tick-by-tick timeline: A's battery, the beacon, B's
//! localization, the controller's actions, and the events as they fire. This is
//! the forensic lens — "what happened, when, and what did the runtime do about
//! it" — built on the same `step` engine the experiment uses, so a replay can
//! never disagree with the run it reconstructs.

use crate::model::{Action, Params};
use crate::sim::{action_changes_state, diagnose, ControlConfig, ScenarioCfg, SimOutcome, SimState};

/// One tick of an incident: the world state after the tick, plus anything that
/// changed (events) and any action the controller applied entering it.
#[derive(Clone, Debug)]
pub struct Frame {
    pub tick: u32,
    pub a_battery: f64,
    pub a_online: bool,
    pub beacon_up: bool,
    pub b_localization: f64,
    pub b_halted: bool,
    pub dangerous_now: bool,
    pub recovered: bool,
    pub action: Option<Action>,
    pub events: Vec<String>,
}

impl Frame {
    /// A "keyframe" is any tick where something worth seeing happened — an
    /// action or a state transition. Ongoing states (a persistent danger window)
    /// are captured by their onset event, so they collapse to a single line.
    pub fn is_keyframe(&self) -> bool {
        self.action.is_some() || !self.events.is_empty()
    }
}

/// A full reconstructed incident.
#[derive(Clone, Debug)]
pub struct Trace {
    pub cfg: ScenarioCfg,
    pub frames: Vec<Frame>,
    pub outcome: SimOutcome,
    pub decision_tick: Option<u32>,
}

/// Reconstruct an incident under the closed-loop controller, recording a frame
/// per tick. Mirrors `sim::run_controlled` exactly (it calls the same `step`),
/// so `trace(..).outcome == run_controlled(..)` for the same decider.
pub fn trace(
    cfg: &ScenarioCfg,
    p: &Params,
    control: &ControlConfig,
    mut decide: impl FnMut(&SimState, crate::model::Symptom) -> Action,
) -> Trace {
    let mut s = SimState::initial(cfg);
    let mut frames = Vec::with_capacity(p.horizon as usize);
    let mut actions_taken = 0u32;
    let mut last_decision: Option<u32> = None;
    let mut incident_open = false;
    let mut decision_tick = None;

    let (mut prev_online, mut prev_beacon) = (s.a_online, s.beacon_up);
    let (mut prev_danger, mut prev_halted) = (s.dangerous_seen, s.b_halted);
    let mut prev_recovered = s.recovered_tick.is_some();

    while s.tick < p.horizon {
        let actionable = !s.beacon_up;
        if actionable && !incident_open {
            incident_open = true;
            s.decision_tick = s.tick;
            s.recovered_tick = None;
            prev_recovered = false;
            decision_tick = Some(s.tick);
        }
        let due = match last_decision {
            None => true,
            Some(t) => s.tick >= t + control.interval,
        };
        let mut applied = None;
        if actionable && actions_taken < control.max_actions && due {
            let sym = diagnose(&s);
            let action = decide(&s, sym);
            if action_changes_state(action, &s) {
                s.apply_action(action, p);
                actions_taken += 1;
                applied = Some(action);
            }
            last_decision = Some(s.tick);
        }

        s.step(p);

        let mut events = Vec::new();
        if prev_online && !s.a_online {
            events.push("robot A dropped offline".to_string());
        }
        if !prev_online && s.a_online {
            events.push("robot A back online".to_string());
        }
        if prev_beacon && !s.beacon_up {
            events.push("beacon lost".to_string());
        }
        if !prev_beacon && s.beacon_up {
            events.push("beacon restored".to_string());
        }
        if !prev_halted && s.b_halted {
            events.push("B halted (safe-mode)".to_string());
        }
        if prev_halted && !s.b_halted {
            events.push("B resumed".to_string());
        }
        if !prev_danger && s.dangerous_seen {
            events.push("DANGER: B moving without localization".to_string());
        }
        let recovered_now = s.recovered_tick.is_some();
        if !prev_recovered && recovered_now {
            events.push("fleet recovered".to_string());
        }

        frames.push(Frame {
            tick: s.tick,
            a_battery: s.a_battery,
            a_online: s.a_online,
            beacon_up: s.beacon_up,
            b_localization: s.b_localization,
            b_halted: s.b_halted,
            dangerous_now: s.b_in_motion && !s.b_halted && s.b_localization < p.localize_safe_min,
            recovered: recovered_now,
            action: applied,
            events,
        });

        prev_online = s.a_online;
        prev_beacon = s.beacon_up;
        prev_danger = s.dangerous_seen;
        prev_halted = s.b_halted;
        prev_recovered = recovered_now;
    }

    let successful =
        s.b_in_motion && !s.b_halted && s.beacon_up && s.b_localization >= p.localize_good;
    Trace {
        cfg: *cfg,
        frames,
        outcome: SimOutcome {
            safe: !s.dangerous_seen,
            successful,
            mttr: s.recovered_tick.map(|t| t.saturating_sub(s.decision_tick)),
        },
        decision_tick,
    }
}

fn bar(frac: f64, width: usize) -> String {
    let filled = (frac.clamp(0.0, 1.0) * width as f64).round() as usize;
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        s.push(if i < filled { '#' } else { '.' });
    }
    s.push(']');
    s
}

fn b_state(f: &Frame) -> &'static str {
    if f.dangerous_now {
        "DANGER"
    } else if f.b_halted {
        "halted"
    } else if f.recovered {
        "ok"
    } else if !f.beacon_up {
        "drift"
    } else {
        "moving"
    }
}

/// Render a trace as a tick-by-tick timeline. `keyframes_only` collapses the
/// quiet ticks (first and last are always kept) so the forensic view is the
/// story, not the noise.
pub fn render(trace: &Trace, keyframes_only: bool) {
    println!(
        "scenario: C_ready={}  A_recharge={:.2}/tick  A_start={:.0}%",
        trace.cfg.c_ready, trace.cfg.charge_rate, trace.cfg.a_init
    );
    if let Some(t) = trace.decision_tick {
        println!("beacon first lost at t={t}");
    }
    println!(
        "{:>4}  {:<11} {:<6} {:<12} {:<7} events / action",
        "t", "A battery", "beacon", "B localize", "B"
    );
    println!("{}", "-".repeat(82));

    let last = trace.frames.len().saturating_sub(1);
    for (i, f) in trace.frames.iter().enumerate() {
        if keyframes_only && !f.is_keyframe() && i != 0 && i != last {
            continue;
        }
        let mut note = String::new();
        if let Some(a) = f.action {
            note.push_str(&format!("\u{2192} {}", a.label()));
            if !f.events.is_empty() {
                note.push_str("  |  ");
            }
        }
        note.push_str(&f.events.join("; "));
        println!(
            "{:>4}  {} {:>3.0}%  {:<6} {} {:>4.2}  {:<7} {}",
            f.tick,
            bar(f.a_battery / 100.0, 6),
            f.a_battery,
            if f.beacon_up { "up" } else { "down" },
            bar(f.b_localization, 6),
            f.b_localization,
            b_state(f),
            note
        );
    }
    println!("{}", "-".repeat(82));
    let mttr = trace
        .outcome
        .mttr
        .map(|m| format!("{m} ticks"))
        .unwrap_or_else(|| "-".to_string());
    println!(
        "verdict: safe={}  recovered={}  time-to-recover={}",
        trace.outcome.safe, trace.outcome.successful, mttr
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;
    use crate::sim::{gen_scenario, run_controlled, seed_for};

    #[test]
    fn trace_records_one_frame_per_tick() {
        let p = Params::ground_truth();
        let ctl = ControlConfig::default_loop();
        let cfg = gen_scenario(&mut Rng::new(1));
        let t = trace(&cfg, &p, &ctl, |_s, _sym| Action::DoNothing);
        assert_eq!(t.frames.len(), p.horizon as usize);
    }

    #[test]
    fn trace_outcome_agrees_with_run_controlled() {
        let p = Params::ground_truth();
        let ctl = ControlConfig::default_loop();
        for idx in 0..2000usize {
            let cfg = gen_scenario(&mut Rng::new(seed_for(77, idx)));
            let t = trace(&cfg, &p, &ctl, |_s, _sym| Action::HaltB);
            let o = run_controlled(&cfg, &p, &ctl, |_s, _sym| Action::HaltB);
            assert_eq!(
                (t.outcome.safe, t.outcome.successful, t.outcome.mttr),
                (o.safe, o.successful, o.mttr),
                "replay must agree with the run it reconstructs (idx {idx})"
            );
        }
    }

    #[test]
    fn keyframes_are_a_subset_with_the_signal() {
        let p = Params::ground_truth();
        let ctl = ControlConfig::default_loop();
        // a do-nothing incident must contain the danger keyframe
        let cfg = gen_scenario(&mut Rng::new(seed_for(5, 3)));
        let t = trace(&cfg, &p, &ctl, |_s, _sym| Action::DoNothing);
        let keys = t.frames.iter().filter(|f| f.is_keyframe()).count();
        assert!(keys >= 1 && keys <= t.frames.len());
        assert!(t.frames.iter().any(|f| f.events.iter().any(|e| e.contains("beacon lost"))));
    }
}
