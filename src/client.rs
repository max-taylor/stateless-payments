use std::collections::HashMap;

use anyhow::anyhow;

use crate::{
    errors::StatelessBitcoinResult,
    rollup::rollup_state::RollupStateTrait,
    types::{
        common::{
            generate_salt, BalanceProof, BlsPublicKey, BlsSecretKey, BlsSignature,
            TransactionProof, U8_32,
        },
        public_key::BlsPublicKeyWrapper,
        transaction::SimpleTransaction,
    },
};

pub struct Client {
    pub public_key: BlsPublicKey,
    pub private_key: BlsSecretKey,

    // Mapping of Merkle Root -> (Transaction, TransactionProof)
    // We use the merkle root for lookups
    pub balance_proof: BalanceProof,
    // Mapping of Transaction Hash -> Transaction
    // Use the tx_hash for lookups in this case
    pub uncomfirmed_transactions: HashMap<U8_32, SimpleTransaction>,

    pub balance: u64,
}

impl Client {
    pub fn new() -> Client {
        let private_key = BlsSecretKey::new();

        Client {
            private_key: private_key.clone(),
            public_key: private_key.public_key(),
            balance_proof: HashMap::new(),
            uncomfirmed_transactions: HashMap::new(),
            balance: 0,
        }
    }

    pub fn sync_rollup_state(
        &mut self,
        rollup_state: &impl RollupStateTrait,
    ) -> StatelessBitcoinResult<()> {
        let deposit_amount = rollup_state.get_account_deposit_amount(self.public_key)?;
        let withdraw_amount = rollup_state.get_account_withdraw_amount(self.public_key)?;

        let transfer_blocks = rollup_state.get_account_transfer_blocks(self.public_key)?;

        self.balance = deposit_amount - withdraw_amount;

        Ok(())
        // self.transaction_history = rollup_state.get_transaction_history();
        // self.balance = rollup_state.get_balance(self.public_key);
    }

