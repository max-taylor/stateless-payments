use std::collections::HashMap;

use anyhow::anyhow;

use crate::{
    errors::StatelessBitcoinResult,
    rollup::rollup_state::RollupContractTrait,
    types::{
        common::{
            generate_salt, BlsPublicKey, BlsSecretKey, BlsSignature, TransactionProof, U8_32,
        },
        transaction::SimpleTransaction,
    },
};

pub struct Client {
    pub public_key: BlsPublicKey,
    pub private_key: BlsSecretKey,

    // Mapping of Merkle Root -> (Transaction, TransactionProof)
    // We use the merkle root for lookups
    pub transaction_history: HashMap<U8_32, (SimpleTransaction, TransactionProof)>,
    // Mapping of Transaction Hash -> Transaction
    // Use the tx_hash for lookups in this case
    pub uncomfirmed_transactions: HashMap<U8_32, SimpleTransaction>,

    pub balance: u64,
}

impl Client {
    pub fn new() -> Client {
        let private_key = BlsSecretKey::new();

        Client {
            private_key: private_key.clone(),
            public_key: private_key.public_key(),
            transaction_history: HashMap::new(),
            uncomfirmed_transactions: HashMap::new(),
            balance: 0,
        }
    }

    pub fn sync_rollup_state(
        &mut self,
        rollup_state: &impl RollupContractTrait,
    ) -> StatelessBitcoinResult<()> {
        let deposit_amount = rollup_state.get_account_deposit_amount(self.public_key)?;
        let withdraw_amount = rollup_state.get_account_withdraw_amount(self.public_key)?;

        let transfer_blocks = rollup_state.get_account_transfer_blocks(self.public_key)?;

        Ok(())
        // self.transaction_history = rollup_state.get_transaction_history();
        // self.balance = rollup_state.get_balance(self.public_key);
    }

    pub fn create_transaction(
        &mut self,
        to: BlsPublicKey,
        amount: u64,
    ) -> StatelessBitcoinResult<SimpleTransaction> {
        let salt = generate_salt();

        if to == self.public_key {
            return Err(anyhow!("Cannot send to self"));
        }

        let transaction = SimpleTransaction {
            to,
            from: self.public_key,
            amount,
            salt,
        };

        self.uncomfirmed_transactions
            .insert(transaction.tx_hash(), transaction.clone());

        // TODO: add back in
        // self.balance
        //     .checked_sub(amount)
        //     .ok_or_else(|| anyhow!("Insufficient balance"))?;

        Ok(transaction)
    }

    // Called when another client sends funds to this client
    pub fn add_receiving_transaction(
        &mut self,
        transaction_proof: TransactionProof,
        // rollup_contract: impl RollupContractTrait,
    ) -> StatelessBitcoinResult<()> {
        // TODO: somewhere we need to check that the transaction has been submitted on-chain, will
        // work for both the user submitting a tx and the receiver getting a tx
        let tx_hash = transaction_proof.tx_hash;

        let transaction = self
            .uncomfirmed_transactions
            .get(&tx_hash)
            .ok_or(anyhow!("Transaction not found"))?;

        if transaction.to != self.public_key {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        // This isn't really needed because validate_and_sign_transaction will be called first and
        // it checks this, but it's here for completeness
        if !transaction_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        self.transaction_history.insert(
            transaction_proof.root,
            (transaction.clone(), transaction_proof.clone()),
        );

        self.uncomfirmed_transactions.remove(&tx_hash);

        Ok(())
    }

    // This is the function that the aggregator will call to get the signature
    // Internally we move the transaction from unconfirmed to confirmed as its been accepted by the
    // aggregator and a block will be produced
    //
    // ! You could validate the inclusion by using the RollupContractTrait, but doesn't seem
    // necessary
    pub fn validate_and_sign_transaction(
        &mut self,
        transaction_proof: TransactionProof,
    ) -> StatelessBitcoinResult<BlsSignature> {
        let tx_hash = transaction_proof.tx_hash;

        let transaction = self
            .uncomfirmed_transactions
            .get(&tx_hash)
            .ok_or(anyhow!("Transaction not found"))?;

        if !transaction_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let signature = self.private_key.sign(
            blsful::SignatureSchemes::MessageAugmentation,
            &transaction_proof.root,
        )?;

        self.transaction_history.insert(
            transaction_proof.root,
            (transaction.clone(), transaction_proof.clone()),
        );

        self.uncomfirmed_transactions.remove(&tx_hash);

        Ok(signature)
    }
}

pub fn calculate_balance(
    rollup_state: impl RollupContractTrait,
    pubkey: BlsPublicKey,
    balance_proof: Vec<()>,
) -> bool {
    let mut balance = 0.0;

    true

    // for proof in balance_proof {
    //     if proof.transaction.to == address {
    //         balance += proof.transaction.amount as f64;
    //     }
    // }
    //
    // balance_proof
    //     .iter()
    //     .all(|proof| proof.transaction.to == address)
}
