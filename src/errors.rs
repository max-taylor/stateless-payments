use crate::types::transaction::TransactionBatch;
use thiserror::Error;

pub type CrateResult<T> = anyhow::Result<T>;

#[derive(Debug, Error, PartialEq)]
pub enum StatelessBitcoinError {
    #[error("TransactionBatch not in a transfer block, batch: {0:?}")]
    BatchNotInATransferBlock(TransactionBatch),
}
