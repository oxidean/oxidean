//! oxidean-runner library — the runner binary is a thin shell over these
//! modules; the API conformance harness drives `exec::execute` directly so
//! tests exercise the real execution engine, not a mirrored interpreter.

pub mod client;
pub mod config;
pub mod exec;
pub mod state;
