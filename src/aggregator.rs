use std::collections::HashMap;

use anyhow::anyhow;
use bls_signatures::{aggregate, PublicKey, Signature};
use rs_merkle::{Hasher, MerkleProof, MerkleTree};
use sha2::{Digest, Sha256};

use crate::{
    errors::StatelessBitcoinResult,
    types::{generate_salt, MerkleTreeProof, TransferBlock, U8_32},
    utils::transaction::SimpleTransaction,
};

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

#[derive(Clone)]
pub struct TxMetadata {
    index: usize,
    public_key: PublicKey,
}

pub struct Aggregator {
    pub txid_to_index: HashMap<U8_32, TxMetadata>,
    pub merkle_tree: MerkleTree<Sha256Algorithm>,

    pub txid_to_signature: HashMap<U8_32, Signature>,
}

impl Aggregator {
    pub fn new() -> Aggregator {
        Aggregator {
            txid_to_index: HashMap::new(),
            merkle_tree: MerkleTree::new(),
            txid_to_signature: HashMap::new(),
        }
    }

    pub fn add_transaction(&mut self, transaction: &SimpleTransaction) {
        let index = self.merkle_tree.leaves_len();
        let tx_hash = transaction.tx_hash();
        self.txid_to_index.insert(
            tx_hash,
            TxMetadata {
                index,
                public_key: transaction.to,
            },
        );
        self.merkle_tree.insert(tx_hash).commit();
    }

    pub fn root(&self) -> StatelessBitcoinResult<U8_32> {
        self.merkle_tree.root().ok_or(anyhow!("No transactions"))
    }

    pub fn get_index_for_txid(&self, txid: U8_32) -> StatelessBitcoinResult<usize> {
        let TxMetadata { index, .. } = self
            .txid_to_index
            .get(&txid)
            .cloned()
            .ok_or(anyhow!("Transaction not found"))?;

        Ok(index)
    }

    pub fn get_merkle_proof_for_transaction(
        &self,
        transaction: &SimpleTransaction,
    ) -> StatelessBitcoinResult<MerkleTreeProof> {
        let tx_hash = transaction.tx_hash();
        let TxMetadata { index, .. } = self
            .txid_to_index
            .get(&tx_hash)
            .ok_or(anyhow!("Transaction not found"))?;

        let proof = self.merkle_tree.proof(&[*index]);

        let merkle_proof = MerkleTreeProof {
            proof,
            root: self.root()?,
            transaction: transaction.clone(),
            index: *index,
            total_leaves: self.merkle_tree.leaves_len(),
        };

        Ok(merkle_proof)
    }

    pub fn add_signature(&mut self, transaction: SimpleTransaction, signature: Signature) {
        // TODO: validate signature
        //
        self.txid_to_signature.insert(transaction.into(), signature);
    }

    pub fn produce_transfer_block(&self) -> StatelessBitcoinResult<TransferBlock> {
        let signatures = self
            .txid_to_signature
            .values()
            .cloned()
            .collect::<Vec<Signature>>();

        let public_keys = self
            .txid_to_signature
            .keys()
            .map(|txid| self.txid_to_index.get(txid).unwrap().public_key)
            .collect();

        let aggregated_signature = aggregate(&signatures.as_slice())?;

        let transfer_block = TransferBlock {
            aggregated_signature,
            merkle_root: self.root()?,
            public_keys,
        };

        Ok(transfer_block)
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        aggregator::Aggregator, client::Client, errors::StatelessBitcoinResult,
        types::MerkleTreeProof, utils::transaction::SimpleTransaction,
    };

    #[test]
    fn test_can_reproduce_root() -> StatelessBitcoinResult<()> {
        let mut aggregator = Aggregator::new();
        let mut bob = Client::new();

        let transactions = (0..10)
            .map(|i| {
                let (_, transaction) = bob.construct_transaction(bob.public_key, i * 100);
                transaction
            })
            .collect::<Vec<SimpleTransaction>>();

        for transaction in transactions.iter() {
            aggregator.add_transaction(transaction);
        }

        for transaction in transactions.iter() {
            let merkle_tree_proof = aggregator
                .get_merkle_proof_for_transaction(transaction)
                .unwrap();

            let verify_result = merkle_tree_proof.verify();

            assert_eq!(verify_result, true);
        }

        Ok(())
    }
}
