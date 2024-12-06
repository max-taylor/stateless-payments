use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use log::info;
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{
    errors::CrateResult,
    rollup::traits::RollupStateTrait,
    types::{
        balance::{BalanceProof, BalanceProofKey},
        common::generate_salt,
        signatures::{BlsPublicKey, BlsSecretKey, BlsSignature},
        transaction::{SimpleTransaction, TransactionBatch, TransactionProof},
    },
};

use super::utils::{calculate_balances_and_validate_balance_proof, merge_balance_proofs};

#[derive(Debug)]
pub struct Wallet {
    pub public_key: BlsPublicKey,
    private_key: BlsSecretKey,

    // Mapping of (Merkle Root, Sender pub key) -> TransactionProof
    pub balance_proof: BalanceProof,
    // Mapping of Transaction Hash -> Transaction
    // Use the tx_hash for lookups in this case
    // pub uncomfirmed_transactions: HashMap<U8_32, SimpleTransaction>,
    pub transaction_batch: TransactionBatch,
    batch_is_pending: bool,

    pub balance: u64,
}

impl Wallet {
    pub fn new(persist_path: Option<String>) -> Wallet {
        let private_key = BlsSecretKey::new();

        Wallet {
            private_key: private_key.clone(),
            public_key: private_key.public_key(),
            balance_proof: HashMap::new(),
            transaction_batch: TransactionBatch::new(private_key.public_key()),
            batch_is_pending: false,
            balance: 0,
        }
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

        Ok(())
    }

    // This is the function that the aggregator will call to get the signature
    // Internally we move the transaction batch to the balance proof because its been accepted
    // by the aggregator
    // TODO: This should likely move the batch to an uncomfirmed state and then get processed when
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
        // TODO: This should be moved to an uncomfirmed state
        self.transaction_batch = TransactionBatch::new(self.public_key);
        self.batch_is_pending = false;

