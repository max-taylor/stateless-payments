# Stateless Payments

This repo is a demo implementation of IntMax's rollup [whitepaper](https://eprint.iacr.org/2023/1082.pdf).

# Efficiency

The balance proof merging is pretty inefficient, we are simply merging the senders entire balance proof with the receivers. This becomes pretty huge and costly quickly, but at the same time in this Merkle proof implementation there doesn't seem to be a simple way around that. We could selectively choose transactions from the senders balance proof and merge them with receivers, but this becomes very complicated quickly. 


# TODO

- [ ] When a sender sends their balance proof to the receiver for validation and merging, we should check the rollup state to ensure that the sender didn't exclude any transactions from their balance proof. If they did that may mean they are trying to double spend
