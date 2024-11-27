use std::collections::HashMap;

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

    pub fn generate_proof_for_tx_hash(
        &self,
        tx_hash: &U8_32,
        public_key: &BlsPublicKey,
    ) -> StatelessBitcoinResult<TransactionProof> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let tx_hash = *tx_hash;
        let public_key = *public_key;

        let TxMetadata { index, .. } = self
            .tx_hash_to_metadata
            .get(&(tx_hash, public_key.into()))
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
        tx_hash: &U8_32,
        public_key: &BlsPublicKey,
        signature: BlsSignature,
    ) -> StatelessBitcoinResult<()> {
        self.check_aggregator_state(AggregatorState::CollectSignatures)?;

        let tx_hash = *tx_hash;
        let public_key = *public_key;

        // if !verify_messages(&signature, &[self.root()?.as_ref()], &[public_key]) {
        //     return Err(anyhow!("Invalid signature"));
        // }

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

    use blsful::{AggregateSignature, Bls12381G1Impl, BlsSignatureImpl, SecretKey};

    use crate::{
        aggregator::{Aggregator, AggregatorState},
        client::Client,
        errors::StatelessBitcoinResult,
        types::{common::generate_salt, transaction::SimpleTransaction},
    };

    fn setup_with_unique_accounts_and_transactions(
        num_accounts: usize,
    ) -> StatelessBitcoinResult<(Aggregator, Vec<Client>, Vec<SimpleTransaction>)> {
        let mut aggregator = Aggregator::new();
        let mut accounts = (0..num_accounts)
            .into_iter()
            .map(|_| Client::new())
            .collect::<Vec<Client>>();

        let transactions = accounts
            .iter_mut()
            .map(|account| {
                let tx = account.create_transaction(account.public_key, 100).unwrap();
                aggregator
                    .add_transaction(&tx.tx_hash(), &account.public_key)
                    .unwrap();

                tx
            })
            .collect::<Vec<SimpleTransaction>>();

        Ok((aggregator, accounts, transactions))
    }

    #[test]
    fn test_can_setup_accounts_and_verify() -> StatelessBitcoinResult<()> {
        let (mut aggregator, accounts, transactions) =
            setup_with_unique_accounts_and_transactions(10)?;

        aggregator.start_collecting_signatures()?;

        for (transaction, account) in transactions.iter().zip(accounts.iter()) {
            let merkle_tree_proof = aggregator
                .generate_proof_for_tx_hash(&transaction.tx_hash(), &account.public_key)
                .unwrap();

            let verify_result = merkle_tree_proof.verify();

            assert_eq!(verify_result, true);
        }

        Ok(())
    }

    #[test]
    fn test_stupid_fucking_thing() -> StatelessBitcoinResult<()> {
        let bob: SecretKey<Bls12381G1Impl> = SecretKey::new();
        let alice = SecretKey::new();
        // let bob = PrivateKey::generate(&mut thread_rng());
        // let alice = PrivateKey::generate(&mut thread_rng());

        let message = "hello world";
        let root = message.as_bytes();

        let mut message = Vec::new();
        message.extend_from_slice("Hello, world!".as_bytes());
        message.extend_from_slice(&generate_salt());

        // Sign the message with both keys
        let bob_sig = bob.sign(blsful::SignatureSchemes::MessageAugmentation, root)?;
        let alice_sig = alice.sign(blsful::SignatureSchemes::MessageAugmentation, root)?;

        // // Verify individual signatures
        // assert!(
        //     verify_messages(&bob_sig, &[message.as_ref()], &[bob.public_key()]),
        //     "Bob's signature failed to verify"
        // );
        // assert!(
        //     verify_messages(&alice_sig, &[message.as_ref()], &[alice.public_key()]),
        //     "Alice's signature failed to verify"
        // );

        // Aggregate signatures
        // let aggregated_signature = aggregate(&[bob_sig, alice_sig])?;
        let aggregated_signature = AggregateSignature::from_signatures(&[bob_sig, alice_sig])?;

        let verified =
            aggregated_signature.verify(&[(bob.public_key(), root), (alice.public_key(), root)])?;

        dbg!(&verified);

        // // Verify the aggregated signature
        // let message_hash = hash(&message);
        // let messages = vec![message_hash, message_hash]; // Same hash for both
        // let public_keys = vec![bob.public_key(), alice.public_key()];
        // let verified = verify(&aggregated_signature, &messages, &public_keys);

        // assert!(verified, "Aggregated signature verification failed");

        Ok(())
    }

    // #[test]
    // fn test_stupid_fucking_thing() -> StatelessBitcoinResult<()> {
    //     let mut aggregator = Aggregator::new();
    //     let bob = PrivateKey::generate(&mut thread_rng());
    //     let alice = PrivateKey::generate(&mut thread_rng());
    //
    //     // let mut bob = Client::new();
    //     // let mut alice = Client::new();
    //     //
    //     // let bob_tx = bob.create_transaction(alice.public_key, 100)?;
    //     // let alice_tx = alice.create_transaction(bob.public_key, 100)?;
    //
    //     let message = "hello world";
    //
    //     // aggregator.add_transaction(&bob_tx.tx_hash(), &bob.public_key)?;
    //     // aggregator.add_transaction(&alice_tx.tx_hash(), &alice.public_key)?;
    //
    //     aggregator.start_collecting_signatures()?;
    //
    //     // let root = aggregator.root()?;
    //     let root = message.to_string().into_bytes();
    //
    //     let bob_sig = bob.sign(&root);
    //     let alice_sig = alice.sign(&root);
    //
    //     // let bob_sig = bob.validate_and_sign_transaction(
    //     //     aggregator.generate_proof_for_tx_hash(&bob_tx.tx_hash(), &bob.public_key)?,
    //     // )?;
    //     //
    //     // let alice_sig = alice.validate_and_sign_transaction(
    //     //     aggregator.generate_proof_for_tx_hash(&alice_tx.tx_hash(), &alice.public_key)?,
    //     // )?;
    //
    //     let aggregated_signature = aggregate(&[bob_sig, alice_sig])?;
    //
    //     let messages = vec![root.as_ref(), root.as_ref()];
    //
    //     let public_keys = vec![bob.public_key(), alice.public_key()];
    //
    //     let verified = verify_messages(&aggregated_signature, &messages, &public_keys);
    //
    //     assert_eq!(verified, true, "Aggregated signature verification failed");
    //
    //     Ok(())
    // }
    //
    // #[test]
    // fn test_finalise() -> StatelessBitcoinResult<()> {
    //     let (mut aggregator, accounts, transactions) =
    //         setup_with_unique_accounts_and_transactions(2)?;
    //
    //     aggregator.start_collecting_signatures()?;
    //
    //     let root = aggregator.root()?;
    //
    //     let mut signatures: Vec<Signature> = vec![];
    //
    //     for (transaction, account) in transactions.iter().zip(accounts.iter()) {
    //         let merkle_tree_proof = aggregator
    //             .generate_proof_for_tx_hash(&transaction.tx_hash(), &account.public_key)?;
    //
    //         assert_eq!(merkle_tree_proof.verify(), true);
    //         assert_eq!(merkle_tree_proof.root, root);
    //
    //         let signature = account.validate_and_sign_transaction(merkle_tree_proof)?;
    //
    //         signatures.push(signature);
    //
    //         // aggregator.add_signature(&transaction.tx_hash(), &account.public_key, signature)?;
    //     }
    //
    //     let aggregated_signature = aggregate(&signatures.as_slice())?;
    //
    //     let messages = (0..signatures.len())
    //         .map(|i| &root as &[u8])
    //         .collect::<Vec<_>>();
    //
    //     let public_keys = accounts
    //         .iter()
    //         .map(|account| account.public_key)
    //         .collect::<Vec<_>>();
    //
    //     let verified = verify_messages(&aggregated_signature, &messages, &public_keys);
    //     assert!(verified, "Aggregated signature verification failed");
    //
    //     Ok(())
    //     // let finalised_block = aggregator.finalise()?;
    //     //
    //     // // assert_eq!(finalised_block.public_keys.len(), 10);
    //     // assert_eq!(finalised_block.merkle_root, aggregator.root()?);
    //     // assert_eq!(
    //     //     aggregator.state,
    //     //     AggregatorState::Finalised(finalised_block.clone())
    //     // );
    //     // dbg!(&finalised_block);
    //     //
    //     // let messages = (0..finalised_block.public_keys.len())
    //     //     .map(|i| &finalised_block.merkle_root as &[u8])
    //     //     .collect::<Vec<_>>();
    //     //
    //     // let verified = verify_messages(
    //     //     &finalised_block.aggregated_signature,
    //     //     // &[&finalised_block.merkle_root],
    //     //     messages.as_slice(),
    //     //     &finalised_block.public_keys,
    //     // );
    //     //
    //     // assert_eq!(verified, true, "Aggregated signature is invalid");
    //     //
    //     // Ok(())
    // }
}
