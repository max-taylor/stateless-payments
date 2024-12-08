use anyhow::anyhow;
use async_trait::async_trait;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{from_reader, to_writer};
use std::fs::OpenOptions;

use crate::{
    errors::CrateResult,
    types::{common::TransferBlock, public_key::AccountTotals, signatures::BlsPublicKey},
};

use super::traits::{MockRollupStateTrait, RollupStateTrait};

// This simply is just the struct that we will be writing to the file system
#[derive(Debug, Serialize, Deserialize)]
struct RollupState {
    withdraw_totals: AccountTotals,
    deposit_totals: AccountTotals,
    transfer_blocks: Vec<TransferBlock>,
}

impl RollupState {
    fn new() -> CrateResult<RollupState> {
        Ok(RollupState {
            withdraw_totals: AccountTotals::new(),
            deposit_totals: AccountTotals::new(),
            transfer_blocks: vec![],
        })
    }
}

// This is used for local demo's, so that we can persist the state
//
// The state inside this class is intentionally empty, this prevents any misuse where we modify the
// memory
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MockRollupFS {}

impl MockRollupFS {
    pub fn new() -> CrateResult<MockRollupFS> {
        Ok(MockRollupFS {})
    }

    fn read_state_from_fs() -> CrateResult<RollupState> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("rollup_state.json")?;

        file.lock_exclusive()?;

        let state: RollupState = match from_reader(&file) {
            Ok(state) => state,
            Err(e) => {
                dbg!(e);
                RollupState::new()?
            }
        };

        file.unlock().expect("Unable to unlock file");

        Ok(state)
    }

    fn write_state_to_fs(state: RollupState) -> CrateResult<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open("rollup_state.json")?;

        file.lock_exclusive()?;

        to_writer(&file, &state)?;

        file.unlock()?;
        Ok(())
    }
}

#[async_trait]
impl MockRollupStateTrait for MockRollupFS {
    async fn add_deposit(&mut self, pubkey: &BlsPublicKey, amount: u64) -> CrateResult<()> {
        let mut state = MockRollupFS::read_state_from_fs()?;

        state
            .deposit_totals
            .entry(pubkey.into())
            .and_modify(|e| *e += amount)
            .or_insert(amount);
        MockRollupFS::write_state_to_fs(state)?;

        Ok(())
    }

    async fn add_withdraw(&mut self, pubkey: &BlsPublicKey, amount: u64) -> CrateResult<()> {
        let deposit_amount = self.get_account_deposit_amount(&pubkey).await?;
        let withdraw_amount = self.get_account_withdraw_amount(&pubkey).await?;

        if deposit_amount < withdraw_amount + amount {
            return Err(anyhow!("Insufficient funds"));
        }

        let mut state = MockRollupFS::read_state_from_fs()?;
        state
            .withdraw_totals
            .entry(pubkey.into())
            .and_modify(|e| *e += amount)
            .or_insert(amount);

        MockRollupFS::write_state_to_fs(state)?;

        Ok(())
    }
}

#[async_trait]
impl RollupStateTrait for MockRollupFS {
    async fn add_transfer_block(&mut self, transfer_block: TransferBlock) -> CrateResult<()> {
        // Sync to FS
        let mut state = MockRollupFS::read_state_from_fs()?;
        state.transfer_blocks.push(transfer_block);
        MockRollupFS::write_state_to_fs(state)?;

        Ok(())
    }

    async fn get_withdraw_totals(&self) -> CrateResult<AccountTotals> {
        // Reload from FS
        let state = MockRollupFS::read_state_from_fs()?;
        Ok(state.withdraw_totals)
    }

    async fn get_deposit_totals(&self) -> CrateResult<AccountTotals> {
        let state = MockRollupFS::read_state_from_fs()?;
        Ok(state.deposit_totals)
    }

    async fn get_transfer_blocks(&self) -> CrateResult<Vec<TransferBlock>> {
        let state = MockRollupFS::read_state_from_fs()?;
        Ok(state.transfer_blocks)
    }
}
