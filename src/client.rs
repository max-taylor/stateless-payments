use std::collections::HashMap;

use anyhow::anyhow;
use bitcoincore_rpc::bitcoin::key::rand::thread_rng;
use bls_signatures::{PrivateKey, PublicKey, Signature};

use crate::{
    errors::StatelessBitcoinResult,
    types::{generate_salt, TransactionWithProof, U8_32},
    utils::transaction::SimpleTransaction,
};

pub struct Client {
    pub public_key: bls_signatures::PublicKey,
    pub private_key: bls_signatures::PrivateKey,
    pub transaction_history: HashMap<U8_32, TransactionWithProof>,
    pub uncomfirmed_transactions: HashMap<U8_32, SimpleTransaction>,

    pub balance: u64,
}

impl Client {
    pub fn new() -> Client {
        let private_key = PrivateKey::generate(&mut thread_rng());

        Client {
            private_key,
            public_key: private_key.public_key(),
            transaction_history: HashMap::new(),
            uncomfirmed_transactions: HashMap::new(),
            balance: 0,
        }
    }

    pub fn create_transaction(
        &mut self,
        to: PublicKey,
        amount: u64,
    ) -> StatelessBitcoinResult<(U8_32, SimpleTransaction)> {
        let salt = generate_salt();
        let transaction = SimpleTransaction {
            to,
            from: self.public_key,
            amount,
            salt,
        };

        let tx_hash = transaction.tx_hash();

        self.uncomfirmed_transactions
            .insert(tx_hash, transaction.clone());

        // TODO: add back in
        // self.balance
        //     .checked_sub(amount)
        //     .ok_or_else(|| anyhow!("Insufficient balance"))?;

        Ok((tx_hash, transaction))
    }

    pub fn add_transaction_with_proof(
        &mut self,
        transaction_proof: TransactionWithProof,
    ) -> StatelessBitcoinResult<()> {
        if transaction_proof.transaction.to != self.public_key
            || transaction_proof.transaction.from != self.public_key
        {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let tx_hash = transaction_proof.transaction.tx_hash();
        self.uncomfirmed_transactions.remove(&tx_hash);

        self.transaction_history
            .insert(tx_hash, transaction_proof.clone());

        Ok(())
    }

    pub fn validate_and_sign_transaction(
        &self,
        merkle_tree_proof: TransactionWithProof,
    ) -> StatelessBitcoinResult<Signature> {
        let tx_hash = merkle_tree_proof.transaction.tx_hash();

        if !self.transaction_history.contains_key(&tx_hash) {
            return Err(anyhow::anyhow!("Transaction not found"));
        }

        if !merkle_tree_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let signature = self.private_key.sign(&tx_hash);

        Ok(signature)
    }
}

pub fn calculate_balance(balance_proof: Vec<TransactionWithProof>, address: PublicKey) -> bool {
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
