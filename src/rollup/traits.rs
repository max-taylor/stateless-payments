use async_trait::async_trait;

use crate::{
    errors::CrateResult,
    types::{common::TransferBlock, public_key::AccountTotals, signatures::BlsPublicKey},
};

#[async_trait]
pub trait RollupStateTrait {
    async fn add_transfer_block(&mut self, transfer_block: TransferBlock) -> CrateResult<()>;

    async fn get_withdraw_totals(&self) -> CrateResult<AccountTotals>;

    async fn get_account_withdraw_amount(&self, pubkey: &BlsPublicKey) -> CrateResult<u64> {
        let withdraw_totals = self.get_withdraw_totals().await?;
        Ok(*withdraw_totals.get(&pubkey.into()).unwrap_or(&0))
    }

    async fn get_deposit_totals(&self) -> CrateResult<AccountTotals>;

    async fn get_account_deposit_amount(&self, pubkey: &BlsPublicKey) -> CrateResult<u64> {
        let deposit_totals = self.get_deposit_totals().await?;
        Ok(*deposit_totals.get(&pubkey.into()).unwrap_or(&0))
    }

    async fn get_transfer_blocks(&self) -> CrateResult<Vec<TransferBlock>>;

    async fn get_account_transfer_blocks(
        &self,
        pubkey: &BlsPublicKey,
    ) -> CrateResult<Vec<TransferBlock>> {
        let transfer_blocks = self.get_transfer_blocks().await?;
        Ok(transfer_blocks
            .iter()
            .filter(|transfer_block| transfer_block.contains_pubkey(&pubkey))
            .cloned()
            .collect())
    }

    async fn get_transfer_block_for_merkle_root_and_pubkey(
        &self,
        merkle_root: &[u8; 32],
        pubkey: &BlsPublicKey,
    ) -> CrateResult<Option<TransferBlock>> {
        let transfer_blocks = self.get_transfer_blocks().await?;
        Ok(transfer_blocks
            .iter()
            .find(|transfer_block| {
                transfer_block.merkle_root == *merkle_root
                    && transfer_block.contains_pubkey(&pubkey)
            })
            .cloned())
    }
}

#[async_trait]
pub trait MockRollupStateTrait: RollupStateTrait {
    async fn add_deposit(&mut self, pubkey: BlsPublicKey, amount: u64) -> CrateResult<()>;

    async fn add_withdraw(&mut self, pubkey: &BlsPublicKey, amount: u64) -> CrateResult<()>;
}
