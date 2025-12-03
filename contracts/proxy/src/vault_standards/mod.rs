//! # Vault Standards Module
//!
//! Implements the NEP-621 Fungible Token Vault standard for NEAR Protocol.
//!
//! ## Module Organization
//!
//! - [`core`]: Trait definition and default implementations for vault operations
//! - [`events`]: NEP-000 compliant event logging for deposits and withdrawals
//! - [`internal`]: Internal helper functions for share/asset conversions
//! - [`mul_div`]: Safe multiplication and division with configurable rounding

pub mod core;
pub mod events;
pub mod internal;
pub mod mul_div;

pub use core::*;
