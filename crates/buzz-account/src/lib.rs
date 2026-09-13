//! Account service for the hosted Buzz fork.
//!
//! Signs people in with an email code, holds their identity keys on their
//! behalf (see `docs/adr/0001-custodial-identity-keys.md`), and provisions
//! communities on the relay with the operator key. This crate is fork-only;
//! the relay is never modified for it.

pub mod config;
pub mod db;
pub mod http;

pub use config::Config;
