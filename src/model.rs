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
}

impl Action {
    pub fn all() -> [Action; 5] {
        [
            Action::DoNothing,
            Action::FailoverCharger,
            Action::RestartRobotA,
            Action::PromoteCToBeacon,
            Action::HaltB,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Action::DoNothing => "do-nothing",
            Action::FailoverCharger => "failover-charger",
            Action::RestartRobotA => "restart-A",
            Action::PromoteCToBeacon => "promote-C-beacon",
            Action::HaltB => "halt-B",
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
    /// Beacon lost and B is losing localization — the dangerous symptom.
    BeaconLostBDrifting,
}

impl Symptom {
    pub fn label(&self) -> &'static str {
        match self {
            Symptom::Nominal => "nominal",
            Symptom::BatteryDraining => "battery-draining",
            Symptom::BeaconLostBDrifting => "beacon-lost-B-drifting",
        }
    }
}

/// World constants — kept in one place so the twin can deliberately
/// *miscalibrate* them later (a second fidelity knob beyond belief noise).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub horizon: u32,
    pub a_drain_per_tick: f64,  // A battery loss/tick while the charger is faulted
    pub charge_per_tick: f64,   // battery gain/tick when a healthy charger is present
    pub a_offline_battery: f64, // A drops offline below this battery
    pub a_online_battery: f64,  // A only comes back online above this (hysteresis)
    pub localize_gain: f64,     // B localization gain/tick with a beacon
    pub localize_decay: f64,    // B localization loss/tick with no beacon
    pub localize_safe_min: f64, // below this while moving => dangerous
    pub localize_good: f64,     // at/above this => fully recovered
    pub restart_downtime: u32,  // ticks A is offline during a restart
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
}