    pub fn create_transaction(
        &mut self,
        to: BlsPublicKey,
        amount: u64,
    ) -> StatelessBitcoinResult<SimpleTransaction> {
        let salt = generate_salt();

        if to == self.public_key {
            return Err(anyhow!("Cannot send to self"));
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

        self.uncomfirmed_transactions
            .insert(transaction.tx_hash(), transaction.clone());

        Ok(transaction)
    }

    // Called when another client sends funds to this client
    // TODO: we actually need the other user's entire balance proof
    // TODO: Figure out how to merge the two balance proofs
    pub fn add_receiving_transaction(
        &mut self,
        transaction: SimpleTransaction,
        transaction_proof: TransactionProof,
        rollup_contract: impl RollupStateTrait,
    ) -> StatelessBitcoinResult<()> {
        // TODO: somewhere we need to check that the transaction has been submitted on-chain, will
        // work for both the user submitting a tx and the receiver getting a tx
        let tx_hash = transaction_proof.tx_hash;

        let transaction = self
            .uncomfirmed_transactions
            .get(&tx_hash)
            .ok_or(anyhow!("Transaction not found"))?;

        if transaction.to != self.public_key {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        // This isn't really needed because validate_and_sign_transaction will be called first and
        // it checks this, but it's here for completeness
        if !transaction_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        self.balance_proof.insert(
            (transaction_proof.root, transaction.from.into()),
            (transaction.clone(), transaction_proof.clone()),
        );

        self.uncomfirmed_transactions.remove(&tx_hash);

        Ok(())
    }

    // This is the function that the aggregator will call to get the signature
    // Internally we move the transaction from unconfirmed to confirmed as its been accepted by the
    // aggregator and a block will be produced
    //
    // ! You could validate the inclusion by using the RollupContractTrait, but doesn't seem
    // necessary
    pub fn validate_and_sign_transaction(
        &mut self,
        transaction_proof: &TransactionProof,
    ) -> StatelessBitcoinResult<BlsSignature> {
        let tx_hash = transaction_proof.tx_hash;

        let transaction = self
            .uncomfirmed_transactions
            .get(&tx_hash)
            .ok_or(anyhow!("Transaction not found"))?;

        if !transaction_proof.verify() {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let signature = self.private_key.sign(
            blsful::SignatureSchemes::MessageAugmentation,
            &transaction_proof.root,
        )?;

        self.balance_proof.insert(
            (transaction_proof.root, self.public_key.into()),
            (transaction.clone(), transaction_proof.clone()),
        );

        self.uncomfirmed_transactions.remove(&tx_hash);

        Ok(signature)
    }
}

pub fn merge_balance_proofs(
    current_client_balance_proof: BalanceProof,
    sender_balance_proof: BalanceProof,
) -> StatelessBitcoinResult<BalanceProof> {
    let mut merged_balance_proof = current_client_balance_proof;

    for (key, value) in sender_balance_proof {
        if merged_balance_proof.contains_key(&key) {
            continue;
        }

        merged_balance_proof.insert(key, value);
    }

    Ok(merged_balance_proof)
}

// TODO: Kinda needs to be recursive and validate senders balance proofs
pub fn calculate_balances_and_validate_balance_proof(
    rollup_state: impl RollupStateTrait,
    balance_proof: BalanceProof,
) -> StatelessBitcoinResult<HashMap<BlsPublicKeyWrapper, u64>> {
    let mut balances: HashMap<BlsPublicKeyWrapper, u64> = HashMap::new();
    let rollup_deposits = rollup_state.get_deposit_totals()?;
    let rollup_withdrawals = rollup_state.get_withdraw_totals()?;

    // for (transaction, transaction_proof) in balance_proof.values() {
    //     transaction_proof.verify();
    //     let sender_balance = balances.get(&transaction.from.into()).cloned().unwrap_or(0);
    //
    //     let receiver_balance = balances.get(&transaction.to.into()).cloned().unwrap_or(0);
    //
    //     balances.insert(transaction.from.into(), sender_balance - transaction.amount);
    //     balances.insert(transaction.to.into(), receiver_balance + transaction.amount);
    // }

    Ok(balances)

    // for proof in balance_proof {
    //     if proof.transaction.to == address {
    //         balance += proof.transaction.amount as f64;
    //     }
    // }
    //
    // balance_proof
    //     .iter()
    //     .all(|proof| proof.transaction.to == address)
}

#[cfg(test)]
mod tests {
    use crate::{
        aggregator::Aggregator, errors::StatelessBitcoinResult,
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

        rollup_state.add_withdraw(client.public_key, 50)?;

        client.sync_rollup_state(&rollup_state)?;

        assert_eq!(client.balance, 50);

        Ok(())
    }

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

        let transaction = client.create_transaction(receiver.public_key, 100)?;

        assert_eq!(transaction.from, client.public_key);
        assert_eq!(transaction.to, receiver.public_key);
        assert_eq!(transaction.amount, 100);
        assert_eq!(client.uncomfirmed_transactions.len(), 1);
        assert_eq!(client.balance, 0);

        Ok(())
    }

    #[test]
    fn test_create_transaction_fails_with_insufficient_balance() -> StatelessBitcoinResult<()> {
        let (mut client, _) = setup(100)?;

        let receiver = Client::new();

        let transaction = client.create_transaction(receiver.public_key, 101);

        assert!(transaction.is_err());
        assert_eq!(client.uncomfirmed_transactions.len(), 0);

        Ok(())
    }

    #[test]
    fn test_validate_and_sign_transaction_succeeds() -> StatelessBitcoinResult<()> {
        let (mut client, _) = setup(100)?;
        let mut aggregator = Aggregator::new();
        let receiver = Client::new();
        let transaction = client.create_transaction(receiver.public_key, 100)?;

        aggregator.add_transaction(&transaction.tx_hash(), &client.public_key)?;
        aggregator.start_collecting_signatures()?;

        let merkle_tree_proof =
            aggregator.generate_proof_for_tx_hash(&transaction.tx_hash(), &client.public_key)?;

        let signature = client.validate_and_sign_transaction(&merkle_tree_proof)?;

        assert_eq!(client.balance, 0);
        assert_eq!(client.uncomfirmed_transactions.len(), 0);
        assert_eq!(client.balance_proof.len(), 1);

        let result = signature.verify(&client.public_key, &merkle_tree_proof.root);

        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_add_receiving_transaction_succeeds() -> StatelessBitcoinResult<()> {
        // Adds transaction to transaction history
        // Removed from unconfirmed transactions
        // Validate signature
        // Balance increases
        Ok(())
    }

    #[test]
    fn test_add_receiving_transaction_fails_when_transaction_not_in_rollup_state() {
        // Invalid transaction
    }
}
