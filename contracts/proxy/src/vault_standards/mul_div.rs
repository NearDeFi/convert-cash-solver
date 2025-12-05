//! # Safe Multiplication and Division
//!
//! Provides overflow-safe multiplication and division operations using
//! 256-bit intermediate arithmetic. This is essential for share/asset
//! conversions where naive multiplication could overflow.
//!
//! ## Rounding Modes
//!
//! - `Down`: Round towards zero (floor)
//! - `Up`: Round away from zero (ceiling)
//!
//! The rounding mode affects financial calculations:
//! - Use `Down` when calculating shares to mint (favor vault)
//! - Use `Up` when calculating shares to burn (favor vault)

/// Rounding direction for division operations.
#[derive(Clone, Copy, Debug)]
pub enum Rounding {
    /// Round towards zero (floor division).
    Down,
    /// Round away from zero (ceiling division).
    Up,
}

/// Performs `(x * y) / denominator` with configurable rounding.
///
/// Uses 256-bit intermediate arithmetic to prevent overflow during
/// the multiplication step.
///
/// # Arguments
///
/// * `x` - First multiplicand
/// * `y` - Second multiplicand
/// * `denominator` - The divisor
/// * `rounding` - Whether to round up or down
///
/// # Returns
///
/// The result of (x * y) / denominator with the specified rounding.
///
/// # Example
///
/// ```ignore
/// // Calculate shares = (assets * supply) / total_assets, rounded down
/// let shares = mul_div(100_000, 1_000_000, 500_000, Rounding::Down);
/// assert_eq!(shares, 200_000);
/// ```
pub fn mul_div(x: u128, y: u128, denominator: u128, rounding: Rounding) -> u128 {
    use super::core::U256;

    let numerator = U256::from(x) * U256::from(y);
    let denominator = U256::from(denominator);
    let result = numerator / denominator;
    let remainder = numerator % denominator;

    match rounding {
        Rounding::Down => result.as_u128(),
        Rounding::Up => {
            if remainder > U256::zero() {
                result.as_u128() + 1
            } else {
                result.as_u128()
            }
        }
    }
}
