use std::collections::HashMap;

use anyhow::anyhow;

use crate::{
    errors::StatelessBitcoinResult,
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

    pub transaction_history: HashMap<U8_32, (SimpleTransaction, TransactionProof)>,
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

    pub fn create_transaction(
        &mut self,
        to: BlsPublicKey,
        amount: u64,
    ) -> StatelessBitcoinResult<SimpleTransaction> {
        let salt = generate_salt();
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

    pub fn add_transaction_with_proof(
        &mut self,
        transaction_proof: TransactionProof,
    ) -> StatelessBitcoinResult<()> {
        // TODO: somewhere we need to check that the transaction has been submitted on-chain
        let tx_hash = transaction_proof.tx_hash;

        let transaction = self
            .uncomfirmed_transactions
            .get(&tx_hash)
            .ok_or(anyhow!("Transaction not found"))?;

        if transaction.to != self.public_key || transaction.from != self.public_key {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        // This isn't really needed because validate_and_sign_transaction will be called first and
        // it checks this, but it's here for completeness
        if !transaction_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        self.transaction_history
            .insert(tx_hash, (transaction.clone(), transaction_proof.clone()));

        self.uncomfirmed_transactions.remove(&tx_hash);

        Ok(())
    }

    pub fn validate_and_sign_transaction(
        &self,
        merkle_tree_proof: TransactionProof,
    ) -> StatelessBitcoinResult<BlsSignature> {
        let tx_hash = merkle_tree_proof.tx_hash;

        if !self.uncomfirmed_transactions.contains_key(&tx_hash) {
            return Err(anyhow::anyhow!("Transaction not found"));
        }

        if !merkle_tree_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let signature = self.private_key.sign(
            blsful::SignatureSchemes::MessageAugmentation,
            &merkle_tree_proof.root,
        )?;

        Ok(signature)
    }
}

pub fn calculate_balance(balance_proof: Vec<TransactionProof>, address: BlsPublicKey) -> bool {
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
