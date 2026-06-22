//! The simulator: ground-truth world dynamics, scenario generation, the twin's
//! noisy observation, and the single `step()` engine shared by both.
//!
//! The same dynamics drive (a) the real world and (b) the twin's lookahead. The
//! ONLY differences are the inputs: the twin runs from a noisy *belief* of the
//! world (see `observe`) and may use miscalibrated params. That separation is
//! what keeps "simulate-before-act helps" from being a tautology.

use crate::event::{EventKind, EventLog};
use crate::model::{Action, Params, RobotId};
use crate::rng::Rng;

/// Per-scenario initial conditions. These three variables decide which recovery
/// action is actually best, so a fixed reactive rule cannot always be right.
#[derive(Clone, Copy, Debug)]
pub struct ScenarioCfg {
    /// Is spare robot C online and charged enough to serve as a beacon?
    pub c_ready: bool,
    /// How fast A recharges under failover (degraded batteries recharge slowly).
    pub charge_rate: f64,
    /// A's starting battery — sets how long until the cascade begins.
    pub a_init: f64,
}

/// The outcome the experiment scores.
#[derive(Clone, Copy, Debug)]
pub struct SimOutcome {
    /// No dangerous (collision-risk) state ever occurred.
    pub safe: bool,
    /// B ended the horizon back on task and well-localized.
    pub successful: bool,
    /// Ticks from the decision point to recovery, if it recovered.
    pub mttr: Option<u32>,
}

/// The full world state. For the real world this is ground truth; for the twin
/// it is the runtime's (imperfect) belief.
#[derive(Clone, Debug)]
pub struct SimState {
    pub tick: u32,
    pub decision_tick: u32,
    pub charger_faulted: bool,
    pub failover_active: bool,
    pub charge_rate: f64,
    pub a_battery: f64,
    pub a_online: bool,
    pub a_restart_until: Option<u32>,
    pub beacon_source: RobotId,
    pub beacon_up: bool,
    pub c_ready: bool,
    pub b_localization: f64,
    pub b_in_motion: bool,
    pub b_halted: bool,
    pub dangerous_seen: bool,
    pub recovered_tick: Option<u32>,
}

impl SimState {
    pub fn initial(cfg: &ScenarioCfg) -> Self {
        SimState {
            tick: 0,
            decision_tick: 0,
            charger_faulted: true, // the incident is already underway: charger has faulted
            failover_active: false,
            charge_rate: cfg.charge_rate,
            a_battery: cfg.a_init,
            a_online: true,
            a_restart_until: None,
            beacon_source: RobotId::A,
            beacon_up: true,
            c_ready: cfg.c_ready,
            b_localization: 1.0,
            b_in_motion: true,
            b_halted: false,
            dangerous_seen: false,
            recovered_tick: None,
        }
    }

    /// Advance the world one tick.
    pub fn step(&mut self, p: &Params) {
        self.tick += 1;

        // --- Battery dynamics ---
        if self.failover_active {
            self.a_battery = (self.a_battery + self.charge_rate).min(100.0);
        } else if !self.charger_faulted {
            self.a_battery = (self.a_battery + p.charge_per_tick).min(100.0);
        } else {
            self.a_battery = (self.a_battery - p.a_drain_per_tick).max(0.0);
        }

        // --- Restart timer ---
        if let Some(until) = self.a_restart_until {
            if self.tick >= until {
                self.a_restart_until = None;
            }
        }

        // --- A online/offline (with hysteresis) ---
        if self.a_restart_until.is_some() {
            self.a_online = false;
        } else if self.a_online {
            self.a_online = self.a_battery >= p.a_offline_battery;
        } else {
            self.a_online = self.a_battery >= p.a_online_battery;
        }

        // --- Beacon availability ---
        self.beacon_up = match self.beacon_source {
            RobotId::A => self.a_online,
            RobotId::C => self.c_ready,
            RobotId::B => false,
        };

        // --- B localization: recovers with a beacon, drifts without one ---
        if self.beacon_up {
            self.b_localization = (self.b_localization + p.localize_gain).min(1.0);
        } else if !self.b_halted {
            self.b_localization = (self.b_localization - p.localize_decay).max(0.0);
        }
        // A halted B is parked: it neither recovers nor drifts into danger.

        // --- Danger: moving without enough localization is a collision risk ---
        if self.b_in_motion && !self.b_halted && self.b_localization < p.localize_safe_min {
            self.dangerous_seen = true;
        }

        // --- Recovery: B back on task and well localized ---
        if self.recovered_tick.is_none()
            && self.b_in_motion
            && !self.b_halted
            && self.beacon_up
            && self.b_localization >= p.localize_good
        {
            self.recovered_tick = Some(self.tick);
        }
    }

    /// Apply a recovery action at the decision point.
    pub fn apply_action(&mut self, a: Action, p: &Params) {
        match a {
            Action::DoNothing => {}
            Action::FailoverCharger => self.failover_active = true,
            Action::RestartRobotA => {
                self.a_restart_until = Some(self.tick + p.restart_downtime);
                self.a_online = false;
            }
            Action::PromoteCToBeacon => self.beacon_source = RobotId::C,
            Action::HaltB => self.b_halted = true,
        }
    }
}

