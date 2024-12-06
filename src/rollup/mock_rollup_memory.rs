use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{
    errors::CrateResult,
    types::{common::TransferBlock, public_key::AccountTotals, signatures::BlsPublicKey},
};

use super::traits::{AsyncCrateResult, MockRollupStateTrait, RollupStateTrait};

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

#[async_trait]
impl MockRollupStateTrait for MockRollupMemory {
    async fn add_deposit(&mut self, pubkey: BlsPublicKey, amount: u64) -> CrateResult<()> {
        self.deposit_totals
            .entry(pubkey.into())
            .and_modify(|e| *e += amount)
            .or_insert(amount);

        Ok(())
    }

    async fn add_withdraw(&mut self, pubkey: &BlsPublicKey, amount: u64) -> CrateResult<()> {
        let deposit_amount = self.get_account_deposit_amount(&pubkey).await?;
        let withdraw_amount = self.get_account_withdraw_amount(&pubkey).await?;

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

#[async_trait]
impl RollupStateTrait for MockRollupMemory {
    async fn add_transfer_block(&mut self, transfer_block: TransferBlock) -> CrateResult<()> {
        self.transfer_blocks.push(transfer_block);

        Ok(())
    }
    async fn get_withdraw_totals(&self) -> AsyncCrateResult<AccountTotals> {
        Ok(self.withdraw_totals.clone())
    }

    async fn get_deposit_totals(&self) -> CrateResult<AccountTotals> {
        Ok(self.deposit_totals.clone())
    }

    async fn get_transfer_blocks(&self) -> CrateResult<Vec<TransferBlock>> {
        Ok(self.transfer_blocks.clone())
    }
}

// impl RollupStateTrait for Arc<Mutex<MockRollupMemory>> {
//     async fn add_transfer_block(&mut self, transfer_block: TransferBlock) -> CrateResult<()> {
//         self.lock().await.add_transfer_block(transfer_block)
//     }
//     fn get_withdraw_totals(&self) -> CrateResult<AccountTotals> {
//         self.lock().unwrap().get_withdraw_totals()
//     }
//
//     fn get_deposit_totals(&self) -> CrateResult<AccountTotals> {
//         self.lock().unwrap().get_deposit_totals()
//     }
//
//     fn get_transfer_blocks(&self) -> CrateResult<Vec<TransferBlock>> {
//         self.lock().unwrap().get_transfer_blocks()
//     }
// }
