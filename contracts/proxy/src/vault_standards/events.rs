//! # Vault Events
//!
//! NEP-000 compliant event logging for vault operations.
//! Events are emitted as JSON logs prefixed with `EVENT_JSON:`.
//!
//! ## Event Types
//!
//! - `VaultDeposit`: Emitted when assets are deposited into the vault
//! - `VaultWithdraw`: Emitted when assets are withdrawn from the vault
//!
//! ## Format
//!
//! Events follow the NEP-000 standard:
//! ```json
//! {
//!   "standard": "nep000",
//!   "version": "1.0.0",
//!   "event": "vault_deposit",
//!   "data": [{ ... }]
//! }
//! ```

use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{env, AccountIdRef};

// ============================================================================
// Event Wrapper
// ============================================================================

/// Top-level event wrapper for NEP-000 compliance.
#[derive(Serialize, Debug)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "standard")]
#[must_use = "don't forget to `.emit()` this event"]
#[serde(rename_all = "snake_case")]
#[allow(unused)]
pub(crate) enum NearEvent<'a> {
    /// NEP-000 standard event container.
    Nep000(Nep000Event<'a>),
}

#[allow(unused)]
impl<'a> NearEvent<'a> {
    /// Serializes the event to JSON.
    fn to_json_string(&self) -> String {
        #[allow(clippy::redundant_closure)]
        serde_json::to_string(self)
            .ok()
            .unwrap_or_else(|| env::abort())
    }

    /// Formats the event with the required EVENT_JSON prefix.
    fn to_json_event_string(&self) -> String {
        format!("EVENT_JSON:{}", self.to_json_string())
    }

    /// Logs the event to the NEAR runtime.
    ///
    /// This must be called to actually emit the event.
    pub(crate) fn emit(self) {
        near_sdk::env::log_str(&self.to_json_event_string());
    }
}

// ============================================================================
// Vault Deposit Event
// ============================================================================

/// Event data for vault deposits.
///
/// Emitted when assets are deposited into the vault and shares are minted.
#[must_use]
#[derive(Serialize, Debug, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct VaultDeposit<'a> {
    /// The account that sent the assets.
    pub sender_id: &'a AccountIdRef,
    /// The account that received the shares.
    pub owner_id: &'a AccountIdRef,
    /// The amount of assets deposited.
    pub assets: U128,
    /// The amount of shares minted.
    pub shares: U128,
    /// Optional memo for the deposit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<&'a str>,
}

#[allow(unused)]
impl VaultDeposit<'_> {
    /// Emits a single deposit event.
    pub fn emit(self) {
        Self::emit_many(&[self])
    }

    /// Emits multiple deposit events in a single log.
    pub fn emit_many(data: &[VaultDeposit<'_>]) {
        new_000_v1(Nep000EventKind::VaultDeposit(data)).emit()
    }
}

// ============================================================================
// Vault Withdraw Event
// ============================================================================

/// Event data for vault withdrawals.
///
/// Emitted when shares are burned and assets are transferred out.
#[must_use]
#[derive(Serialize, Debug, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct VaultWithdraw<'a> {
    /// The account that owned the shares.
    pub owner_id: &'a AccountIdRef,
    /// The account that received the assets.
    pub receiver_id: &'a AccountIdRef,
    /// The amount of shares burned.
    pub shares: U128,
    /// The amount of assets transferred.
    pub assets: U128,
    /// Optional memo for the withdrawal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<&'a str>,
}

#[allow(unused)]
impl VaultWithdraw<'_> {
    /// Emits a single withdraw event.
    pub fn emit(self) {
        Self::emit_many(&[self])
    }

    /// Emits multiple withdraw events in a single log.
    pub fn emit_many(data: &[VaultWithdraw<'_>]) {
        new_000_v1(Nep000EventKind::VaultWithdraw(data)).emit()
    }
}

// ============================================================================
// Internal Event Structures
// ============================================================================

/// NEP-000 event payload structure.
#[derive(Serialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub(crate) struct Nep000Event<'a> {
    /// Event format version.
    version: &'static str,
    /// The actual event data.
    #[serde(flatten)]
    event_kind: Nep000EventKind<'a>,
}

/// Enum of supported vault event types.
#[derive(Serialize, Debug)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
enum Nep000EventKind<'a> {
    /// One or more deposit events.
    VaultDeposit(&'a [VaultDeposit<'a>]),
    /// One or more withdraw events.
    VaultWithdraw(&'a [VaultWithdraw<'a>]),
}

/// Creates a NEP-000 event with the specified version.
fn new_000<'a>(version: &'static str, event_kind: Nep000EventKind<'a>) -> NearEvent<'a> {
    NearEvent::Nep000(Nep000Event {
        version,
        event_kind,
    })
}

/// Creates a NEP-000 v1.0.0 event.
fn new_000_v1(event_kind: Nep000EventKind) -> NearEvent {
    new_000("1.0.0", event_kind)
}
