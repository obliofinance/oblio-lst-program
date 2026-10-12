use anchor_lang::{
    prelude::*,
    solana_program::program::{invoke, invoke_signed},
    system_program::{self, Allocate, Assign},
};
use solana_stake_interface::{
    instruction as stake_ix,
    state::{Authorized, Lockup, StakeAuthorize, StakeStateV2},
};
use crate::constants::*;
use crate::errors::OblioError;

pub fn stake_rent_reserve() -> Result<u64> {
    Ok(Rent::get()?.minimum_balance(StakeStateV2::size_of()))
}

/// Turns a (possibly pre-funded) system-owned PDA into an uninitialised stake account.
/// Uses allocate + assign (never create_account) so pre-funding the address cannot grief setup.
pub fn allocate_stake_pda<'info>(
    pda: &AccountInfo<'info>,
    system_program_info: &AccountInfo<'info>,
    seeds: &[&[&[u8]]],
) -> Result<()> {
    system_program::allocate(
        CpiContext::new_with_signer(
            system_program_info.key(),
            Allocate { account_to_allocate: pda.clone() },
            seeds,
        ),
        StakeStateV2::size_of() as u64,
    )?;
    system_program::assign(
        CpiContext::new_with_signer(
            system_program_info.key(),
            Assign { account_to_assign: pda.clone() },
            seeds,
        ),
        &STAKE_PROGRAM_ID,
    )
}

pub fn initialize_stake<'info>(
    stake: &AccountInfo<'info>,
    rent: &AccountInfo<'info>,
    stake_program: &AccountInfo<'info>,
    authority: &Pubkey,
) -> Result<()> {
    let ix = stake_ix::initialize(
        &stake.key(),
        &Authorized { staker: *authority, withdrawer: *authority },
        &Lockup::default(),
    );
    invoke(&ix, &[stake.clone(), rent.clone(), stake_program.clone()])?;
    Ok(())
}

/// Stake-program CPIs signed by the gateway PDA.
pub struct StakeCpi<'info> {
    pub stake_program: AccountInfo<'info>,
    pub clock: AccountInfo<'info>,
    pub stake_history: AccountInfo<'info>,
    pub gateway: AccountInfo<'info>,
    pub gateway_bump: u8,
}

impl<'info> StakeCpi<'info> {
    fn gw_seeds(&self) -> [&[u8]; 2] {
        // bump lives in `self`, so build the slice per call
        [GATEWAY_SEED, std::slice::from_ref(&self.gateway_bump)]
    }

    pub fn delegate(
        &self,
        stake: &AccountInfo<'info>,
        vote: &AccountInfo<'info>,
        stake_config: &AccountInfo<'info>,
    ) -> Result<()> {
        let ix = stake_ix::delegate_stake(&stake.key(), &self.gateway.key(), &vote.key());
        let gw = self.gw_seeds();
        invoke_signed(
            &ix,
            &[
                stake.clone(), vote.clone(), self.clock.clone(), self.stake_history.clone(),
                stake_config.clone(), self.gateway.clone(), self.stake_program.clone(),
            ],
            &[&gw],
        )?;
        Ok(())
    }

    pub fn deactivate(&self, stake: &AccountInfo<'info>) -> Result<()> {
        let ix = stake_ix::deactivate_stake(&stake.key(), &self.gateway.key());
        let gw = self.gw_seeds();
        invoke_signed(
            &ix,
            &[stake.clone(), self.clock.clone(), self.gateway.clone(), self.stake_program.clone()],
            &[&gw],
        )?;
        Ok(())
    }

    pub fn merge(
        &self,
        destination: &AccountInfo<'info>,
        source: &AccountInfo<'info>,
        err: OblioError,
    ) -> Result<()> {
        let ixs = stake_ix::merge(&destination.key(), &source.key(), &self.gateway.key());
        let gw = self.gw_seeds();
        let infos = [
            destination.clone(), source.clone(), self.clock.clone(), self.stake_history.clone(),
            self.gateway.clone(), self.stake_program.clone(),
        ];
        for ix in ixs.iter() {
            invoke_signed(ix, &infos, &[&gw]).map_err(|_| anchor_lang::error::Error::from(err))?;
        }
        Ok(())
    }

    /// Withdraw `lamports` from an inactive stake account into the gateway.
    pub fn withdraw_to_gateway(&self, stake: &AccountInfo<'info>, lamports: u64) -> Result<()> {
        let ix = stake_ix::withdraw(&stake.key(), &self.gateway.key(), &self.gateway.key(), lamports, None);
        let gw = self.gw_seeds();
        invoke_signed(
            &ix,
            &[
                stake.clone(), self.gateway.clone(), self.clock.clone(), self.stake_history.clone(),
                self.gateway.clone(), self.stake_program.clone(),
            ],
            &[&gw],
        )
        .map_err(|_| error!(OblioError::StakeNotInactive))?;
        Ok(())
    }

    /// Splits `lamports` of delegated stake from `source` into the pre-funded PDA `dest`.
    pub fn split(
        &self,
        source: &AccountInfo<'info>,
        dest: &AccountInfo<'info>,
        system_program_info: &AccountInfo<'info>,
        lamports: u64,
        dest_seeds: &[&[u8]],
    ) -> Result<()> {
        let ixs = stake_ix::split(&source.key(), &self.gateway.key(), lamports, &dest.key());
        let gw = self.gw_seeds();
        let infos = [
            source.clone(), dest.clone(), self.gateway.clone(),
            system_program_info.clone(), self.stake_program.clone(),
        ];
        for ix in ixs.iter() {
            invoke_signed(ix, &infos, &[&gw, dest_seeds])?;
        }
        Ok(())
    }

    /// Hand both authorities of `stake` from `current` (a signer on this tx) to the gateway.
    pub fn take_authorities(
        &self,
        stake: &AccountInfo<'info>,
        current: &AccountInfo<'info>,
    ) -> Result<()> {
        for kind in [StakeAuthorize::Staker, StakeAuthorize::Withdrawer] {
            let ix = stake_ix::authorize(&stake.key(), &current.key(), &self.gateway.key(), kind, None);
            invoke(
                &ix,
                &[stake.clone(), self.clock.clone(), current.clone(), self.stake_program.clone()],
            )?;
        }
        Ok(())
    }
}
