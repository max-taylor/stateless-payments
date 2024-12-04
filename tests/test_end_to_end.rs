use stateless_bitcoin_l2::{
    aggregator::Aggregator,
    errors::CrateResult,
    rollup::{
        mock_rollup_memory::MockRollupMemory,
        traits::{MockRollupStateTrait, RollupStateTrait},
    },
    types::common::{BalanceProof, TransactionProof},
    wallet::wallet::Wallet,
};

// This test creates a number of accounts, each account sends a transaction to the next in the
// array. This goes on recursively until the last account has all the funds.
// This validates a relatively complex flow of transactions, proofs and dependent transactions.
#[test]
fn test_flow() -> CrateResult<()> {
    let mut rollup_state = MockRollupMemory::new();

    let num_accounts = 10;
    let amount_to_increment = 100;

    let mut accounts = (0..num_accounts)
        .map(|idx| {
            let mut client = Wallet::new();
            let amount = calculate_total_for_account(idx, amount_to_increment);
            rollup_state
                .add_deposit(client.public_key, amount.try_into().unwrap())
                .unwrap();
            client.sync_rollup_state(&rollup_state).unwrap();

            client
        })
        .collect::<Vec<_>>();

    let total_balance = accounts.iter().map(|account| account.balance).sum::<u64>();

    // Need to create a copy of the public keys to avoid borrowing issues in the loop
    let account_pubkeys = accounts
        .iter()
        .map(|account| account.public_key)
        .collect::<Vec<_>>();

    for aggregator_loop in 0..num_accounts {
        // Break on the last iteration because all the funds have moved to the last account
        if aggregator_loop == num_accounts - 1 {
            break;
        }
        fn skip_account(aggregator_loop: usize, account_idx: usize, num_accounts: usize) -> bool {
            aggregator_loop > account_idx || account_idx == num_accounts - 1
        }

        let mut aggregator = Aggregator::new();

        // Create all the transactions
        for (idx, account) in accounts.iter_mut().enumerate() {
            if skip_account(aggregator_loop, idx, num_accounts) {
                continue;
            }

            let receiver = account_pubkeys[idx + 1].clone();

            account.append_transaction_to_batch(receiver, account.balance)?;
            let batch = account.produce_batch()?;

            aggregator.add_batch(&batch)?;
        }

        aggregator.start_collecting_signatures()?;

        let mut proofs: Vec<(TransactionProof, BalanceProof)> = vec![];

        // Generate proofs and sign the transactions
        for (idx, account) in accounts.iter_mut().enumerate() {
            if skip_account(aggregator_loop, idx, num_accounts) {
                continue;
            }

            let batch = account.transaction_batch.clone();

            let transaction_proof = aggregator.generate_proof_for_batch(&batch)?;

            let signature = account.validate_and_sign_batch(&transaction_proof)?;

            aggregator.add_signature(&account.public_key, &signature)?;

            proofs.push((transaction_proof, account.balance_proof.clone()));
        }

        let block = aggregator.finalise()?;

        rollup_state.add_transfer_block(block)?;

        // Validate the proofs and update the balances
        for (idx, account) in accounts.iter_mut().enumerate() {
            // Add receiving transaction to the account only if it's not the first account
            if idx > aggregator_loop {
                let loop_diff = idx - aggregator_loop - 1;
                account.add_receiving_transaction(
                    &proofs[loop_diff].0,
                    &proofs[loop_diff].1,
                    &rollup_state,
                )?;
            }

            let expected_balance = calculate_expected_balance(
                aggregator_loop,
                idx,
                num_accounts,
                amount_to_increment,
            )?;

            assert_eq!(account.balance, expected_balance);
        }
    }

    // This is already validated above, but putting it here for clarity
    for (idx, account) in accounts.iter().enumerate() {
        if idx == num_accounts - 1 {
            assert_eq!(account.balance, total_balance);
        } else {
            assert_eq!(account.balance, 0);
        }
    }

    Ok(())
}

fn calculate_total_for_account(account_idx: usize, amount_to_increment: usize) -> usize {
    account_idx * amount_to_increment + amount_to_increment
}

fn calculate_expected_balance(
    aggregator_loop_idx: usize,
    account_idx: usize,
    num_accounts: usize,
    amount_to_increment: usize,
) -> CrateResult<u64> {
    Ok({
        if account_idx == num_accounts - 1 {
            let initial_amount_for_last_account =
                calculate_total_for_account(num_accounts - 1, amount_to_increment);
            let total = (0..aggregator_loop_idx + 1)
                .map(|idx| calculate_total_for_account(num_accounts - idx - 2, amount_to_increment))
                .sum::<usize>();

            total + initial_amount_for_last_account
        } else {
            if aggregator_loop_idx >= account_idx {
                0
            } else {
                let loop_diff = account_idx - aggregator_loop_idx;
                loop_diff * amount_to_increment
            }
        }
    }
    .try_into()?)
}
