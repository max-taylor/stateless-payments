use bls_signatures::{PublicKey, Serialize};

use crate::{
    errors::StatelessBitcoinResult,
    types::{AccountTotals, TransferBlock},
};

trait RollupContractTrait {
    fn get_withdraw_totals(&self) -> StatelessBitcoinResult<&AccountTotals>;

    fn get_deposit_blocks(&self) -> StatelessBitcoinResult<&AccountTotals>;

    fn get_transfer_blocks(&self) -> StatelessBitcoinResult<&Vec<TransferBlock>>;
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

    fn add_deposit(&mut self, pubkey: PublicKey, amount: u64) {
        self.deposit_blocks.insert(pubkey.into(), amount);
        // self.deposit_blocks.add(pubkey, amount);
    }

    fn add_withdraw_block(&mut self, withdraw_block: AccountTotals) {
        self.withdraw_totals = withdraw_block;
    }
}

impl RollupContractTrait for MockRollupState {
    fn get_withdraw_totals(&self) -> StatelessBitcoinResult<&AccountTotals> {
        Ok(&self.withdraw_totals)
    }

    fn get_deposit_blocks(&self) -> StatelessBitcoinResult<&AccountTotals> {
        Ok(&self.deposit_blocks)
    }

    fn get_transfer_blocks(&self) -> StatelessBitcoinResult<&Vec<TransferBlock>> {
        Ok(&self.transfer_blocks)
    }
}
