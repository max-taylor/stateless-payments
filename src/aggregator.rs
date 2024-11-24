use std::collections::HashMap;

use rs_merkle::{Hasher, MerkleTree};
use sha2::{Digest, Sha256};

use crate::types::U8_32;

#[derive(Clone)]
pub struct Sha256Algorithm {}

impl Hasher for Sha256Algorithm {
    type Hash = U8_32;

    fn hash(data: &[u8]) -> U8_32 {
        let mut hasher = Sha256::new();

        hasher.update(data);
        <[u8; 32]>::from(hasher.finalize())
    }
}

pub struct Aggregator {
    pub txid_to_index: HashMap<U8_32, usize>,
    pub merkle_tree: MerkleTree<Sha256Algorithm>,
}

impl Aggregator {
    pub fn new() -> Aggregator {
        Aggregator {
            txid_to_index: HashMap::new(),
            merkle_tree: MerkleTree::new(),
        }
    }

    pub fn add_transaction(&mut self, txid: U8_32) {
        let index = self.merkle_tree.leaves_len();
        self.txid_to_index.insert(txid, index);
        self.merkle_tree.insert(txid).commit();
    }

    pub fn root(&self) -> Option<U8_32> {
        self.merkle_tree.root()
    }

    pub fn get_merkle_proofs_for_txid(&self, txid: U8_32) -> Option<U8_32> {
        let index = self.txid_to_index.get(&txid)?;

        self.merkle_tree
            .proof(&[*index])
            .proof_hashes()
            .first()
            .cloned()
    }
}
