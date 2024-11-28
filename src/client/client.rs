use std::collections::HashMap;

use anyhow::anyhow;

use crate::{
    errors::StatelessBitcoinResult,
    rollup::rollup_state::RollupStateTrait,
    types::{
        common::{
            generate_salt, BalanceProof, BlsPublicKey, BlsSecretKey, BlsSignature, TransactionProof,
        },
        transaction::{SimpleTransaction, TransactionBatch},
    },
};

use super::utils::{calculate_balances_and_validate_balance_proof, merge_balance_proofs};

pub struct Client {
    pub public_key: BlsPublicKey,
    private_key: BlsSecretKey,

    // Mapping of (Merkle Root, Sender pub key) -> TransactionProof
    pub balance_proof: BalanceProof,
    // Mapping of Transaction Hash -> Transaction
    // Use the tx_hash for lookups in this case
    // pub uncomfirmed_transactions: HashMap<U8_32, SimpleTransaction>,
    pub transaction_batch: TransactionBatch,

    pub balance: u64,
}

impl Client {
    pub fn new() -> Client {
        let private_key = BlsSecretKey::new();

        Client {
            private_key: private_key.clone(),
            public_key: private_key.public_key(),
            balance_proof: HashMap::new(),
            transaction_batch: TransactionBatch::new(private_key.public_key()),
            // uncomfirmed_transactions: HashMap::new(),
            balance: 0,
        }
    }

    // This is called somewhat intermittently to ensure the client is in sync with the contract
    // It mainly ensures that the user's deposits and withdraws are accounted for
    pub fn sync_rollup_state(
        &mut self,
        rollup_state: &impl RollupStateTrait,
    ) -> StatelessBitcoinResult<()> {
        let balances =
            calculate_balances_and_validate_balance_proof(rollup_state, &self.balance_proof)?;

        if let Some(current_users_balance) = balances.get(&self.public_key.into()) {
            self.balance = *current_users_balance;
        } else {
            let deposit_amount = rollup_state.get_account_deposit_amount(&self.public_key)?;
            let withdraw_amount = rollup_state.get_account_withdraw_amount(&self.public_key)?;

            self.balance = deposit_amount - withdraw_amount;
        }

        Ok(())
    }

