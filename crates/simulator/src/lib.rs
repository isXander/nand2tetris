//! Hardware simulator for Nand to Tetris chips.
//!
//! - [`circuit::Circuit`] instantiates a validated
//!   [`n2t_core::hdl::ChipDefinition`] with fresh state and evaluates it with
//!   the two-phase `tick`/`tock` clock.
//! - [`primitives`] holds native implementations of the built-in chips.
//! - [`runner::ScriptRunner`] executes `.tst` scripts, writing `.out` files
//!   and comparing against `.cmp` files ([`format`]).

pub mod circuit;
pub mod format;
pub mod primitives;
pub mod runner;

pub use circuit::{Circuit, SimError, Time};
pub use primitives::{BuiltinRegistry, Primitive};
pub use runner::{
    Event, Outcome, RunError, RunnerOptions, ScriptRunner, StartError, SuspendReason,
};
