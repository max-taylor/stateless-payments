# Stateless Payments

This repo is a demo implementation of IntMax's rollup [whitepaper](https://eprint.iacr.org/2023/1082.pdf).

# Efficiency

The balance proof merging is pretty inefficient, we are simply merging the senders entire balance proof with the receivers. This becomes pretty huge and costly quickly, but at the same time in this Merkle proof implementation there doesn't seem to be a simple way around that. We could selectively choose transactions from the senders balance proof and merge them with receivers, but this becomes very complicated quickly. 

# Edge-cases

Documenting every edge-case that comes up for full transparency.

## Withdraw + Transfer double spend
There may be a weird edge case where:
- The Wallet class has produced a block, but the new block isn’t written onchain yet
- The owner of the Wallet class withdraws funds from the L1
When the rollup state resyncs with the Wallet the balance proof will be in an invalid state, because of the double-spend.
Try to fix because now the users balance proof is basically bad. Only fix really is to ensure the balance proof proving function works sequentially, by updating the balance as it goes proving each deposit, withdraw and transfer in order. If any are invalid; i.e; they drop the balance below 0 we ignore it. 
- NOTE: If a withdraw drops the balance below 0 that is a bad error that should never occur.
- If it did its more than likely a transfer was processed either immediately before or after (indicating a double-spend). If that is the case disregard the transfer and include the withdraw.


## The user signs a TransferBlock’s merkle root but it never gets on-chain
When the wallet signs a merkle root via `validate_and_sign_proof` it moves the transaction_proof into the user’s balance proof, which basically confirms it on the users side.
If for whatever reason this transfer block never makes it on-chain that user will basically have “dead” money in their balance proof.
Only work around I can see at the moment is to add a timeout to transfer blocks inside a wallets balance proof. When we sync the rollup state we check for confirmations of the transfer blocks on chain, if we don’t find one and the transfer block as “timed out” then we can remove it from the user’s balance proof.
But be wary because once removed it can never be restored, so the timeout needs to 
be huge.

## User sends a batch whilst the current batch is awaiting signatures
The aggregator will reject this batch because it can’t append to the existing batch while its awaiting clients to sign the current merkle root. If it did add it to the merkle tree the root would change causing all sorts of issues. 
This causes a bit of a divide between the clients state and the servers state, but given this batch isn’t added to the users balance proof UNTIL they sign it, if the user just restarts their CLI this will go away.
For a fix, given how websockets work it is a bit annoying to send a response back to the client to confirm acceptance, when we move to HTTP request/response this will be solved because we can just send an error status code to indicate rejection.

## User’s goes offline and their transaction batch is confirmed
The sender messages the receiver with their transaction batch via a separate thread that checks the transfer blocks to a user and compares it to the previous loop. If there is any difference it extracts the difference and finds the relevant proofs, then messages the receivers.
If the user is offline when it is confirmed, this check won’t get a chance to be fulfilled and will instead not fire at all.
Fix for this will be to track "uncomfirmed transactions” for the user, when a transaction is found in the rollup state it is moved to confirmed and the user is messaged.
- NOTE: This can pretty easily be expanded to also handle users being offline. If the user doesn’t respond with a confirmation that they have received it, we keep the transaction in an uncomfirmed state and retry every so often.
- DOUBLE NOTE: We could also resolve the “user signs a TransferBlock’s merkle root but it never gets on chain” here by adding a timeout to these uncomfirmed transactions

## Append tx and add deposit
If you append a transaction, then add a deposit before you send a batch to the aggregator and sign the root then the user’s balance will update to not include the deduction from the transaction.
This is because new transactions aren’t added to the balance proof until the user signs the transaction, then the automate syncing of the rollup state uses the clients current balance proof (excluding the new transaction). So it will update the users balance to not include the new transaction.
This is resolved when the user restarts the CLI.
For a fix we need to track the lifecycle of transactions better, created → accepted → confirmed → onchain.


# TODO

- [ ] When a sender sends their balance proof to the receiver for validation and merging, we should check the rollup state to ensure that the sender didn't exclude any transactions from their balance proof. If they did that may mean they are trying to double spend
