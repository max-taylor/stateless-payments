use stateless_bitcoin_l2::{
    aggregator::Aggregator,
    client::client::Client,
    errors::StatelessBitcoinResult,
    rollup::rollup_state::MockRollupState,
    types::common::{BalanceProof, TransactionProof},
};

#[test]
fn test_flow() -> StatelessBitcoinResult<()> {
    let mut rollup_state = MockRollupState::new();

    let num_accounts = 10;
    let amount_to_increment = 100;

    let mut accounts = (0..num_accounts)
        .map(|idx| {
            let mut client = Client::new();
            let amount = idx * amount_to_increment + amount_to_increment;
            rollup_state.add_deposit(client.public_key, amount.try_into().unwrap());
            client.sync_rollup_state(&rollup_state).unwrap();

            dbg!(client.balance);

            client
        })
        .collect::<Vec<_>>();

    // Need to create a copy of the public keys to avoid borrowing issues in the loop
    let account_pubkeys = accounts
        .iter()
        .map(|account| account.public_key)
        .collect::<Vec<_>>();

    for idx in 0..1 {
        let mut aggregator = Aggregator::new();

        for (idx, account) in accounts.iter_mut().enumerate() {
            if idx == num_accounts - 1 {
                break;
            }

            let receiver = account_pubkeys[idx + 1].clone();

            let batch = account.append_transaction_to_batch(receiver, account.balance)?;

            aggregator.add_batch(&batch.tx_hash(), &account.public_key)?;
        }

        aggregator.start_collecting_signatures()?;

        let mut proofs: Vec<(TransactionProof, BalanceProof)> = vec![];

        for (idx, account) in accounts.iter_mut().enumerate() {
            if idx == num_accounts - 1 {
                break;
            }

            let batch = account.transaction_batch.clone();

            let transaction_proof = aggregator.generate_proof_for_batch(&batch)?;

            let signature = account.validate_and_sign_batch(&transaction_proof)?;

            aggregator.add_signature(&batch.tx_hash(), &account.public_key, signature)?;

            proofs.push((transaction_proof, account.balance_proof.clone()));
        }

        let block = aggregator.finalise()?;

        rollup_state.add_transfer_block(block);

        for (idx, account) in accounts.iter_mut().enumerate() {
            // Add receiving transaction to the account only if it's not the first account
            if idx != 0 {
                account.add_receiving_transaction(
                    &proofs[idx - 1].0,
                    &proofs[idx - 1].1,
                    &rollup_state,
                )?;
            }

            let expected_balance: u64 = if idx == num_accounts - 1 {
                num_accounts * amount_to_increment * 2 - amount_to_increment
            } else {
                idx * amount_to_increment
            }
            .try_into()?;

            assert_eq!(account.balance, expected_balance);
        }
    }

    Ok(())
}
