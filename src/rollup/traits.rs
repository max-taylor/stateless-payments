use crate::{
    errors::CrateResult,
    types::{
        common::{BlsPublicKey, TransferBlock},
        public_key::AccountTotals,
    },
};

pub trait RollupStateTrait {
    fn add_transfer_block(&mut self, transfer_block: TransferBlock) -> CrateResult<()>;

    fn get_withdraw_totals(&self) -> CrateResult<AccountTotals>;

    fn get_account_withdraw_amount(&self, pubkey: &BlsPublicKey) -> CrateResult<u64> {
        let withdraw_totals = self.get_withdraw_totals()?;
        Ok(*withdraw_totals.get(&pubkey.into()).unwrap_or(&0))
    }

    fn get_deposit_totals(&self) -> CrateResult<AccountTotals>;

    fn get_account_deposit_amount(&self, pubkey: &BlsPublicKey) -> CrateResult<u64> {
        let deposit_totals = self.get_deposit_totals()?;
        Ok(*deposit_totals.get(&pubkey.into()).unwrap_or(&0))
    }

    fn get_transfer_blocks(&self) -> CrateResult<Vec<TransferBlock>>;

    fn get_account_transfer_blocks(&self, pubkey: BlsPublicKey) -> CrateResult<Vec<TransferBlock>> {
        let transfer_blocks = self.get_transfer_blocks()?;
        Ok(transfer_blocks
            .iter()
            .filter(|transfer_block| transfer_block.contains_pubkey(&pubkey))
            .cloned()
            .collect())
    }

    fn get_transfer_block_for_merkle_root_and_pubkey(
        &self,
        merkle_root: &[u8; 32],
        pubkey: &BlsPublicKey,
    ) -> CrateResult<Option<TransferBlock>> {
        let transfer_blocks = self.get_transfer_blocks()?;
        Ok(transfer_blocks
            .iter()
            .find(|transfer_block| {
                transfer_block.merkle_root == *merkle_root
                    && transfer_block.contains_pubkey(&pubkey)
            })
            .cloned())
    }
}

pub trait MockRollupStateTrait: RollupStateTrait {
    fn add_deposit(&mut self, pubkey: BlsPublicKey, amount: u64) -> CrateResult<()>;

    fn add_withdraw(&mut self, pubkey: &BlsPublicKey, amount: u64) -> CrateResult<()>;
}
