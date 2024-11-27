use std::collections::HashMap;

use anyhow::anyhow;
use bls_signatures::{aggregate, PublicKey, Signature};
use rs_merkle::{Hasher, MerkleTree};
use sha2::{Digest, Sha256};

use crate::{
    errors::StatelessBitcoinResult,
    types::common::{TransactionProof, TransferBlock, U8_32},
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
    signature: Option<Signature>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AggregatorState {
    Open,
    CollectSignatures,
    Finalised(TransferBlock),
}

pub struct Aggregator {
    pub tx_hash_to_metadata: HashMap<U8_32, TxMetadata>,
    pub merkle_tree: MerkleTree<Sha256Algorithm>,

    pub state: AggregatorState,
}

impl Aggregator {
    pub fn new() -> Aggregator {
        Aggregator {
            tx_hash_to_metadata: HashMap::new(),
            merkle_tree: MerkleTree::new(),
            state: AggregatorState::Open,
        }
    }

    pub fn start_collecting_signatures(&mut self) -> StatelessBitcoinResult<()> {
        self.check_aggregator_state(AggregatorState::Open)?;

        self.state = AggregatorState::CollectSignatures;

        Ok(())
    }

    pub fn add_transaction(
        &mut self,
        tx_hash: U8_32,
        public_key: PublicKey,
    ) -> StatelessBitcoinResult<()> {
        self.check_aggregator_state(AggregatorState::Open)?;

        let index = self.merkle_tree.leaves_len();

        self.tx_hash_to_metadata.insert(
            tx_hash,
            TxMetadata {
                index,
                public_key,
                signature: None,
            },
        );
        self.merkle_tree.insert(tx_hash).commit();

        Ok(())
    }

    pub fn root(&self) -> StatelessBitcoinResult<U8_32> {
        self.merkle_tree.root().ok_or(anyhow!("No transactions"))
    }

    pub fn generate_proof_for_tx_hash(
        &self,
        tx_hash: U8_32,
    ) -> StatelessBitcoinResult<TransactionProof> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let TxMetadata { index, .. } = self
            .tx_hash_to_metadata
            .get(&tx_hash)
            .ok_or(anyhow!("Transaction not found"))?;

        let proof = self.merkle_tree.proof(&[*index]);

        let merkle_proof = TransactionProof {
            proof_hashes: proof.proof_hashes().to_vec(),
            root: self.root()?,
            tx_hash,
            index: *index,
            total_leaves: self.merkle_tree.leaves_len(),
        };

        Ok(merkle_proof)
    }

    pub fn add_signature(
        &mut self,
        transaction: SimpleTransaction,
        signature: Signature,
    ) -> StatelessBitcoinResult<()> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        // TODO: validate signature

        let metadata = self
            .tx_hash_to_metadata
            .get_mut(&transaction.tx_hash())
            .ok_or(anyhow!("Transaction not found"))?;

        metadata.signature = Some(signature);

        Ok(())
    }

    pub fn finalise(&mut self) -> StatelessBitcoinResult<TransferBlock> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let mut signatures: Vec<Signature> = vec![];
        let mut public_keys: Vec<PublicKey> = vec![];

        for tx_metadata in self.tx_hash_to_metadata.values() {
            if let Some(signature) = tx_metadata.signature {
                signatures.push(signature);
                public_keys.push(tx_metadata.public_key.clone());
            }
        }

        let aggregated_signature = aggregate(&signatures.as_slice())?;

        let transfer_block = TransferBlock {
            aggregated_signature,
            merkle_root: self.root()?,
            public_keys,
        };

        self.state = AggregatorState::Finalised(transfer_block.clone());

        Ok(transfer_block)
    }

    fn check_aggregator_state(
        &self,
        expected_state: AggregatorState,
    ) -> StatelessBitcoinResult<()> {
        if self.state != expected_state {
            return Err(anyhow!(
                "Invalid state, is {:?} but expected {:?}",
                self.state,
                expected_state
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        aggregator::Aggregator, client::Client, errors::StatelessBitcoinResult,
        utils::transaction::SimpleTransaction,
    };

    #[test]
    fn test_can_reproduce_root() -> StatelessBitcoinResult<()> {
        let mut aggregator = Aggregator::new();
        let mut bob = Client::new();

        let transactions = (0..10)
            .map(|i| {
                let transaction = bob.create_transaction(bob.public_key, i * 100).unwrap();
                transaction
            })
            .collect::<Vec<SimpleTransaction>>();

        for transaction in transactions.iter() {
            aggregator.add_transaction(transaction.tx_hash(), bob.public_key.clone())?;
        }

        aggregator.start_collecting_signatures()?;

        for transaction in transactions.iter() {
            let merkle_tree_proof = aggregator
                .generate_proof_for_tx_hash(transaction.tx_hash())
                .unwrap();

            let verify_result = merkle_tree_proof.verify();

            assert_eq!(verify_result, true);
        }

        Ok(())
    }

    #[test]
    fn test_can_finalise_and_produce_valid_transfer_block() -> StatelessBitcoinResult<()> {
        let mut aggregator = Aggregator::new();
        let mut bob = Client::new();
        let alice = Client::new();
        let mary = Client::new();

        let (_, to_alice_tx) = bob.construct_transaction(alice.public_key, 100)?;
        let (_, to_mary_tx) = bob.construct_transaction(mary.public_key, 100)?;

        aggregator.add_transaction(&to_alice_tx)?;
        aggregator.add_transaction(&to_mary_tx)?;

        Ok(())
    }
}