        Ok(signature)
    }

    /// All the rollup related logic to keep the wallet in-sync with the rollup state
    pub async fn new_with_automatic_sync(
        perist_path: Option<String>,
        rollup_state: impl RollupStateTrait + Send + Sync + 'static,
        sync_rate_seconds: u64,
    ) -> CrateResult<(Arc<Mutex<Wallet>>, JoinHandle<CrateResult<()>>)> {
        let wallet = Arc::new(Mutex::new(Wallet::new(perist_path)));

        let spawn_handle =
            Wallet::spawn_automatic_sync_thread(wallet.clone(), rollup_state, sync_rate_seconds)
                .await?;

        Ok((wallet, spawn_handle))
    }

    async fn spawn_automatic_sync_thread(
        wallet: Arc<Mutex<Wallet>>,
        rollup_state: impl RollupStateTrait + Send + Sync + 'static,
        sync_rate_seconds: u64,
    ) -> CrateResult<JoinHandle<CrateResult<()>>> {
        wallet.lock().await.sync_rollup_state(&rollup_state).await?;

        #[derive(PartialEq, Eq)]
        struct SyncState {
            deposit_total: u64,
            withdraw_total: u64,
            total_transfer_blocks: u64,
        }

        async fn get_sync_state(
            rollup_state: &(impl RollupStateTrait + Send + Sync),
            public_key: &BlsPublicKey,
        ) -> CrateResult<SyncState> {
            Ok(SyncState {
                deposit_total: rollup_state.get_account_deposit_amount(public_key).await?,
                withdraw_total: rollup_state.get_account_withdraw_amount(public_key).await?,
                total_transfer_blocks: rollup_state
                    .get_account_transfer_blocks(public_key)
                    .await?
                    .len()
                    .try_into()?,
            })
        }

        let public_key = wallet.lock().await.public_key;

        let mut last_sync_state = get_sync_state(&rollup_state, &public_key).await?;

        Ok(tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(sync_rate_seconds)).await;

                let new_sync_state = get_sync_state(&rollup_state, &public_key).await?;

                if new_sync_state != last_sync_state {
                    let mut wallet = wallet.lock().await;
                    wallet.sync_rollup_state(&rollup_state).await?;
                }

                last_sync_state = new_sync_state;
            }
        }))
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
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use crate::{
        aggregator::Aggregator,
        errors::{CrateError, CrateResult},
        rollup::{
            mock_rollup_memory::MockRollupMemory,
            traits::{MockRollupStateTrait, RollupStateTrait},
        },
        wallet::constants::TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS,
        websocket::client::client::Client,
    };

    use super::Wallet;

    async fn setup(initial_deposit: u64) -> CrateResult<(Wallet, MockRollupMemory)> {
        let mut client = Wallet::new(None);
        let mut rollup_state = MockRollupMemory::new();
        rollup_state
            .add_deposit(client.public_key, initial_deposit)
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

    #[test]
    fn test_wallet_persisted() -> CrateResult<()> {
        let client = Wallet::new(Some("wallet.json".to_string()));
        let mut aggregator = Aggregator::new();

        Ok(())
    }

    const DEPOSIT: u64 = 100;
    const SLEEP_TIME_SECONDS: u64 = TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS + 1;

    async fn setup_auto_sync_tests(
    ) -> CrateResult<(Arc<Mutex<MockRollupMemory>>, Arc<Mutex<Wallet>>)> {
        let rollup_state = Arc::new(Mutex::new(MockRollupMemory::new()));

        let (client, _) = Wallet::new_with_automatic_sync(
            None,
            rollup_state.clone(),
            TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS,
        )
        .await?;

        let client_public_key = client.lock().await.public_key;

        // DEPOSIT

        rollup_state
            .lock()
            .await
            .add_deposit(client_public_key, DEPOSIT)
            .await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(SLEEP_TIME_SECONDS)).await;

        Ok((rollup_state, client))
    }

    #[tokio::test]
    async fn test_wallet_auto_syncs_deposits() -> CrateResult<()> {
        let (_, client) = setup_auto_sync_tests().await?;

        assert_eq!(client.lock().await.balance, DEPOSIT);

        Ok(())
    }

    #[tokio::test]
    async fn test_wallet_auto_syncs_withdraws() -> CrateResult<()> {
        let (rollup_state, client) = setup_auto_sync_tests().await?;

        let prev_balance = client.lock().await.balance;

        let withdraw = 50;

        rollup_state
            .lock()
            .await
            .add_withdraw(&client.lock().await.public_key, withdraw)
            .await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(SLEEP_TIME_SECONDS)).await;

        assert_eq!(client.lock().await.balance, prev_balance - withdraw);

        Ok(())
    }

    #[tokio::test]
    async fn test_wallet_auto_syncs_transfers() -> CrateResult<()> {
        let (rollup_state, client) = setup_auto_sync_tests().await?;

        // TRANSFER

        let (receiver, _) = Wallet::new_with_automatic_sync(
            None,
            rollup_state.clone(),
            TESTING_WALLET_AUTOMATIC_SYNC_RATE_SECONDS,
        )
        .await?;

        let mut aggregator = Aggregator::new();

        let transfer_amount = client.lock().await.balance;

        client
            .lock()
            .await
            .append_transaction_to_batch(receiver.lock().await.public_key, transfer_amount)?;

        let batch = client.lock().await.produce_batch()?;

        aggregator.add_batch(&batch)?;

        aggregator.start_collecting_signatures()?;

        let merkle_tree_proof = aggregator.generate_proof_for_pubkey(&batch.from)?;

        let signature = client
            .lock()
            .await
            .validate_and_sign_proof(&merkle_tree_proof)?;

        aggregator.add_signature(&client.lock().await.public_key, &signature)?;

        let transfer_block = aggregator.finalise()?;

        rollup_state
            .lock()
            .await
            .add_transfer_block(transfer_block)
            .await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(SLEEP_TIME_SECONDS)).await;

        assert_eq!(client.lock().await.balance, 0);

        receiver
            .lock()
            .await
            .add_receiving_transaction(
                &merkle_tree_proof,
                &client.lock().await.balance_proof,
                &rollup_state,
            )
            .await?;

        assert_eq!(receiver.lock().await.balance, transfer_amount);

        Ok(())
    }
}
