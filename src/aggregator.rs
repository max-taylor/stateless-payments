use core::slice::SlicePattern;
use std::collections::HashMap;

use anyhow::anyhow;
use bls_signatures::{aggregate, PublicKey, Signature};
use rs_merkle::{Hasher, MerkleProof, MerkleTree};
use sha2::{Digest, Sha256};

use crate::{errors::StatelessBitcoinResult, types::U8_32, utils::transaction::SimpleTransaction};

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

pub struct TransferBlock {
    aggregated_signature: Signature,
    merkle_root: U8_32,
    public_keys: Vec<PublicKey>,
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

    pub fn add_transaction(&mut self, transaction: SimpleTransaction) {
        let index = self.merkle_tree.leaves_len();
        let txid: U8_32 = transaction.clone().into();
        self.txid_to_index.insert(
            txid,
            TxMetadata {
                index,
                public_key: transaction.to,
            },
        );
        self.merkle_tree.insert(txid).commit();
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

    pub fn get_merkle_proof_for_txid(
        &self,
        txid: U8_32,
    ) -> StatelessBitcoinResult<MerkleProof<Sha256Algorithm>> {
        let TxMetadata { index, .. } = self
            .txid_to_index
            .get(&txid)
            .ok_or(anyhow!("Transaction not found"))?;

        let proof = self.merkle_tree.proof(&[*index]);

        Ok(proof)
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
        aggregator::Aggregator,
        client::Client,
        errors::StatelessBitcoinResult,
        types::{generate_salt, U8_32},
    };

    #[test]
    fn test_can_reproduce_root() -> StatelessBitcoinResult<()> {
        let mut aggregator = Aggregator::new();
        let mut bob = Client::new();
        let salt = generate_salt();

        let txids: Vec<U8_32> = (0..10)
            .map(|i| {
                let (txid, _) = bob.construct_transaction(bob.public_key, i * 100, salt);
                txid
            })
            .collect();

        for txid in txids.iter() {
            aggregator.add_transaction(*txid);
        }

        let root = aggregator.root().unwrap();

        for (index, txid) in txids.iter().enumerate() {
            let proof = aggregator.get_merkle_proof_for_txid(*txid).unwrap();
            let verify_result = proof.verify(
                root.clone(),
                &[index],
                &[txid.clone()],
                aggregator.merkle_tree.leaves_len(),
            );

            assert_eq!(verify_result, true);
        }

        Ok(())
    }
}