    pub fn append_transaction_to_batch(
        &mut self,
        to: BlsPublicKey,
        amount: u64,
    ) -> StatelessBitcoinResult<&TransactionBatch> {
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

    // Called when another client sends funds to this client
    pub fn add_receiving_transaction(
        &mut self,
        transaction_proof: &TransactionProof,
        senders_balance_proof: &BalanceProof,
        rollup_contract: &impl RollupStateTrait,
    ) -> StatelessBitcoinResult<()> {
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

        if !senders_balance_proof
            .contains_key(&(transaction_proof.root, transaction_proof.batch.from.into()))
        {
            return Err(anyhow!(
                "Transaction not included in sender's balance proof"
            ));
        }

        let merged_proof =
            merge_balance_proofs(self.balance_proof.clone(), senders_balance_proof.clone())?;

        let balances =
            calculate_balances_and_validate_balance_proof(rollup_contract, &merged_proof)?;

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
    pub fn validate_and_sign_batch(
        &mut self,
        transaction_proof: &TransactionProof,
    ) -> StatelessBitcoinResult<BlsSignature> {
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
            (transaction_proof.root, self.public_key.into()),
            transaction_proof.clone(),
        );

        self.transaction_batch = TransactionBatch::new(self.public_key);

        Ok(signature)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        aggregator::Aggregator,
        errors::{StatelessBitcoinError, StatelessBitcoinResult},
        rollup::rollup_state::MockRollupState,
    };

    use super::Client;

    fn setup(initial_deposit: u64) -> StatelessBitcoinResult<(Client, MockRollupState)> {
        let mut client = Client::new();
        let mut rollup_state = MockRollupState::new();
        rollup_state.add_deposit(client.public_key, initial_deposit);

        client.sync_rollup_state(&rollup_state).unwrap();

        Ok((client, rollup_state))
    }

    #[test]
    fn test_balance_increases_with_deposits_when_syncing_rollup_state() -> StatelessBitcoinResult<()>
    {
        let (client, _) = setup(100)?;

        assert_eq!(client.balance, 100);

        Ok(())
    }

    #[test]
    fn test_balance_decreases_with_withdrawals_when_syncing_rollup_state(
    ) -> StatelessBitcoinResult<()> {
        let (mut client, mut rollup_state) = setup(100)?;

        rollup_state.add_withdraw(&client.public_key, 50)?;

        client.sync_rollup_state(&rollup_state)?;

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

    #[test]
    fn test_create_transaction_succeeds() -> StatelessBitcoinResult<()> {
        let (mut client, _) = setup(100)?;

        let receiver = Client::new();

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

    #[test]
    fn test_create_transaction_fails_with_insufficient_balance() -> StatelessBitcoinResult<()> {
        let (mut client, _) = setup(100)?;

        let receiver = Client::new();

        let transaction = client.append_transaction_to_batch(receiver.public_key, 101);

        assert!(transaction.is_err());
        assert_eq!(client.transaction_batch.transactions.len(), 0);

        Ok(())
    }

    #[test]
    fn test_validate_and_sign_transaction_succeeds() -> StatelessBitcoinResult<()> {
        let (mut client, _) = setup(100)?;
        let mut aggregator = Aggregator::new();
        let receiver = Client::new();
        let batch = client
            .append_transaction_to_batch(receiver.public_key, 100)?
            .clone();

        aggregator.add_batch(&batch.tx_hash(), &client.public_key)?;
        aggregator.start_collecting_signatures()?;

        let merkle_tree_proof = aggregator.generate_proof_for_batch(&batch)?;

        let signature = client.validate_and_sign_batch(&merkle_tree_proof)?;

        assert_eq!(client.balance, 0);
        assert_eq!(client.transaction_batch.transactions.len(), 0);
        assert_eq!(client.balance_proof.len(), 1);

        let result = signature.verify(&client.public_key, &merkle_tree_proof.root);

        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_adding_multiple_transactions_to_a_batch_succeeds() -> StatelessBitcoinResult<()> {
        let (mut client, _) = setup(300)?;
        let alice = Client::new();
        let mary = Client::new();
        let bobs_uncle = Client::new();

        client.append_transaction_to_batch(alice.public_key, 100)?;
        client.append_transaction_to_batch(bobs_uncle.public_key, 100)?;
        let batch = client.append_transaction_to_batch(mary.public_key, 100)?;

        assert_eq!(batch.transactions.len(), 3);
        assert_eq!(client.transaction_batch.transactions.len(), 3);

        Ok(())
    }

    #[test]
    fn test_add_receiving_transaction_succeeds() -> StatelessBitcoinResult<()> {
        let mut aggregator = Aggregator::new();
        let (mut client, mut rollup_state) = setup(300)?;
        let mut alice = Client::new();

        let batch = client
            .append_transaction_to_batch(alice.public_key, 100)?
            .clone();

        aggregator.add_batch(&batch.tx_hash(), &client.public_key)?;
        aggregator.start_collecting_signatures()?;
        let merkle_tree_proof = aggregator.generate_proof_for_batch(&batch)?;

        let signature = client.validate_and_sign_batch(&merkle_tree_proof)?;

        aggregator.add_signature(&batch.tx_hash(), &client.public_key, signature)?;

        let transfer_block = aggregator.finalise()?;

        rollup_state.add_transfer_block(transfer_block);

        alice.add_receiving_transaction(
            &merkle_tree_proof,
            &client.balance_proof,
            &rollup_state,
        )?;

        assert_eq!(alice.balance, 100);
        assert_eq!(client.balance, 200);

        Ok(())
    }

    fn complete_aggregator_round(
        sender: &mut Client,
        rollup_state: &mut MockRollupState,
        amount: u64,
    ) -> StatelessBitcoinResult<Client> {
        let mut aggregator = Aggregator::new();

        let mut receiver = Client::new();

        let batch = sender
            .append_transaction_to_batch(receiver.public_key, amount)?
            .clone();

        aggregator.add_batch(&batch.tx_hash(), &sender.public_key)?;
        aggregator.start_collecting_signatures()?;
        let merkle_tree_proof = aggregator.generate_proof_for_batch(&batch)?;
        let signature = sender.validate_and_sign_batch(&merkle_tree_proof)?;

        aggregator.add_signature(&batch.tx_hash(), &sender.public_key, signature)?;

        let transfer_block = aggregator.finalise()?;

        rollup_state.add_transfer_block(transfer_block);

        receiver.add_receiving_transaction(
            &merkle_tree_proof,
            &sender.balance_proof,
            rollup_state,
        )?;

        Ok(receiver)
    }

    #[test]
    fn test_long_chain_of_transactions_still_can_be_spent() -> StatelessBitcoinResult<()> {
        let amount = 100;

        let (client, mut rollup_state) = setup(amount)?;

        let mut next_sender = client;

        for _ in 0..10 {
            let receiver = complete_aggregator_round(&mut next_sender, &mut rollup_state, amount)?;

            assert_eq!(receiver.balance, amount);
            assert_eq!(next_sender.balance, 0);

            next_sender = receiver;
        }

        Ok(())
    }

    #[test]
    fn test_add_receiving_transaction_fails_when_transaction_not_in_rollup_state(
    ) -> StatelessBitcoinResult<()> {
        let amount = 100;

        let (mut client, rollup_state) = setup(amount)?;

        let mut aggregator = Aggregator::new();

        let mut receiver = Client::new();

        let batch = client
            .append_transaction_to_batch(receiver.public_key, amount)?
            .clone();

        aggregator.add_batch(&batch.tx_hash(), &client.public_key)?;
        aggregator.start_collecting_signatures()?;
        let merkle_tree_proof = aggregator.generate_proof_for_batch(&batch)?;
        let signature = client.validate_and_sign_batch(&merkle_tree_proof)?;

        aggregator.add_signature(&batch.tx_hash(), &client.public_key, signature)?;

        // Produce the transfer block, but don't add it to the rollup state
        aggregator.finalise()?;

        let value = receiver.add_receiving_transaction(
            &merkle_tree_proof,
            &client.balance_proof,
            &rollup_state,
        );

        match value {
            Err(err) => {
                // Downcast the anyhow::Error to your custom error
                let custom_error = err.downcast_ref::<StatelessBitcoinError>();
                assert_eq!(
                    custom_error,
                    Some(&StatelessBitcoinError::BatchNotInATransferBlock(
                        batch.clone()
                    ))
                );
            }
            _ => assert!(false, "Expected an error"),
        }

        Ok(())
    }
}
