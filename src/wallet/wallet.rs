use std::{collections::HashMap, fs::OpenOptions};

use anyhow::anyhow;
use fs2::FileExt;
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::{from_reader, to_writer};

use crate::{
    errors::CrateResult,
    rollup::traits::RollupStateTrait,
    types::{
        balance::{BalanceProof, BalanceProofKey},
        common::generate_salt,
        signatures::{BlsPublicKey, BlsSecretKey, BlsSecretKeyWrapper, BlsSignature},
        transaction::{SimpleTransaction, TransactionBatch, TransactionProof},
    },
};

use super::utils::{calculate_balances_and_validate_balance_proof, merge_balance_proofs};

#[derive(Debug)]
pub struct Wallet {
    pub wallet_name: Option<String>,
    pub public_key: BlsPublicKey,
    private_key: BlsSecretKey,

    // Mapping of (Merkle Root, Sender pub key) -> TransactionProof
    pub balance_proof: BalanceProof,
    pub transaction_batch: TransactionBatch,
    batch_is_pending: bool,

    pub balance: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalletPersistState {
    pub balance_proof: BalanceProof,
    pub private_key: BlsSecretKeyWrapper,
}

impl Wallet {
    pub fn new(wallet_name: Option<String>) -> Wallet {
        let WalletPersistState {
            balance_proof,
            private_key,
        } = match wallet_name.clone() {
            Some(wallet_name) => Wallet::load_wallet_state(&wallet_name).unwrap(),
            None => WalletPersistState {
                balance_proof: HashMap::new(),
                private_key: BlsSecretKey::new().into(),
            },
        };

        let private_key: BlsSecretKey = private_key.into();

        Wallet {
            wallet_name,
            private_key: private_key.clone(),
            public_key: private_key.public_key(),
            balance_proof,
            transaction_batch: TransactionBatch::new(private_key.public_key()),
            batch_is_pending: false,
            balance: 0,
        }
    }

    fn save_wallet_state(&self) -> CrateResult<()> {
        if self.wallet_name.is_none() {
            return Ok(());
        }

        let wallet_name = self.wallet_name.as_ref().unwrap();

        let wallet_state = WalletPersistState {
            balance_proof: self.balance_proof.clone(),
            private_key: self.private_key.clone().into(),
        };

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(format!("/tmp/{}.json", wallet_name))?;

        file.lock_exclusive()?;

        to_writer(&file, &wallet_state)?;

        file.unlock()?;
        Ok(())
    }

    fn load_wallet_state(wallet_name: &str) -> CrateResult<WalletPersistState> {
        dbg!("Loading wallet state");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(format!("/tmp/{}.json", wallet_name))?;

        file.lock_exclusive()?;

        let state: WalletPersistState = match from_reader(&file) {
            Ok(state) => state,
            Err(e) => {
                println!("Error reading wallet state: {:?}", e);
                error!("Error reading wallet state: {:?}", e);
                WalletPersistState {
                    balance_proof: HashMap::new(),
                    private_key: BlsSecretKey::new().into(),
                }
            }
        };
        dbg!(&state);

        file.unlock().expect("Unable to unlock file");

        Ok(state)
    }

    /// Core logic of the wallet
    pub fn append_transaction_to_batch(
        &mut self,
        to: BlsPublicKey,
        amount: u64,
    ) -> CrateResult<&TransactionBatch> {
        info!("Appending transaction to batch");

        if self.batch_is_pending {
            return Err(anyhow!("Batch is currently pending"));
        }

        let salt = generate_salt();

        if to == self.public_key {
            return Err(anyhow!("Cannot send to self"));
        }

        if amount == 0 {
            return Err(anyhow!("Amount must be greater than 0"));
        }

        let transaction = SimpleTransaction {
            to,
            from: self.public_key,
            amount,
            salt,
        };

        self.balance = self
            .balance
            .checked_sub(amount)
            .ok_or_else(|| anyhow!("Insufficient balance"))?;

        self.transaction_batch
            .transactions
            .push(transaction.clone());

        Ok(&self.transaction_batch)
    }

    pub fn produce_batch(&mut self) -> CrateResult<TransactionBatch> {
        if self.transaction_batch.transactions.is_empty() {
            return Err(anyhow!("Transaction batch is empty"));
        }

        if self.batch_is_pending {
            return Err(anyhow!("Batch is already pending"));
        }

        self.batch_is_pending = true;

        Ok(self.transaction_batch.clone())
    }

