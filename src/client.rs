use std::{collections::HashMap, io::Read};

use bitcoincore_rpc::bitcoin::key::rand::thread_rng;
use bls_signatures::{PrivateKey, PublicKey, Signature};
use rs_merkle::MerkleProof;
use sha2::Sha256;
use stateless_bitcoin_l2::types::U8_32;

use crate::{
    aggregator::Sha256Algorithm,
    errors::StatelessBitcoinResult,
    utils::{hashing::hash_tx_hash_with_salt, transaction::SimpleTransaction},
};

pub struct Client {
    pub public_key: bls_signatures::PublicKey,
    pub private_key: bls_signatures::PrivateKey,
    pub transaction_history: HashMap<U8_32, SimpleTransaction>,

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

        (tx_hash, transaction.into())
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
        merkle_root: U8_32,
        merkle_proof: MerkleProof<Sha256Algorithm>,
        txid: U8_32,
        txid_index: usize,
        total_txs: usize,
    ) -> StatelessBitcoinResult<Signature> {
        if !self.transaction_history.contains_key(&txid) {
            return Err(anyhow::anyhow!("Transaction not found"));
        }

        if !merkle_proof.verify(merkle_root, &[txid_index], &[txid], total_txs) {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let signature = self.private_key.sign(&txid);

        Ok(signature)
    }
}
