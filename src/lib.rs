//! Aegis Fabric — operational memory runtime for autonomous fleets.
//!
//! This crate is the MVP wedge from the project scope: a simulated fleet that
//! can remember failures, simulate interventions against a calibrated twin, and
//! recover safely — wrapped in a falsifiable experiment that asks one question:
//!
//!   Does simulate-before-act beat a reactive baseline?
//!
//! Three arms answer it: Reactive, Memory-only, and Full Aegis. The twin is a
//! deliberately-imperfect model of the ground-truth world (see `sim::observe`),
//! so a Full-Aegis win is a real result, not a perfect-oracle artifact.
#![allow(dead_code)]

pub mod rng;
pub mod model;
pub mod event;
pub mod sim;
pub mod decision;
pub mod replay;
pub mod experiment;