    // Called when another client sends funds to this client
    //
    // TODO: This should validate that the rollup contract doesn't have any additional transactions
    // that weren't apart of the senders balance proof. If they do that means the sender may be trying
    // to double spend
    pub async fn add_receiving_transaction(
        &mut self,
        transaction_proof: &TransactionProof,
        senders_balance_proof: &BalanceProof,
        rollup_contract: &(impl RollupStateTrait + Send + Sync),
    ) -> CrateResult<()> {
        // Iterate over the batch and ensure one is addressed to this user
        if !transaction_proof
            .batch
            .transactions
            .iter()
            .any(|t| t.to == self.public_key)
        {
            return Err(anyhow!("No transaction addressed to this user"));
        }

        // This isn't really needed because validate_and_sign_transaction will be called first and
        // it checks this, but it's here for completeness
        if !transaction_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        if !senders_balance_proof.contains_key(&BalanceProofKey {
            root: transaction_proof.root,
            public_key: transaction_proof.batch.from.into(),
        }) {
            return Err(anyhow!(
                "Transaction not included in sender's balance proof"
            ));
        }

        let merged_proof =
            merge_balance_proofs(self.balance_proof.clone(), senders_balance_proof.clone())?;

        let balances =
            calculate_balances_and_validate_balance_proof(rollup_contract, &merged_proof).await?;

        let current_users_balance = balances.get(&self.public_key.into()).ok_or(anyhow!(
            "Current user's balance not found in merged balance proof"
        ))?;

        self.balance = *current_users_balance;
        self.balance_proof = merged_proof;
        self.save_wallet_state()?;

        Ok(())
    }

    // This is the function that the aggregator will call to get the signature
    // Internally we move the transaction batch to the balance proof because its been accepted
    // by the aggregator
    // the client is synced with the contract
    //
    // ! You could validate the inclusion by using the RollupContractTrait, but doesn't seem
    // necessary
    pub fn validate_and_sign_proof(
        &mut self,
        transaction_proof: &TransactionProof,
    ) -> CrateResult<BlsSignature> {
        if !self.batch_is_pending {
            return Err(anyhow!("No batch to sign"));
        }

        if self.transaction_batch.tx_hash() != transaction_proof.batch.tx_hash() {
            return Err(anyhow!("Provided proof doesn't match transaction batch"));
        }

        // Weird error that should never happen unless aggregator sends bad data
        if transaction_proof.batch.from != self.public_key {
            return Err(anyhow!("Transaction batch not from this user"));
        }

        if !transaction_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction proof"));
        }

        let signature = self.private_key.sign(
            blsful::SignatureSchemes::MessageAugmentation,
            &transaction_proof.root,
        )?;

        self.balance_proof.insert(
            BalanceProofKey {
                root: transaction_proof.root,
                public_key: self.public_key.into(),
            },
            transaction_proof.clone(),
        );

        self.transaction_batch = TransactionBatch::new(self.public_key);
        self.batch_is_pending = false;
        self.save_wallet_state()?;

        Ok(signature)
    }

