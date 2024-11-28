use anyhow::anyhow;
use blsful::AggregateSignature;
use indexmap::IndexMap;
use rs_merkle::{Hasher, MerkleTree};
use sha2::{Digest, Sha256};

use crate::{
    errors::StatelessBitcoinResult,
    types::{
        common::{
            generate_salt, BlsPublicKey, BlsSignature, TransactionProof, TransferBlock, U8_32,
        },
        public_key::BlsPublicKeyWrapper,
        transaction::TransactionBatch,
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
    public_key: BlsPublicKey,
    signature: Option<BlsSignature>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AggregatorState {
    Open,
    CollectSignatures,
    Finalised(TransferBlock),
}

pub struct Aggregator {
    pub tx_hash_to_metadata: IndexMap<(U8_32, BlsPublicKeyWrapper), TxMetadata>,
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

    pub fn start_collecting_signatures(&mut self) -> StatelessBitcoinResult<()> {
        self.check_aggregator_state(AggregatorState::Open)?;

        self.state = AggregatorState::CollectSignatures;

        Ok(())
    }

    pub fn add_transaction(
        &mut self,
        tx_hash: &U8_32,
        public_key: &BlsPublicKey,
    ) -> StatelessBitcoinResult<()> {
        self.check_aggregator_state(AggregatorState::Open)?;

        let tx_hash = *tx_hash;
        let public_key = *public_key;

        if self
            .tx_hash_to_metadata
            .contains_key(&(tx_hash, public_key.into()))
        {
            return Err(anyhow!("Transaction already exists"));
        }

        let index = self.merkle_tree.leaves_len();

        self.tx_hash_to_metadata.insert(
            (tx_hash, public_key.into()),
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

    pub fn generate_proof_for_batch(
        &self,
        batch: &TransactionBatch,
        public_key: &BlsPublicKey,
    ) -> StatelessBitcoinResult<TransactionProof> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let public_key = *public_key;

        let TxMetadata { index, .. } = self
            .tx_hash_to_metadata
            .get(&(batch.tx_hash(), public_key.into()))
            .ok_or(anyhow!("Transaction not found"))?;

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
        tx_hash: &U8_32,
        public_key: &BlsPublicKey,
        signature: BlsSignature,
    ) -> StatelessBitcoinResult<()> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let tx_hash = *tx_hash;
        let public_key = *public_key;

        signature.verify(&public_key, self.root()?)?;

        let metadata = self
            .tx_hash_to_metadata
            .get_mut(&(tx_hash, public_key.into()))
            .ok_or(anyhow!("Transaction not found"))?;

        metadata.signature = Some(signature);

        Ok(())
    }

    pub fn finalise(&mut self) -> StatelessBitcoinResult<TransferBlock> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let mut signatures: Vec<BlsSignature> = vec![];
        let mut public_keys: Vec<BlsPublicKey> = vec![];

        for tx_metadata in self.tx_hash_to_metadata.values() {
            if let Some(signature) = tx_metadata.signature {
                signatures.push(signature);
                public_keys.push(tx_metadata.public_key.clone());
            }
        }

        let aggregated_signature = AggregateSignature::from_signatures(&signatures)?;

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
        aggregator::{Aggregator, AggregatorState},
        client::client::Client,
        errors::StatelessBitcoinResult,
        rollup::rollup_state::MockRollupState,
        types::transaction::TransactionBatch,
    };

    fn setup_with_unique_accounts_and_transactions(
        num_accounts: usize,
    ) -> StatelessBitcoinResult<(Aggregator, Vec<Client>, Vec<TransactionBatch>)> {
        let mut rollup_state = MockRollupState::new();
        let mut aggregator = Aggregator::new();
        let mut accounts = (0..num_accounts)
            .into_iter()
            .map(|_| Client::new())
            .collect::<Vec<Client>>();
        let receiver = Client::new();

        let transactions = accounts
            .iter_mut()
            .map(|account| {
                rollup_state.add_deposit(account.public_key, 100);

                let tx = account
                    .append_transaction_to_batch(receiver.public_key, 100)
                    .unwrap()
                    .clone();
                aggregator
                    .add_transaction(&tx.tx_hash(), &account.public_key)
                    .unwrap();

                tx
            })
            .collect::<Vec<TransactionBatch>>();

        Ok((aggregator, accounts, transactions))
    }

    #[test]
    fn test_can_setup_accounts_and_verify() -> StatelessBitcoinResult<()> {
        let (mut aggregator, accounts, transactions) =
            setup_with_unique_accounts_and_transactions(10)?;

        aggregator.start_collecting_signatures()?;

        for (transaction, account) in transactions.iter().zip(accounts.iter()) {
            let merkle_tree_proof = aggregator
                .generate_proof_for_batch(&transaction, &account.public_key)
                .unwrap();

            let verify_result = merkle_tree_proof.verify();

            assert_eq!(verify_result, true);
        }

        Ok(())
    }

    #[test]
    fn test_finalise() -> StatelessBitcoinResult<()> {
        let (mut aggregator, mut accounts, transactions) =
            setup_with_unique_accounts_and_transactions(2)?;

        aggregator.start_collecting_signatures()?;

        for (transaction, account) in transactions.iter().zip(accounts.iter_mut()) {
            let merkle_tree_proof =
                aggregator.generate_proof_for_batch(&transaction, &account.public_key)?;

            let signature = account.validate_and_sign_batch(&merkle_tree_proof)?;

            aggregator.add_signature(&transaction.tx_hash(), &account.public_key, signature)?;
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
