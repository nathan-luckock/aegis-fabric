//! The append-only event log — the source of truth (core laws #1 and #5).
//!
//! Every meaningful state transition becomes an immutable, timestamped event.
//! For the MVP the log powers the incident timeline shown in the demo; it is
//! the seed of the causal memory graph described in the project scope.

use crate::model::{Action, RobotId};

#[derive(Clone, Debug)]
pub enum EventKind {
    ChargerFaulted,
    BatteryLow(RobotId, f64),
    RobotOffline(RobotId),
    RobotOnline(RobotId),
    BeaconLost,
    BeaconRestored,
    BeaconJammed,
    ChannelSwitched,
    LocalizationDegraded(f64),
    DangerousState(RobotId),
    ActionTaken(Action),
    FleetRecovered,
}

impl EventKind {
    pub fn describe(&self) -> String {
        match self {
            EventKind::ChargerFaulted => "shared charger faulted".to_string(),
            EventKind::BatteryLow(r, b) => format!("robot {r} battery low ({b:.0}%)"),
            EventKind::RobotOffline(r) => format!("robot {r} dropped offline"),
            EventKind::RobotOnline(r) => format!("robot {r} back online"),
            EventKind::BeaconLost => "beacon network lost".to_string(),
            EventKind::BeaconRestored => "beacon network restored".to_string(),
            EventKind::BeaconJammed => "beacon channel jammed (interference)".to_string(),
            EventKind::ChannelSwitched => "beacon retuned to a clear channel".to_string(),
            EventKind::LocalizationDegraded(q) => format!("B localization degraded ({q:.2})"),
            EventKind::DangerousState(r) => {
                format!("DANGER: robot {r} moving without localization")
            }
            EventKind::ActionTaken(a) => format!("runtime action: {a}"),
            EventKind::FleetRecovered => "fleet recovered".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub tick: u32,
    pub kind: EventKind,
}

#[derive(Clone, Debug, Default)]
pub struct EventLog {
    pub events: Vec<Event>,
}

impl EventLog {
    pub fn new() -> Self {
        EventLog { events: Vec::new() }
    }

    pub fn record(&mut self, tick: u32, kind: EventKind) {
        self.events.push(Event { tick, kind });
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, RobotId};

    #[test]
    fn log_records_in_order() {
        let mut log = EventLog::new();
        assert!(log.is_empty());
        log.record(1, EventKind::ChargerFaulted);
        log.record(5, EventKind::BeaconLost);
        assert_eq!(log.len(), 2);
        assert_eq!(log.events[0].tick, 1);
        assert_eq!(log.events[1].tick, 5);
    }

    #[test]
    fn every_kind_describes_nonempty() {
        let kinds = [
            EventKind::ChargerFaulted,
            EventKind::BatteryLow(RobotId::A, 12.0),
            EventKind::RobotOffline(RobotId::A),
            EventKind::RobotOnline(RobotId::A),
            EventKind::BeaconLost,
            EventKind::BeaconRestored,
            EventKind::BeaconJammed,
            EventKind::ChannelSwitched,
            EventKind::LocalizationDegraded(0.3),
            EventKind::DangerousState(RobotId::B),
            EventKind::ActionTaken(Action::HaltB),
            EventKind::FleetRecovered,
        ];
        for k in kinds {
            assert!(!k.describe().is_empty());
        }
    }
}
