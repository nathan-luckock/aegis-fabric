//! The simulator: ground-truth world dynamics, scenario generation, the twin's
//! noisy observation, and the single `step()` engine shared by both.
//!
//! The same dynamics drive (a) the real world and (b) the twin's lookahead. The
//! ONLY differences are the inputs: the twin runs from a noisy *belief* of the
//! world (see `observe`) and may use miscalibrated params. That separation is
//! what keeps "simulate-before-act helps" from being a tautology.

use crate::event::{EventKind, EventLog};
use crate::model::{Action, Fault, Params, RobotId, Symptom};
use crate::rng::Rng;

/// Per-scenario initial conditions. The fault type plus these variables decide
/// which recovery action is actually best, so a fixed reactive rule cannot
/// always be right — and the right *root* fix differs by fault, so diagnosis
/// matters.
#[derive(Clone, Copy, Debug)]
pub struct ScenarioCfg {
    /// Which root cause this incident carries.
    pub fault: Fault,
    /// Is spare robot C online and charged enough to serve as a beacon?
    pub c_ready: bool,
    /// How fast A recharges under failover (degraded batteries recharge slowly).
    /// Only relevant to a power cascade.
    pub charge_rate: f64,
    /// A's starting battery — sets how long until a power cascade begins.
    pub a_init: f64,
    /// Tick at which interference jams A's beacon (ignored for a power cascade).
    pub jam_at: u32,
}

impl ScenarioCfg {
    /// A power-cascade scenario (charger fault).
    pub fn power(c_ready: bool, charge_rate: f64, a_init: f64) -> Self {
        ScenarioCfg {
            fault: Fault::PowerCascade,
            c_ready,
            charge_rate,
            a_init,
            jam_at: 0,
        }
    }

    /// An interference scenario (beacon jammed at `jam_at`).
    pub fn interference(c_ready: bool, jam_at: u32) -> Self {
        ScenarioCfg {
            fault: Fault::Interference,
            c_ready,
            charge_rate: 4.0,
            a_init: 50.0,
            jam_at,
        }
    }
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
    pub beacon_jammed: bool,
    pub jam_at: Option<u32>,
    pub c_ready: bool,
    pub b_localization: f64,
    pub b_in_motion: bool,
    pub b_halted: bool,
    pub dangerous_seen: bool,
    pub recovered_tick: Option<u32>,
}

impl SimState {
    pub fn initial(cfg: &ScenarioCfg) -> Self {
        let interference = cfg.fault == Fault::Interference;
        SimState {
            tick: 0,
            decision_tick: 0,
            // Power cascade: the charger has faulted and A is draining.
            // Interference: the charger is healthy; the jam arrives at jam_at.
            charger_faulted: !interference,
            failover_active: false,
            charge_rate: cfg.charge_rate,
            a_battery: cfg.a_init,
            a_online: true,
            a_restart_until: None,
            beacon_source: RobotId::A,
            beacon_up: true,
            beacon_jammed: false,
            jam_at: if interference { Some(cfg.jam_at) } else { None },
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

        // --- Interference onset: the jam arrives once, at jam_at ---
        if let Some(t) = self.jam_at {
            if self.tick == t {
                self.beacon_jammed = true;
            }
        }

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
        // A's beacon needs A online AND a clear channel; C serves on its own
        // channel, so it is immune to A's jamming.
        self.beacon_up = match self.beacon_source {
            RobotId::A => self.a_online && !self.beacon_jammed,
            RobotId::C => self.c_ready,
            RobotId::B => false,
        };

        // --- B localization: recovers with a beacon, drifts without one ---
        if self.beacon_up {
            self.b_localization = (self.b_localization + p.localize_gain).min(1.0);
        } else if !self.b_halted {
            self.b_localization = (self.b_localization - p.localize_decay).max(0.0);
        }
        // A halted B is parked: with no beacon it neither recovers nor drifts.

        // --- Safe-mode auto-resume: a parked robot re-acquires localization from
        // a restored beacon and resumes its task once well localized again. This
        // is what lets a multi-step recovery (halt -> fix -> resume) close. ---
        if self.b_halted && self.beacon_up && self.b_localization >= p.localize_good {
            self.b_halted = false;
        }

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
            Action::SwitchBeaconChannel => {
                self.beacon_jammed = false;
                self.jam_at = None; // retuned to a clear channel; the jam is gone
            }
        }
    }
}

