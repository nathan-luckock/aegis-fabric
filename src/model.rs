//! Core domain model shared across the runtime: robot identities, the action
//! space, diagnosed symptoms, and the world's physical constants.

use std::fmt;

/// The three robots in the demo fleet.
/// A = beacon anchor, B = depends on A's beacon to localize, C = spare.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RobotId {
    A,
    B,
    C,
}

impl fmt::Display for RobotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RobotId::A => "A",
            RobotId::B => "B",
            RobotId::C => "C",
        };
        write!(f, "{s}")
    }
}

/// The two independent root causes a scenario can carry. They surface with the
/// same symptom (B loses localization) but demand *different* fixes — which is
/// what makes diagnosis matter.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fault {
    /// Charger fault → A drains → A drops offline → its beacon goes dark.
    /// Root fix: recover A's power (failover the charger).
    PowerCascade,
    /// A is healthy, but radio interference jams its beacon channel.
    /// Root fix: retune the beacon to a clear channel.
    Interference,
    /// A is healthy and on a clear channel, but its transmitter has degraded.
    /// Root fix: power-cycle the radio (failover the charger). Looks *identical*
    /// to interference in the obvious signals — only the signal reading differs.
    Brownout,
}

impl Fault {
    pub fn label(&self) -> &'static str {
        match self {
            Fault::PowerCascade => "power-cascade",
            Fault::Interference => "interference",
            Fault::Brownout => "brownout",
        }
    }
}

/// Candidate recovery actions the runtime can choose between at a decision point.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    /// Do nothing this incident.
    DoNothing,
    /// Switch A onto the backup charger — addresses the root cause (A's
    /// battery), but recharge takes time, leaving a window where B keeps
    /// drifting. Safe only if A recharges fast enough.
    FailoverCharger,
    /// Restart robot A. A plausible runbook reflex, but it drops the beacon for
    /// the restart window and never touches the battery — risky while B moves.
    RestartRobotA,
    /// Promote C to beacon anchor so B localizes off C immediately.
    /// Ideal *if* C is actually ready; useless (and dangerous) if it is not.
    PromoteCToBeacon,
    /// Safe-mode: halt B's motion. Always safe, never a full recovery.
    HaltB,
    /// Retune A's beacon to a clear channel — the root fix for interference,
    /// useless against a power cascade (A is offline, not jammed).
    SwitchBeaconChannel,
}

impl Action {
    pub fn all() -> [Action; 6] {
        [
            Action::DoNothing,
            Action::FailoverCharger,
            Action::RestartRobotA,
            Action::PromoteCToBeacon,
            Action::HaltB,
            Action::SwitchBeaconChannel,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Action::DoNothing => "do-nothing",
            Action::FailoverCharger => "failover-charger",
            Action::RestartRobotA => "restart-A",
            Action::PromoteCToBeacon => "promote-C-beacon",
            Action::HaltB => "halt-B",
            Action::SwitchBeaconChannel => "switch-channel",
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A compact description of "what's wrong right now", produced by diagnosis and
/// used as the memory lookup key and the trigger for the deciders.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Symptom {
    Nominal,
    BatteryDraining,
    /// Beacon lost, B drifting — root cause unknown (the *coarse* symptom, used
    /// by the no-diagnosis baseline).
    BeaconLostBDrifting,
    /// Beacon lost because A lost power (A offline/draining).
    BeaconLostPower,
    /// Beacon lost because A's channel is jammed (A still online).
    BeaconLostInterference,
    /// Beacon lost because A's transmitter degraded (A online, channel clear).
    BeaconLostBrownout,
}

impl Symptom {
    pub fn label(&self) -> &'static str {
        match self {
            Symptom::Nominal => "nominal",
            Symptom::BatteryDraining => "battery-draining",
            Symptom::BeaconLostBDrifting => "beacon-lost-B-drifting",
            Symptom::BeaconLostPower => "beacon-lost-power",
            Symptom::BeaconLostInterference => "beacon-lost-interference",
            Symptom::BeaconLostBrownout => "beacon-lost-brownout",
        }
    }
}

/// World constants — kept in one place so the twin can deliberately
/// *miscalibrate* them later (a second fidelity knob beyond belief noise).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub horizon: u32,
    pub a_drain_per_tick: f64, // A battery loss/tick while the charger is faulted
    pub charge_per_tick: f64,  // battery gain/tick when a healthy charger is present
    pub a_offline_battery: f64, // A drops offline below this battery
    pub a_online_battery: f64, // A only comes back online above this (hysteresis)
    pub localize_gain: f64,    // B localization gain/tick with a beacon
    pub localize_decay: f64,   // B localization loss/tick with no beacon
    pub localize_safe_min: f64, // below this while moving => dangerous
    pub localize_good: f64,    // at/above this => fully recovered
    pub restart_downtime: u32, // ticks A is offline during a restart
}