/// Score a single outcome. Danger is penalized hard so that "safe but idle"
/// beats "effective but reckless" — exactly the tradeoff the project is about.
pub fn scenario_score(o: &SimOutcome) -> f64 {
    (if o.safe { 1.0 } else { -2.0 }) + (if o.successful { 1.0 } else { 0.0 })
}

/// Generate a scenario from an RNG stream.
pub fn gen_scenario(r: &mut Rng) -> ScenarioCfg {
    ScenarioCfg {
        c_ready: r.chance(0.6),
        charge_rate: r.range_f64(1.5, 5.0),
        a_init: r.range_f64(30.0, 55.0),
    }
}

/// The twin's view of the world: ground truth seen through a noisy sensor.
/// `fidelity == 1.0` returns the truth exactly (a perfect oracle); lower
/// fidelity flips C's readiness and blurs the recharge estimate. This is the
/// knob the fidelity sweep turns to prove the result isn't an oracle artifact.
pub fn observe(truth: &SimState, fidelity: f64, r: &mut Rng) -> SimState {
    let mut belief = truth.clone();
    if !r.chance(fidelity) {
        belief.c_ready = !truth.c_ready;
    }
    let noise = (1.0 - fidelity) * r.range_f64(-2.5, 2.5);
    belief.charge_rate = (truth.charge_rate + noise).clamp(0.5, 6.0);
    belief
}

/// Simulate forward from a state under one action to the horizon. Used by both
/// the ground-truth post-decision rollout and the twin's per-action lookahead.
pub fn simulate_from(mut s: SimState, action: Action, p: &Params, horizon: u32) -> SimOutcome {
    s.apply_action(action, p);
    while s.tick < horizon {
        s.step(p);
    }
    let successful =
        s.b_in_motion && !s.b_halted && s.beacon_up && s.b_localization >= p.localize_good;
    SimOutcome {
        safe: !s.dangerous_seen,
        successful,
        mttr: s.recovered_tick.map(|t| t.saturating_sub(s.decision_tick)),
    }
}

/// Run a full ground-truth scenario. `choose` is invoked once, at the decision
/// point (the moment A drops the beacon), with the true world state; it returns
/// the recovery action to apply.
pub fn run_scenario(
    cfg: &ScenarioCfg,
    p: &Params,
    mut choose: impl FnMut(&SimState) -> Action,
) -> SimOutcome {
    let mut s = SimState::initial(cfg);
    // Phase 1: let the cascade run until A drops offline (the decision trigger).
    while s.tick < p.horizon && s.a_online {
        s.step(p);
    }
    s.decision_tick = s.tick;
    s.recovered_tick = None; // recovery is measured from the decision point onward
    if s.tick >= p.horizon {
        // No incident within the horizon — nothing to recover from.
        return SimOutcome { safe: true, successful: true, mttr: Some(0) };
    }
    let action = choose(&s);
    simulate_from(s, action, p, p.horizon)
}

/// Like `run_scenario` but with a fixed action and a recorded event timeline.
/// Used only by the demo to narrate an incident.
pub fn run_with_log(cfg: &ScenarioCfg, action: Action, p: &Params) -> (SimOutcome, EventLog) {
    let mut s = SimState::initial(cfg);
    let mut log = EventLog::new();
    log.record(0, EventKind::ChargerFaulted);

    let mut prev_online = s.a_online;
    let mut prev_beacon = s.beacon_up;
    while s.tick < p.horizon && s.a_online {
        s.step(p);
        if prev_online && !s.a_online {
            log.record(s.tick, EventKind::RobotOffline(RobotId::A));
        }
        if prev_beacon && !s.beacon_up {
            log.record(s.tick, EventKind::BeaconLost);
        }
        prev_online = s.a_online;
        prev_beacon = s.beacon_up;
    }

    s.decision_tick = s.tick;
    s.recovered_tick = None;
    if s.tick >= p.horizon {
        return (SimOutcome { safe: true, successful: true, mttr: Some(0) }, log);
    }

    s.apply_action(action, p);
    log.record(s.tick, EventKind::ActionTaken(action));

    let mut prev_danger = s.dangerous_seen;
    let mut prev_rec = s.recovered_tick.is_some();
    let mut prev_beacon2 = s.beacon_up;
    while s.tick < p.horizon {
        s.step(p);
        if !prev_beacon2 && s.beacon_up {
            log.record(s.tick, EventKind::BeaconRestored);
        }
        if !prev_danger && s.dangerous_seen {
            log.record(s.tick, EventKind::DangerousState(RobotId::B));
        }
        if !prev_rec && s.recovered_tick.is_some() {
            log.record(s.tick, EventKind::FleetRecovered);
        }
        prev_danger = s.dangerous_seen;
        prev_rec = s.recovered_tick.is_some();
        prev_beacon2 = s.beacon_up;
    }

    let successful =
        s.b_in_motion && !s.b_halted && s.beacon_up && s.b_localization >= p.localize_good;
    (
        SimOutcome {
            safe: !s.dangerous_seen,
            successful,
            mttr: s.recovered_tick.map(|t| t.saturating_sub(s.decision_tick)),
        },
        log,
    )
}

/// Derive an independent, reproducible RNG seed per scenario index.
pub fn seed_for(base: u64, idx: usize) -> u64 {
    base ^ (idx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0xD1B5_4A32_D192_ED03)
}
