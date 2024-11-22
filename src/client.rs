use std::collections::HashMap;

use bitcoincore_rpc::bitcoin::{key::rand::thread_rng, Txid};
use bls_signatures::{PrivateKey, PublicKey};

use crate::{
    types::Salt,
    utils::{hashing::hash_tx_hash_with_salt, transaction::SimpleTransaction},
};

pub struct Client {
    pub public_key: bls_signatures::PublicKey,
    pub private_key: bls_signatures::PrivateKey,
    pub transaction_history: HashMap<Txid, SimpleTransaction>,
}

impl Client {
    pub fn new() -> Client {
        let private_key = PrivateKey::generate(&mut thread_rng());

        Client {
            private_key,
            public_key: private_key.public_key(),
            transaction_history: HashMap::new(),
        }
    }

    pub fn construct_transaction(
        &mut self,
        to: PublicKey,
        amount: u64,
        salt: Salt,
    ) -> (Txid, SimpleTransaction) {
        let transaction = SimpleTransaction {
            to,
            from: self.public_key,
            amount,
        };
        let tx_hash = hash_tx_hash_with_salt(transaction.clone().into(), &salt);

        self.transaction_history
            .insert(tx_hash, transaction.clone());

        (tx_hash, transaction.into())
    }
}