/// Score a single outcome. Danger is penalized hard so that "safe but idle"
/// beats "effective but reckless" — exactly the tradeoff the project is about.
pub fn scenario_score(o: &SimOutcome) -> f64 {
    (if o.safe { 1.0 } else { -2.0 }) + (if o.successful { 1.0 } else { 0.0 })
}

/// Generate a scenario from an RNG stream — a 50/50 mix of the two faults.
pub fn gen_scenario(r: &mut Rng) -> ScenarioCfg {
    let fault = if r.chance(0.5) {
        Fault::PowerCascade
    } else {
        Fault::Interference
    };
    ScenarioCfg {
        fault,
        c_ready: r.chance(0.6),
        charge_rate: r.range_f64(1.5, 5.0),
        a_init: r.range_f64(30.0, 55.0),
        jam_at: r.range_f64(4.0, 10.0) as u32,
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
    // Phase 1: let the cascade run until the beacon drops (the decision trigger),
    // whether from A draining offline or the channel being jammed.
    while s.tick < p.horizon && s.beacon_up {
        s.step(p);
    }
    s.decision_tick = s.tick;
    s.recovered_tick = None; // recovery is measured from the decision point onward
    if s.tick >= p.horizon {
        // No incident within the horizon — nothing to recover from.
        return SimOutcome {
            safe: true,
            successful: true,
            mttr: Some(0),
        };
    }
    let action = choose(&s);
    simulate_from(s, action, p, p.horizon)
}

/// Like `run_scenario` but with a fixed action and a recorded event timeline.
/// Used only by the demo to narrate an incident.
pub fn run_with_log(cfg: &ScenarioCfg, action: Action, p: &Params) -> (SimOutcome, EventLog) {
    let mut s = SimState::initial(cfg);
    let mut log = EventLog::new();
    if s.charger_faulted {
        log.record(0, EventKind::ChargerFaulted);
    }

    let mut prev_online = s.a_online;
    let mut prev_beacon = s.beacon_up;
    let mut prev_jammed = s.beacon_jammed;
    while s.tick < p.horizon && s.beacon_up {
        s.step(p);
        if prev_online && !s.a_online {
            log.record(s.tick, EventKind::RobotOffline(RobotId::A));
        }
        if !prev_jammed && s.beacon_jammed {
            log.record(s.tick, EventKind::BeaconJammed);
        }
        if prev_beacon && !s.beacon_up {
            log.record(s.tick, EventKind::BeaconLost);
        }
        prev_online = s.a_online;
        prev_beacon = s.beacon_up;
        prev_jammed = s.beacon_jammed;
    }

    s.decision_tick = s.tick;
    s.recovered_tick = None;
    if s.tick >= p.horizon {
        return (
            SimOutcome {
                safe: true,
                successful: true,
                mttr: Some(0),
            },
            log,
        );
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

/// Diagnose the current world state into a symptom — the memory key and the
/// controller's trigger. When the beacon is down it infers the *root cause*:
/// a jammed channel (A still online) vs a power loss (A offline).
pub fn diagnose(s: &SimState) -> Symptom {
    if s.beacon_up {
        if s.charger_faulted && !s.failover_active && s.a_battery < 40.0 {
            Symptom::BatteryDraining
        } else {
            Symptom::Nominal
        }
    } else if s.beacon_jammed {
        Symptom::BeaconLostInterference
    } else {
        Symptom::BeaconLostPower
    }
}

/// The coarse symptom: "the beacon is down", root cause unknown. Used by the
/// no-diagnosis baseline to show what diagnosis buys.
pub fn diagnose_coarse(s: &SimState) -> Symptom {
    if s.beacon_up {
        Symptom::Nominal
    } else {
        Symptom::BeaconLostBDrifting
    }
}

/// Whether applying `a` would actually change the world, so the controller does
/// not burn its action budget re-applying a no-op.
pub fn action_changes_state(a: Action, s: &SimState) -> bool {
    match a {
        Action::DoNothing => false,
        Action::FailoverCharger => !s.failover_active,
        Action::RestartRobotA => s.a_restart_until.is_none(),
        Action::PromoteCToBeacon => s.beacon_source != RobotId::C,
        Action::HaltB => !s.b_halted,
        Action::SwitchBeaconChannel => s.beacon_jammed,
    }
}

/// Configuration for the closed-loop controller.
#[derive(Clone, Copy, Debug)]
pub struct ControlConfig {
    /// Ticks to wait between re-decisions.
    pub interval: u32,
    /// Maximum number of distinct state-changing actions per incident.
    pub max_actions: u32,
}

impl ControlConfig {
    pub fn default_loop() -> Self {
        ControlConfig {
            interval: 3,
            max_actions: 4,
        }
    }
}

/// Run a scenario under the closed-loop controller: **act → verify → re-decide**.
///
/// While the beacon is down, the controller consults `decide` every `interval`
/// ticks (up to `max_actions` state-changing actions), then lets the world run.
/// This is what lets a strategy *sequence* actions — halt B to make it safe,
/// fail the charger over to recover A, and let B auto-resume once the beacon is
/// back — a recovery no single action can achieve.
pub fn run_controlled(
    cfg: &ScenarioCfg,
    p: &Params,
    control: &ControlConfig,
    mut decide: impl FnMut(&SimState, Symptom) -> Action,
) -> SimOutcome {
    let mut s = SimState::initial(cfg);
    let mut actions_taken = 0u32;
    let mut last_decision: Option<u32> = None;
    let mut incident_open = false;

    while s.tick < p.horizon {
        let actionable = !s.beacon_up; // the incident is the beacon being down
        if actionable && !incident_open {
            incident_open = true;
            s.decision_tick = s.tick;
            s.recovered_tick = None;
        }
        let due = match last_decision {
            None => true,
            Some(t) => s.tick >= t + control.interval,
        };
        if actionable && actions_taken < control.max_actions && due {
            let sym = diagnose(&s);
            let action = decide(&s, sym);
            if action_changes_state(action, &s) {
                s.apply_action(action, p);
                actions_taken += 1;
            }
            last_decision = Some(s.tick);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Params {
        Params::ground_truth()
    }

    #[test]
    fn initial_state_is_nominal() {
        let cfg = ScenarioCfg::power(true, 3.0, 50.0);
        let s = SimState::initial(&cfg);
        assert!(s.a_online && s.beacon_up);
        assert!(s.b_in_motion && !s.b_halted);
        assert_eq!(s.b_localization, 1.0);
        assert!(!s.dangerous_seen && s.recovered_tick.is_none());
    }

    #[test]
    fn scenarios_are_deterministic() {
        for idx in 0..500usize {
            let mut r1 = Rng::new(seed_for(1, idx));
            let mut r2 = Rng::new(seed_for(1, idx));
            let o1 = run_scenario(&gen_scenario(&mut r1), &p(), |_| Action::FailoverCharger);
            let o2 = run_scenario(&gen_scenario(&mut r2), &p(), |_| Action::FailoverCharger);
            assert_eq!(
                (o1.safe, o1.successful, o1.mttr),
                (o2.safe, o2.successful, o2.mttr)
            );
        }
    }

    #[test]
    fn halt_b_is_always_safe_single_step() {
        for idx in 0..5000usize {
            let mut r = Rng::new(seed_for(7, idx));
            let cfg = gen_scenario(&mut r);
            let o = run_scenario(&cfg, &p(), |_| Action::HaltB);
            assert!(o.safe, "HaltB produced a dangerous state (idx {idx})");
        }
    }

    #[test]
    fn promote_c_is_safe_iff_c_ready() {
        let (mut ready, mut not_ready) = (false, false);
        for idx in 0..3000usize {
            let mut r = Rng::new(seed_for(11, idx));
            let cfg = gen_scenario(&mut r);
            let o = run_scenario(&cfg, &p(), |_| Action::PromoteCToBeacon);
            if cfg.c_ready {
                ready = true;
                assert!(
                    o.safe && o.successful,
                    "PromoteC with C ready should recover"
                );
            } else {
                not_ready = true;
                assert!(!o.safe, "PromoteC with C not ready should be dangerous");
            }
        }
        assert!(ready && not_ready, "both regimes should appear");
    }

    #[test]
    fn observe_at_full_fidelity_is_identity() {
        for idx in 0..2000usize {
            let mut r = Rng::new(seed_for(3, idx));
            let truth = SimState::initial(&gen_scenario(&mut r));
            let mut br = Rng::new(seed_for(99, idx));
            let belief = observe(&truth, 1.0, &mut br);
            assert_eq!(belief.c_ready, truth.c_ready);
            assert!((belief.charge_rate - truth.charge_rate).abs() < 1e-9);
        }
    }

    #[test]
    fn auto_resume_recovers_a_halted_robot_when_the_beacon_returns() {
        let cfg = ScenarioCfg::power(true, 3.0, 50.0);
        let mut s = SimState::initial(&cfg);
        s.b_halted = true;
        s.beacon_up = false;
        s.a_online = false;
        s.b_localization = 0.5;
        s.apply_action(Action::PromoteCToBeacon, &p()); // beacon returns via C
        for _ in 0..40 {
            s.step(&p());
        }
        assert!(!s.b_halted, "B should auto-resume once well localized");
        assert!(s.b_localization >= p().localize_good);
    }

    #[test]
    fn scenario_score_matrix() {
        let mk = |safe, successful| SimOutcome {
            safe,
            successful,
            mttr: None,
        };
        assert_eq!(scenario_score(&mk(true, true)), 2.0);
        assert_eq!(scenario_score(&mk(true, false)), 1.0);
        assert_eq!(scenario_score(&mk(false, true)), -1.0);
        assert_eq!(scenario_score(&mk(false, false)), -2.0);
    }

    #[test]
    fn multi_step_recovers_a_regime_single_step_cannot() {
        // C not ready + slow charge: the best single action is HaltB (safe but
        // never recovers). The closed loop sequences halt -> failover -> resume.
        let cfg = ScenarioCfg::power(false, 2.0, 45.0);
        let single = run_scenario(&cfg, &p(), |_| Action::HaltB);
        assert!(single.safe && !single.successful);

        let ctl = ControlConfig::default_loop();
        let mut step = 0;
        let multi = run_controlled(&cfg, &p(), &ctl, |_s, _sym| {
            step += 1;
            match step {
                1 => Action::HaltB,
                2 => Action::FailoverCharger,
                _ => Action::DoNothing,
            }
        });
        assert!(
            multi.safe && multi.successful,
            "halt->failover->resume should recover safely"
        );
    }

    #[test]
    fn interference_is_fixed_by_switching_channel() {
        let cfg = ScenarioCfg::interference(false, 5);
        let o = run_scenario(&cfg, &p(), |_| Action::SwitchBeaconChannel);
        assert!(o.safe && o.successful, "retuning the channel clears a jam");
    }

    #[test]
    fn interference_is_not_fixed_by_failover() {
        // Failover recovers power, not a jammed channel — A was never offline.
        let cfg = ScenarioCfg::interference(false, 5);
        let o = run_scenario(&cfg, &p(), |_| Action::FailoverCharger);
        assert!(!o.safe, "failover does nothing for interference");
    }

    #[test]
    fn switch_channel_does_not_fix_a_power_cascade() {
        // A is offline; a clear channel doesn't bring a dead beacon back.
        let cfg = ScenarioCfg::power(false, 2.0, 40.0);
        let o = run_scenario(&cfg, &p(), |_| Action::SwitchBeaconChannel);
        assert!(!o.safe, "switching channel does nothing for a power loss");
    }

    #[test]
    fn diagnose_distinguishes_the_root_cause() {
        use crate::model::Symptom;
        let mut power_sym = Symptom::Nominal;
        run_scenario(&ScenarioCfg::power(false, 3.0, 45.0), &p(), |truth| {
            power_sym = diagnose(truth);
            Action::HaltB
        });
        assert_eq!(power_sym, Symptom::BeaconLostPower);

        let mut intf_sym = Symptom::Nominal;
        run_scenario(&ScenarioCfg::interference(false, 5), &p(), |truth| {
            intf_sym = diagnose(truth);
            Action::HaltB
        });
        assert_eq!(intf_sym, Symptom::BeaconLostInterference);
    }
}
