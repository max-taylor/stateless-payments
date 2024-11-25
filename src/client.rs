use std::collections::HashMap;

use bitcoincore_rpc::bitcoin::{key::rand::thread_rng, merkle_tree};
use bls_signatures::{PrivateKey, PublicKey, Signature};

use crate::{
    errors::StatelessBitcoinResult,
    types::{MerkleTreeProof, U8_32},
    utils::{hashing::hash_tx_hash_with_salt, transaction::SimpleTransaction},
};

pub struct Client {
    pub public_key: bls_signatures::PublicKey,
    pub private_key: bls_signatures::PrivateKey,
    pub transaction_history: HashMap<U8_32, SimpleTransaction>,
    // pub balance_proof: IndexMap<U8_32>
    pub balance: u64,
}

impl Client {
    pub fn new() -> Client {
        let private_key = PrivateKey::generate(&mut thread_rng());

        Client {
            private_key,
            public_key: private_key.public_key(),
            transaction_history: HashMap::new(),
            balance: 0,
        }
    }

    pub fn construct_transaction(
        &mut self,
        to: PublicKey,
        amount: u64,
        salt: U8_32,
    ) -> (U8_32, SimpleTransaction) {
        let transaction = SimpleTransaction {
            to,
            from: self.public_key,
            amount,
        };
        let tx_hash = hash_tx_hash_with_salt(&transaction.clone().into(), &salt);

        self.transaction_history
            .insert(tx_hash, transaction.clone());

        (tx_hash, transaction)
    }

    pub fn add_transaction(
        &mut self,
        merkle_root: U8_32,
        merkle_proof: U8_32,
        transaction: SimpleTransaction,
        salt: U8_32,
    ) -> StatelessBitcoinResult<()> {
        if transaction.to != self.public_key || transaction.from != self.public_key {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let tx_hash = hash_tx_hash_with_salt(&transaction.clone().into(), &salt);

        self.transaction_history
            .insert(tx_hash, transaction.clone());

        self.balance += transaction.amount;

        Ok(())
    }

    pub fn validate_and_sign_transaction(
        &self,
        merkle_tree_proof: MerkleTreeProof,
    ) -> StatelessBitcoinResult<Signature> {
        let txid = hash_tx_hash_with_salt(
            &merkle_tree_proof.transaction.clone().into(),
            &merkle_tree_proof.salt,
        );
        if !self.transaction_history.contains_key(&txid) {
            return Err(anyhow::anyhow!("Transaction not found"));
        }

        if !merkle_tree_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let signature = self.private_key.sign(&txid);

        Ok(signature)
    }
}

pub fn calculate_balance(balance_proof: Vec<MerkleTreeProof>, address: PublicKey) -> bool {
    let mut balance = 0.0;

    for proof in balance_proof {
        if proof.transaction.to == address {
            balance += proof.transaction.amount as f64;
        }
    }

    balance_proof
        .iter()
        .all(|proof| proof.transaction.to == address)
}
