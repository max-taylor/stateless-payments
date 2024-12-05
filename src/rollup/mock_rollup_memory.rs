use anyhow::anyhow;

use crate::{
    errors::CrateResult,
    types::{common::TransferBlock, public_key::AccountTotals, signatures::BlsPublicKey},
};

use super::traits::{MockRollupStateTrait, RollupStateTrait};

// This is mostly used for test cases
pub struct MockRollupMemory {
    pub withdraw_totals: AccountTotals,
    pub deposit_totals: AccountTotals,
    pub transfer_blocks: Vec<TransferBlock>,
}

impl MockRollupMemory {
    pub fn new() -> MockRollupMemory {
        MockRollupMemory {
            withdraw_totals: AccountTotals::new(),
            deposit_totals: AccountTotals::new(),
            transfer_blocks: vec![],
        }
    }
}

impl MockRollupStateTrait for MockRollupMemory {
    fn add_deposit(&mut self, pubkey: BlsPublicKey, amount: u64) -> CrateResult<()> {
        self.deposit_totals
            .entry(pubkey.into())
            .and_modify(|e| *e += amount)
            .or_insert(amount);

        Ok(())
    }

    fn add_withdraw(&mut self, pubkey: &BlsPublicKey, amount: u64) -> CrateResult<()> {
        let deposit_amount = self.get_account_deposit_amount(&pubkey)?;
        let withdraw_amount = self.get_account_withdraw_amount(&pubkey)?;

        if deposit_amount < withdraw_amount + amount {
            return Err(anyhow!("Insufficient funds"));
        }

        self.withdraw_totals
            .entry(pubkey.into())
            .and_modify(|e| *e += amount)
            .or_insert(amount);

        Ok(())
    }
}

impl RollupStateTrait for MockRollupMemory {
    fn add_transfer_block(&mut self, transfer_block: TransferBlock) -> CrateResult<()> {
        self.transfer_blocks.push(transfer_block);

        Ok(())
    }
    fn get_withdraw_totals(&self) -> CrateResult<AccountTotals> {
        Ok(self.withdraw_totals.clone())
    }

    fn get_deposit_totals(&self) -> CrateResult<AccountTotals> {
        Ok(self.deposit_totals.clone())
    }

    fn get_transfer_blocks(&self) -> CrateResult<Vec<TransferBlock>> {
        Ok(self.transfer_blocks.clone())
    }
}
