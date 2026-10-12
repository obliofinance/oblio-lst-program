use anchor_lang::prelude::*;
use crate::constants::{BPS_DENOMINATOR, WAD};
use crate::errors::OblioError;
use crate::state::ProtocolParams;

fn pow_wad(x: u128, n: u32) -> u128 {
    let mut r = WAD;
    for _ in 0..n {
        r = r * x / WAD;
    }
    r
}

/// Instant-exit fee as the *integral* of the utilisation curve over the reserve consumed by the trade:
///
///   curve(u) = min + (max - min) * u^k,   u = (target - reserve) / target  (clamped to [0, 1])
///   fee      = gross * min + (max - min) * target * (u1^(k+1) - u0^(k+1)) / (k + 1)
///
/// Draining the reserve in one transaction costs the same as draining it in many small ones.
/// Pure u128 fixed point, no floating point.
pub fn instant_exit_fee(gross: u64, reserve: u64, target: u64, p: &ProtocolParams) -> Result<u64> {
    let bps = BPS_DENOMINATOR as u128;
    let gross128 = gross as u128;
    let min = p.fee_min_bps as u128;
    let max = p.fee_max_bps as u128;
    let k1 = p.fee_curve_exponent as u32 + 1;

    let t = target.max(1) as u128;
    let r0 = reserve as u128;
    let r1 = r0.saturating_sub(gross128);

    let util = |r: u128| if r >= t { 0 } else { (t - r) * WAD / t };
    let (u0, u1) = (util(r0), util(r1));

    let base = gross128 * min / bps;
    let diff = pow_wad(u1, k1).saturating_sub(pow_wad(u0, k1));
    let extra = (max - min)
        .checked_mul(t)
        .and_then(|v| v.checked_mul(diff))
        .ok_or(OblioError::MathOverflow)?
        / (k1 as u128 * bps * WAD);

    let cap = gross128 * max / bps;
    u64::try_from((base + extra).min(cap)).map_err(|_| error!(OblioError::MathOverflow))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(min: u16, max: u16, k: u8) -> ProtocolParams {
        ProtocolParams {
            reserve_ratio_bps: 500, fee_min_bps: min, fee_max_bps: max, fee_curve_exponent: k,
            protocol_fee_share_bps: 5_000, min_deposit_lamports: 1, min_unstake_lamports: 1,
            min_delegation_lamports: 1, min_total_shares: 0, min_reward_injection_lamports: 1,
            tvl_cap_lamports: u64::MAX, max_buckets: 10, max_bucket_share_bps: 10_000,
            rebalance_tolerance_bps: 100, ticket_cooldown_epochs: 1,
            profit_vesting_secs: 100, rate_drop_tolerance_bps: 10, max_rate_jump_bps: 100, timelock_secs: 3_600,
        }
    }

    #[test]
    fn full_reserve_pays_min_fee() {
        let p = params(10, 500, 2);
        assert_eq!(instant_exit_fee(1_000, 10_000, 10_000, &p).unwrap(), 1);
    }

    #[test]
    fn one_big_trade_equals_two_halves() {
        let p = params(10, 500, 2);
        let whole = instant_exit_fee(10_000_000, 10_000_000, 10_000_000, &p).unwrap();
        let a = instant_exit_fee(5_000_000, 10_000_000, 10_000_000, &p).unwrap();
        let b = instant_exit_fee(5_000_000, 5_000_000, 10_000_000, &p).unwrap();
        assert!(whole.abs_diff(a + b) <= 2);
    }

    #[test]
    fn fee_never_exceeds_cap() {
        let p = params(10, 500, 3);
        let f = instant_exit_fee(1_000_000, 1_000_000, 100_000_000, &p).unwrap();
        assert!(f <= 1_000_000 * 500 / 10_000);
    }
}
