use crate::{
    errors::StatelessBitcoinResult,
    types::{
        common::{BlsPublicKey, TransferBlock},
        public_key::AccountTotals,
    },
};

pub trait RollupContractTrait {
    fn get_withdraw_totals(&self) -> StatelessBitcoinResult<&AccountTotals>;

    fn get_account_withdraw_amount(&self, pubkey: BlsPublicKey) -> StatelessBitcoinResult<u64> {
        let withdraw_totals = self.get_withdraw_totals()?;
        Ok(*withdraw_totals.get(&pubkey.into()).unwrap_or(&0))
    }

    fn get_deposit_totals(&self) -> StatelessBitcoinResult<&AccountTotals>;

    fn get_account_deposit_amount(&self, pubkey: BlsPublicKey) -> StatelessBitcoinResult<u64> {
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
            .filter(|transfer_block| transfer_block.public_keys.contains(&pubkey))
            .cloned()
            .collect())
    }
}

pub struct MockRollupState {
    pub withdraw_totals: AccountTotals,
    pub deposit_blocks: AccountTotals,
    pub transfer_blocks: Vec<TransferBlock>,
}

impl MockRollupState {
    pub fn new() -> MockRollupState {
        MockRollupState {
            withdraw_totals: AccountTotals::new(),
            deposit_blocks: AccountTotals::new(),
            transfer_blocks: vec![],
        }
    }

    fn add_transfer_block(&mut self, transfer_block: TransferBlock) {
        self.transfer_blocks.push(transfer_block);
    }

    fn add_deposit(&mut self, pubkey: BlsPublicKey, amount: u64) {
        self.deposit_blocks.insert(pubkey.into(), amount);
    }

    fn add_withdraw_block(&mut self, pubkey: BlsPublicKey, amount: u64) {
        self.withdraw_totals.insert(pubkey.into(), amount);
    }
}

impl RollupContractTrait for MockRollupState {
    fn get_withdraw_totals(&self) -> StatelessBitcoinResult<&AccountTotals> {
        Ok(&self.withdraw_totals)
    }

    fn get_deposit_totals(&self) -> StatelessBitcoinResult<&AccountTotals> {
        Ok(&self.deposit_blocks)
    }

    fn get_transfer_blocks(&self) -> StatelessBitcoinResult<&Vec<TransferBlock>> {
        Ok(&self.transfer_blocks)
    }
}
