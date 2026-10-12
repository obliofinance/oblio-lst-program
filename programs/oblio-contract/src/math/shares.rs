use anchor_lang::prelude::*;
use crate::errors::OblioError;

/// shares = assets * (supply + off) / (vested + off)   (floor: favours the vault)
pub fn shares_for_assets(assets: u64, vested: u64, supply: u64, offset: u64) -> Result<u64> {
    let num = (assets as u128)
        .checked_mul(supply as u128 + offset as u128)
        .ok_or(OblioError::MathOverflow)?;
    let den = vested as u128 + offset as u128;
    let out = num.checked_div(den).ok_or(OblioError::MathOverflow)?;
    u64::try_from(out).map_err(|_| error!(OblioError::MathOverflow))
}

/// assets = shares * (vested + off) / (supply + off)   (floor: favours the vault)
pub fn assets_for_shares(shares: u64, vested: u64, supply: u64, offset: u64) -> Result<u64> {
    let num = (shares as u128)
        .checked_mul(vested as u128 + offset as u128)
        .ok_or(OblioError::MathOverflow)?;
    let den = supply as u128 + offset as u128;
    let out = num.checked_div(den).ok_or(OblioError::MathOverflow)?;
    u64::try_from(out).map_err(|_| error!(OblioError::MathOverflow))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_is_one_to_one() {
        assert_eq!(shares_for_assets(5_000_000_000, 0, 0, 1_000_000).unwrap(), 5_000_000_000);
    }

    #[test]
    fn round_trip_never_profits() {
        let (vested, supply, off) = (1_234_567_891_u64, 1_000_000_007_u64, 1_000_000_u64);
        for dep in [1_000_000_000_u64, 1_000_000_123, 7_777_777_777] {
            let s = shares_for_assets(dep, vested, supply, off).unwrap();
            let back = assets_for_shares(s, vested + dep, supply + s, off).unwrap();
            assert!(back <= dep);
        }
    }

    #[test]
    fn donation_cannot_zero_out_victim() {
        let s = shares_for_assets(1_000_000_000, 1_000_000_000_000, 1, 1_000_000).unwrap();
        assert!(s > 0);
    }
}