impl Params {
    /// The ground-truth physics of the world.
    pub fn ground_truth() -> Self {
        Params {
            horizon: 60,
            a_drain_per_tick: 2.2,
            charge_per_tick: 4.0,
            a_offline_battery: 20.0,
            a_online_battery: 32.0,
            localize_gain: 0.20,
            localize_decay: 0.14,
            localize_safe_min: 0.40,
            localize_good: 0.85,
            restart_downtime: 6,
        }
    }

    /// The twin's *model* of the world. `calibration` in [0, 1]: 1.0 is a perfect
    /// model (identical to ground truth); lower values drift the **dynamics**
    /// (not the observations), so the twin mispredicts even from perfect inputs.
    ///
    /// The drift is deliberately *optimistic* — the twin underestimates how fast
    /// B drifts and overestimates how fast it recovers — so a poorly-calibrated
    /// twin rates risky actions as safe. This is the dangerous kind of wrong.
    pub fn twin(calibration: f64) -> Self {
        let drift = (1.0 - calibration).clamp(0.0, 1.0);
        let mut p = Params::ground_truth();
        p.localize_decay *= 1.0 - 0.7 * drift; // thinks B drifts slower than it does
        p.localize_gain *= 1.0 + 0.4 * drift; // thinks B re-localizes faster than it does
        p.a_online_battery = (p.a_online_battery - 12.0 * drift).max(p.a_offline_battery + 1.0); // thinks A recovers sooner
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn actions_enumerate_with_unique_labels() {
        let all = Action::all();
        assert_eq!(all.len(), 6);
        let labels: HashSet<_> = all.iter().map(|a| a.label()).collect();
        assert_eq!(labels.len(), 6, "every action label must be distinct");
    }

    #[test]
    fn ground_truth_params_are_well_ordered() {
        let p = Params::ground_truth();
        assert!(p.a_offline_battery < p.a_online_battery, "needs hysteresis");
        assert!(p.localize_safe_min < p.localize_good);
        assert!(p.localize_gain > 0.0 && p.localize_decay > 0.0);
        assert!(p.horizon > 0 && p.a_drain_per_tick > 0.0);
    }

    #[test]
    fn a_perfectly_calibrated_twin_equals_ground_truth() {
        let gt = Params::ground_truth();
        let twin = Params::twin(1.0);
        assert_eq!(twin.localize_decay, gt.localize_decay);
        assert_eq!(twin.localize_gain, gt.localize_gain);
        assert_eq!(twin.a_online_battery, gt.a_online_battery);
    }

    #[test]
    fn a_drifting_twin_is_optimistic() {
        let gt = Params::ground_truth();
        let twin = Params::twin(0.2);
        // It underestimates drift and overestimates recovery — the dangerous way.
        assert!(twin.localize_decay < gt.localize_decay);
        assert!(twin.localize_gain > gt.localize_gain);
        assert!(twin.a_online_battery < gt.a_online_battery);
    }
}
