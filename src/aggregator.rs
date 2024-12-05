use anyhow::anyhow;
use indexmap::IndexMap;
use rs_merkle::{Hasher, MerkleTree};
use sha2::{Digest, Sha256};

use crate::{
    errors::CrateResult,
    types::{
        common::{generate_salt, TransferBlock, TransferBlockSignature, U8_32},
        public_key::BlsPublicKeyWrapper,
        signatures::{BlsPublicKey, BlsSignature},
        transaction::{TransactionBatch, TransactionProof},
    },
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
    batch: TransactionBatch,
    signature: Option<BlsSignature>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AggregatorState {
    Open,
    CollectSignatures,
    Finalised(TransferBlock),
}

pub struct Aggregator {
    pub tx_hash_to_metadata: IndexMap<BlsPublicKeyWrapper, TxMetadata>,
    pub merkle_tree: MerkleTree<Sha256Algorithm>,

    pub state: AggregatorState,
    pub salt: U8_32,
}

impl Aggregator {
    pub fn new() -> Aggregator {
        Aggregator {
            tx_hash_to_metadata: IndexMap::new(),
            merkle_tree: MerkleTree::new(),
            state: AggregatorState::Open,
            salt: generate_salt(),
        }
    }

    pub fn start_collecting_signatures(&mut self) -> CrateResult<()> {
        if self.tx_hash_to_metadata.is_empty() {
            return Err(anyhow!(
                "No transactions to start collecting signatures for"
            ));
        }

        self.check_aggregator_state(AggregatorState::Open)?;

        self.state = AggregatorState::CollectSignatures;

        Ok(())
    }

    pub fn add_batch(&mut self, batch: &TransactionBatch) -> CrateResult<()> {
        self.check_aggregator_state(AggregatorState::Open)?;

        let public_key_wrapper: BlsPublicKeyWrapper = batch.from.into();
        if self.tx_hash_to_metadata.contains_key(&public_key_wrapper) {
            return Err(anyhow!("Transaction already exists"));
        }

        let index = self.merkle_tree.leaves_len();

        self.tx_hash_to_metadata.insert(
            public_key_wrapper,
            TxMetadata {
                index,
                batch: batch.clone(),
                signature: None,
            },
        );
        self.merkle_tree.insert(batch.tx_hash()).commit();

        Ok(())
    }

    pub fn root(&self) -> CrateResult<U8_32> {
        self.merkle_tree.root().ok_or(anyhow!("No transactions"))
    }

    pub fn generate_proof_for_pubkey(
        &self,
        public_key: &BlsPublicKey,
    ) -> CrateResult<TransactionProof> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let public_key: BlsPublicKeyWrapper = public_key.into();

        let TxMetadata { index, batch, .. } = self.tx_hash_to_metadata.get(&public_key).ok_or(
            anyhow!("Transaction not found, when generating proof for batch"),
        )?;

        let proof = self.merkle_tree.proof(&[*index]);

        let merkle_proof = TransactionProof {
            proof_hashes: proof.proof_hashes().to_vec(),
            root: self.root()?,
            batch: batch.clone(),
            index: *index,
            total_leaves: self.merkle_tree.leaves_len(),
        };

        Ok(merkle_proof)
    }

    pub fn add_signature(
        &mut self,
        public_key: &BlsPublicKey,
        signature: &BlsSignature,
    ) -> CrateResult<()> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let public_key = *public_key;

        signature.verify(&public_key, self.root()?)?;

        let public_key_wrapper: BlsPublicKeyWrapper = public_key.into();
        let metadata = self
            .tx_hash_to_metadata
            .get_mut(&public_key_wrapper)
            .ok_or(anyhow!("Transaction not found, when adding signature"))?;

        metadata.signature = Some(*signature);

        Ok(())
    }

    pub fn finalise(&mut self) -> CrateResult<TransferBlock> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let mut signatures_and_public_keys: Vec<(BlsPublicKey, BlsSignature)> = vec![];

        for tx_metadata in self.tx_hash_to_metadata.values() {
            if let Some(signature) = tx_metadata.signature {
                signatures_and_public_keys.push((tx_metadata.batch.from.clone(), signature));
            }
        }

        if signatures_and_public_keys.is_empty() {
            return Err(anyhow!("No signatures"));
        }

        let signature = TransferBlockSignature::new(signatures_and_public_keys)?;

        let transfer_block = TransferBlock {
            signature,
            merkle_root: self.root()?,
        };

        self.state = AggregatorState::Finalised(transfer_block.clone());

        Ok(transfer_block)
    }

    fn check_aggregator_state(&self, expected_state: AggregatorState) -> CrateResult<()> {
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
        aggregator::{Aggregator, AggregatorState},
        errors::CrateResult,
        rollup::{mock_rollup_memory::MockRollupMemory, traits::MockRollupStateTrait},
        types::transaction::TransactionBatch,
        wallet::wallet::Wallet,
    };

    fn setup_with_unique_accounts_and_transactions(
        num_accounts: usize,
    ) -> CrateResult<(Aggregator, Vec<Wallet>, Vec<TransactionBatch>)> {
        let mut rollup_state = MockRollupMemory::new();
        let mut aggregator = Aggregator::new();
        let mut accounts = (0..num_accounts)
            .into_iter()
            .map(|_| Wallet::new())
            .collect::<Vec<Wallet>>();
        let receiver = Wallet::new();

        let batches = accounts
            .iter_mut()
            .map(|account| {
                rollup_state.add_deposit(account.public_key, 100).unwrap();
                account.sync_rollup_state(&rollup_state).unwrap();

                account
                    .append_transaction_to_batch(receiver.public_key, 100)
                    .unwrap();

                let batch = account.produce_batch().unwrap();

                aggregator.add_batch(&batch).unwrap();

                batch
            })
            .collect::<Vec<TransactionBatch>>();

        Ok((aggregator, accounts, batches))
    }

    #[test]
    fn test_can_setup_accounts_and_verify() -> CrateResult<()> {
        let (mut aggregator, _, batches) = setup_with_unique_accounts_and_transactions(10)?;

        aggregator.start_collecting_signatures()?;

        for transaction in batches.iter() {
            let merkle_tree_proof = aggregator
                .generate_proof_for_pubkey(&transaction.from)
                .unwrap();

            let verify_result = merkle_tree_proof.verify();

            assert_eq!(verify_result, true);
        }

        Ok(())
    }

    #[test]
    fn test_finalise() -> CrateResult<()> {
        let (mut aggregator, mut accounts, batches) =
            setup_with_unique_accounts_and_transactions(2)?;

        aggregator.start_collecting_signatures()?;

        for (transaction, account) in batches.iter().zip(accounts.iter_mut()) {
            let merkle_tree_proof = aggregator.generate_proof_for_pubkey(&transaction.from)?;

            let signature = account.validate_and_sign_proof(&merkle_tree_proof)?;

            aggregator.add_signature(&account.public_key, &signature)?;
        }

        let finalised_block = aggregator.finalise()?;

        assert_eq!(finalised_block.merkle_root, aggregator.root()?);
        assert_eq!(
            aggregator.state,
            AggregatorState::Finalised(finalised_block.clone())
        );

        let verified = &finalised_block.verify();

        match verified {
            Ok(_) => (),
            Err(e) => {
                assert!(
                    false,
                    "{}",
                    format!("Aggregated signature verification failed: {:?}", e)
                );
            }
        }

        Ok(())
    }
}
