use anyhow::anyhow;

use crate::{
    errors::StatelessBitcoinResult,
    types::{
        common::{BlsPublicKey, TransferBlock},
        public_key::AccountTotals,
    },
};

pub trait RollupStateTrait {
    fn get_withdraw_totals(&self) -> StatelessBitcoinResult<&AccountTotals>;

    fn get_account_withdraw_amount(&self, pubkey: &BlsPublicKey) -> StatelessBitcoinResult<u64> {
        let withdraw_totals = self.get_withdraw_totals()?;
        Ok(*withdraw_totals.get(&pubkey.into()).unwrap_or(&0))
    }

    fn get_deposit_totals(&self) -> StatelessBitcoinResult<&AccountTotals>;

    fn get_account_deposit_amount(&self, pubkey: &BlsPublicKey) -> StatelessBitcoinResult<u64> {
        let deposit_totals = self.get_deposit_totals()?;
        Ok(*deposit_totals.get(&pubkey.into()).unwrap_or(&0))
    }

    fn get_transfer_blocks(&self) -> StatelessBitcoinResult<&Vec<TransferBlock>>;

    fn get_account_transfer_blocks(
        &self,
        pubkey: BlsPublicKey,
    ) -> StatelessBitcoinResult<Vec<TransferBlock>> {
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
    ) -> StatelessBitcoinResult<Option<TransferBlock>> {
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

pub struct MockRollupState {
    pub withdraw_totals: AccountTotals,
    pub deposit_totals: AccountTotals,
    pub transfer_blocks: Vec<TransferBlock>,
}

impl MockRollupState {
    pub fn new() -> MockRollupState {
        MockRollupState {
            withdraw_totals: AccountTotals::new(),
            deposit_totals: AccountTotals::new(),
            transfer_blocks: vec![],
        }
    }

    pub fn add_transfer_block(&mut self, transfer_block: TransferBlock) {
        self.transfer_blocks.push(transfer_block);
    }

    pub fn add_deposit(&mut self, pubkey: BlsPublicKey, amount: u64) {
        self.deposit_totals
            .entry(pubkey.into())
            .and_modify(|e| *e += amount)
            .or_insert(amount);
    }

    pub fn add_withdraw(
        &mut self,
        pubkey: &BlsPublicKey,
        amount: u64,
    ) -> StatelessBitcoinResult<()> {
        let deposit_amount = self.get_account_deposit_amount(&pubkey)?;
        let withdraw_amount = self.get_account_withdraw_amount(&pubkey)?;

        dbg!(deposit_amount, withdraw_amount);

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

impl RollupStateTrait for MockRollupState {
    fn get_withdraw_totals(&self) -> StatelessBitcoinResult<&AccountTotals> {
        Ok(&self.withdraw_totals)
    }

    fn get_deposit_totals(&self) -> StatelessBitcoinResult<&AccountTotals> {
        Ok(&self.deposit_totals)
    }

    fn get_transfer_blocks(&self) -> StatelessBitcoinResult<&Vec<TransferBlock>> {
        Ok(&self.transfer_blocks)
    }
}
