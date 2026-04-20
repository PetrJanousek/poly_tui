/// Taker fee rates by market category.
/// Only takers pay fees; makers are never charged.
/// Formula: fee = shares × rate × price × (1 − price)
/// Fees peak at p=0.50 and taper symmetrically toward 0 and 1.
pub const CRYPTO_FEE_RATE: f64 = 0.072;

pub fn calc_fee(shares: f64, price: f64, fee_rate: f64) -> f64 {
    shares * fee_rate * price * (1.0 - price)
}
