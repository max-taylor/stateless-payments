use std::collections::HashMap;

use anyhow::anyhow;

use crate::{
    errors::{CrateResult, StatelessBitcoinError},
    rollup::rollup_state::RollupStateTrait,
    types::{common::BalanceProof, public_key::BlsPublicKeyWrapper},
};

pub fn merge_balance_proofs(
    current_client_balance_proof: BalanceProof,
    sender_balance_proof: BalanceProof,
) -> CrateResult<BalanceProof> {
    let mut merged_balance_proof = current_client_balance_proof;

    for (key, value) in sender_balance_proof {
        if merged_balance_proof.contains_key(&key) {
            continue;
        }

        merged_balance_proof.insert(key, value);
    }

    Ok(merged_balance_proof)
}

// The pure method calculates the balances for all accounts in the balance proof, validating all
// proofs
pub fn calculate_balances_and_validate_balance_proof(
    rollup_state: &impl RollupStateTrait,
    balance_proof: &BalanceProof,
) -> CrateResult<HashMap<BlsPublicKeyWrapper, u64>> {
    // Use i128 to avoid underflow, we don't check deposit, withdrawal and tx ordering. We just
    // ensure the balance is > 0 for accounts at the end
    let mut unchecked_balances: HashMap<BlsPublicKeyWrapper, i128> = HashMap::new();

    for transaction_proof in balance_proof.values() {
        let batch = &transaction_proof.batch;

        // Validates that the transaction is included in the merkle root
        if !transaction_proof.verify() {
            return Err(anyhow!(format!(
                "Invalid transaction proof for transaction: {:?}",
                batch
            )));
        }

        // Ensures the merkle root and sender was included in a transfer block
        let transfer_block = rollup_state
            .get_transfer_block_for_merkle_root_and_pubkey(
                &transaction_proof.root,
                &batch.from.into(),
            )?
            .ok_or(StatelessBitcoinError::BatchNotInATransferBlock(
                batch.clone(),
            ))?;

        // Validates the aggregated signature
        transfer_block.verify()?;

        for transaction in batch.transactions.iter() {
            // u64 can safely be converted to i128
            let amount: i128 = transaction.amount.into();

            unchecked_balances
                .entry(batch.from.into())
                .and_modify(|e| *e -= amount)
                .or_insert(-amount);

            unchecked_balances
                .entry(transaction.to.into())
                .and_modify(|e| *e += amount)
                .or_insert(amount);
        }
    }

    let mut balances: HashMap<BlsPublicKeyWrapper, u64> = HashMap::new();

    for (public_key, amount) in unchecked_balances {
        let deposit_amount = rollup_state.get_account_deposit_amount(&public_key.into())?;
        let withdraw_amount = rollup_state.get_account_withdraw_amount(&public_key.into())?;

        let balance = amount + deposit_amount as i128 - withdraw_amount as i128;

        let balance = balance
            .try_into()
            .map_err(|_| anyhow!(format!("Balance for {:?} is negative", public_key)))?;

        balances.insert(public_key, balance);
    }

    Ok(balances)
}
