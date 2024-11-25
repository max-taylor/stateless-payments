use bitcoincore_rpc::bitcoin::key::rand;
use bls_signatures::{PublicKey, Signature};
use rs_merkle::MerkleProof;

use crate::{
    aggregator::Sha256Algorithm,
    utils::{hashing::hash_tx_hash_with_salt, transaction::SimpleTransaction},
};

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    rand::random()
}

pub struct TransferBlock {
    pub aggregated_signature: Signature,
    pub merkle_root: U8_32,
    pub public_keys: Vec<PublicKey>,
}

pub struct MerkleTreeProof {
    pub proof: MerkleProof<Sha256Algorithm>,
    pub root: U8_32,
    pub transaction: SimpleTransaction,
    pub salt: U8_32,
    pub index: usize,
    pub total_leaves: usize,
}

impl MerkleTreeProof {
    pub fn verify(&self) -> bool {
        let txid = hash_tx_hash_with_salt(&self.transaction.clone().into(), &self.salt);

        self.proof
            .verify(self.root, &[self.index], &[txid], self.total_leaves)
    }
}
