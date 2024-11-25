use bitcoincore_rpc::bitcoin::key::rand;
use bls_signatures::{PublicKey, Signature};
use rs_merkle::MerkleProof;

use crate::{aggregator::Sha256Algorithm, utils::transaction::SimpleTransaction};

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
    pub index: usize,
    pub total_leaves: usize,
}

impl MerkleTreeProof {
    pub fn verify(&self) -> bool {
        let tx_hash = self.transaction.tx_hash();

        self.proof
            .verify(self.root, &[self.index], &[tx_hash], self.total_leaves)
    }
}