    // This is called somewhat intermittently to ensure the client is in sync with the contract
    // It mainly ensures that the user's deposits and withdraws are accounted for
    pub async fn sync_rollup_state(
        &mut self,
        rollup_state: &(impl RollupStateTrait + Send + Sync),
    ) -> CrateResult<()> {
        let balances =
            calculate_balances_and_validate_balance_proof(rollup_state, &self.balance_proof)
                .await?;

        if let Some(current_users_balance) = balances.get(&self.public_key.into()) {
            self.balance = *current_users_balance;
        } else {
            let deposit_amount = rollup_state
                .get_account_deposit_amount(&self.public_key)
                .await?;
            let withdraw_amount = rollup_state
                .get_account_withdraw_amount(&self.public_key)
                .await?;

            self.balance = deposit_amount - withdraw_amount;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        aggregator::Aggregator,
        errors::{CrateError, CrateResult},
        rollup::{
            mock_rollup_memory::MockRollupMemory,
            traits::{MockRollupStateTrait, RollupStateTrait},
        },
    };

    use super::Wallet;

    async fn setup(initial_deposit: u64) -> CrateResult<(Wallet, MockRollupMemory)> {
        let mut client = Wallet::new(None);
        let mut rollup_state = MockRollupMemory::new();
        rollup_state
            .add_deposit(&client.public_key, initial_deposit)
            .await?;

        client.sync_rollup_state(&rollup_state).await?;

        Ok((client, rollup_state))
    }

    #[tokio::test]
    async fn test_balance_increases_with_deposits_when_syncing_rollup_state() -> CrateResult<()> {
        let (client, _) = setup(100).await?;

        assert_eq!(client.balance, 100);

        Ok(())
    }

    #[tokio::test]
    async fn test_balance_decreases_with_withdrawals_when_syncing_rollup_state() -> CrateResult<()>
    {
        let (mut client, mut rollup_state) = setup(100).await?;

        rollup_state.add_withdraw(&client.public_key, 50).await?;

        client.sync_rollup_state(&rollup_state).await?;

        assert_eq!(client.balance, 50);

        Ok(())
    }

    // TODO: leaving for now as these are nice to have tests
    #[test]
    fn test_sync_rollup_state_errors_when_rollup_state_has_transaction_not_in_transaction_history()
    {
        // Invalid transaction
    }

    #[test]
    fn test_sync_rollup_state_errors_when_client_has_transaction_not_in_rollup_state() {
        // Invalid transaction
    }

    #[tokio::test]
    async fn test_create_transaction_succeeds() -> CrateResult<()> {
        let (mut client, _) = setup(100).await?;

        let receiver = Wallet::new(None);

        let batch = client
            .append_transaction_to_batch(receiver.public_key, 100)?
            .clone();

        assert_eq!(batch.from, client.public_key);
        assert_eq!(batch.transactions.len(), 1);

        let transaction = &batch.transactions[0];
        assert_eq!(transaction.to, receiver.public_key);
        assert_eq!(transaction.amount, 100);
        assert_eq!(client.transaction_batch.transactions.len(), 1);
        assert_eq!(client.balance, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_transaction_fails_with_insufficient_balance() -> CrateResult<()> {
        let (mut client, _) = setup(100).await?;

        let receiver = Wallet::new(None);

        let transaction = client.append_transaction_to_batch(receiver.public_key, 101);

        assert!(transaction.is_err());
        assert_eq!(client.transaction_batch.transactions.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_and_sign_transaction_succeeds() -> CrateResult<()> {
        let (mut client, _) = setup(100).await?;
        let mut aggregator = Aggregator::new();
        let receiver = Wallet::new(None);
        client.append_transaction_to_batch(receiver.public_key, 100)?;
        let batch = client.produce_batch()?;

        aggregator.add_batch(&batch)?;
        aggregator.start_collecting_signatures()?;

        let merkle_tree_proof = aggregator.generate_proof_for_pubkey(&batch.from)?;

        let signature = client.validate_and_sign_proof(&merkle_tree_proof)?;

        assert_eq!(client.balance, 0);
        assert_eq!(client.transaction_batch.transactions.len(), 0);
        assert_eq!(client.balance_proof.len(), 1);

        let result = signature.verify(&client.public_key, &merkle_tree_proof.root);

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_adding_multiple_transactions_to_a_batch_succeeds() -> CrateResult<()> {
        let (mut client, _) = setup(300).await?;
        let alice = Wallet::new(None);
        let mary = Wallet::new(None);
        let bobs_uncle = Wallet::new(None);

        client.append_transaction_to_batch(alice.public_key, 100)?;
        client.append_transaction_to_batch(bobs_uncle.public_key, 100)?;
        let batch = client.append_transaction_to_batch(mary.public_key, 100)?;

        assert_eq!(batch.transactions.len(), 3);
        assert_eq!(client.transaction_batch.transactions.len(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_add_receiving_transaction_succeeds() -> CrateResult<()> {
        let mut aggregator = Aggregator::new();
        let (mut client, mut rollup_state) = setup(300).await?;
        let mut alice = Wallet::new(None);

        client.append_transaction_to_batch(alice.public_key, 100)?;
        let batch = client.produce_batch()?;

        aggregator.add_batch(&batch)?;
        aggregator.start_collecting_signatures()?;
        let merkle_tree_proof = aggregator.generate_proof_for_pubkey(&batch.from)?;

        let signature = client.validate_and_sign_proof(&merkle_tree_proof)?;

        aggregator.add_signature(&client.public_key, &signature)?;

        let transfer_block = aggregator.finalise()?;

        rollup_state.add_transfer_block(transfer_block).await?;

        alice
            .add_receiving_transaction(&merkle_tree_proof, &client.balance_proof, &rollup_state)
            .await?;

        assert_eq!(alice.balance, 100);
        assert_eq!(client.balance, 200);

        Ok(())
    }

    async fn complete_aggregator_round(
        sender: &mut Wallet,
        rollup_state: &mut MockRollupMemory,
        amount: u64,
    ) -> CrateResult<Wallet> {
        let mut aggregator = Aggregator::new();

        let mut receiver = Wallet::new(None);

        sender.append_transaction_to_batch(receiver.public_key, amount)?;
        let batch = sender.produce_batch()?;

        aggregator.add_batch(&batch)?;
        aggregator.start_collecting_signatures()?;
        let merkle_tree_proof = aggregator.generate_proof_for_pubkey(&batch.from)?;
        let signature = sender.validate_and_sign_proof(&merkle_tree_proof)?;

        aggregator.add_signature(&sender.public_key, &signature)?;

        let transfer_block = aggregator.finalise()?;

        rollup_state.add_transfer_block(transfer_block).await?;

        receiver
            .add_receiving_transaction(&merkle_tree_proof, &sender.balance_proof, rollup_state)
            .await?;

        Ok(receiver)
    }

    #[tokio::test]
    async fn test_long_chain_of_transactions_still_can_be_spent() -> CrateResult<()> {
        let amount = 100;

        let (client, mut rollup_state) = setup(amount).await?;

        let mut next_sender = client;

        for _ in 0..10 {
            let receiver =
                complete_aggregator_round(&mut next_sender, &mut rollup_state, amount).await?;

            assert_eq!(receiver.balance, amount);
            assert_eq!(next_sender.balance, 0);

            next_sender = receiver;
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_add_receiving_transaction_fails_when_transaction_not_in_rollup_state(
    ) -> CrateResult<()> {
        let amount = 100;

        let (mut client, rollup_state) = setup(amount).await?;

        let mut aggregator = Aggregator::new();

        let mut receiver = Wallet::new(None);

        client.append_transaction_to_batch(receiver.public_key, amount)?;

        let batch = client.produce_batch()?;

        aggregator.add_batch(&batch)?;
        aggregator.start_collecting_signatures()?;
        let merkle_tree_proof = aggregator.generate_proof_for_pubkey(&batch.from)?;
        let signature = client.validate_and_sign_proof(&merkle_tree_proof)?;

        aggregator.add_signature(&client.public_key, &signature)?;

        // Produce the transfer block, but don't add it to the rollup state
        aggregator.finalise()?;

        let value = receiver
            .add_receiving_transaction(&merkle_tree_proof, &client.balance_proof, &rollup_state)
            .await;

        match value {
            Err(err) => {
                // Downcast the anyhow::Error to your custom error
                let custom_error = err.downcast_ref::<CrateError>();
                assert_eq!(
                    custom_error,
                    Some(&CrateError::BatchNotInATransferBlock(batch.clone()))
                );
            }
            _ => assert!(false, "Expected an error"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_wallet_persisted() -> CrateResult<()> {
        let mut rollup_state = MockRollupMemory::new();
        let wallet_name = "1".to_string();
        let mut client = Wallet::new(Some(wallet_name.clone()));
        rollup_state.add_deposit(&client.public_key, 100).await?;
        client.sync_rollup_state(&rollup_state).await?;

        let receiver = Wallet::new(None);
        let mut aggregator = Aggregator::new();

        client.append_transaction_to_batch(receiver.public_key, 100)?;
        let batch = client.produce_batch()?;

        aggregator.add_batch(&batch)?;

        aggregator.start_collecting_signatures()?;

        let merkle_tree_proof = aggregator.generate_proof_for_pubkey(&batch.from)?;

        // Moves the transaction batch to the balance proof
        client.validate_and_sign_proof(&merkle_tree_proof)?;

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let loaded_wallet = Wallet::new(Some(wallet_name));

        dbg!(&loaded_wallet);

        assert_eq!(client.balance_proof, loaded_wallet.balance_proof);

        // Delete the file if it exists to prevent issues with the test
        std::fs::remove_file("/tmp/1.json").ok();

        Ok(())
    }
}
