# Carrier: vision and goals (the simple version)

[한국어](VISION.ko.md)

Written 2026-10-01. The product is called "zypher" for now (2026-10-03); this document still says "Carrier" for it.

## The vision in one sentence

**A place to buy and sell digital art on Zcash where nobody, not even us, holds your money or your art, and nobody can see what you own.**

## The problem

Today, if you buy digital art on Zcash, one of two things happens.

1. **A company keeps the list of owners.** The payment is private, but "who owns what" is a row in the company's database. If the company makes a mistake, changes the list, or shuts down, your ownership goes with it.
2. **The company holds your money in the middle of a sale.** You pay the marketplace, and the marketplace later hands the art to you and the money to the seller. You have to trust it not to lose, freeze, or steal the money. A hacked marketplace loses everyone's money at once.

## What we are building

A marketplace website where:

- **Your art is yours.** Owning a piece means holding a private note on the Zcash blockchain. No company list is involved.
- **A sale is one step.** The art and the money change hands in one blockchain transaction. Either both move or nothing moves. The website never touches the money or the art.
- **Nobody sees your collection.** Outsiders only see that a Zcash transaction happened. A buyer learns the history of the one piece they buy, and nothing else about you.
- **Anyone can check a piece is real**, in their own browser, from blockchain data and the piece's history that the seller hands over. You do not have to trust our website. (Today the check still trusts the blockchain data server it reads from; removing that trust is on the road below.)
- **The art lives on the blockchain**, not on a server that might go offline.

## Goals

1. **No custody.** The marketplace never holds users' money, art, or keys. The server only keeps a public list of offers.
2. **Private ownership.** Nobody can look up who owns what.
3. **Check, don't trust.** Every page shows a "verified" result that your own browser computed from the blockchain.
4. **Easy to use.** Connect a wallet, see your pieces, list, buy. No software to install.
5. **Honest.** We say plainly what the system can and cannot do, including its limits.

## What we will not do

- Hold users' funds, even for a moment.
- Store users' secret keys on our servers.
- Collect lists that link people's social accounts to their Zcash addresses.
- Sell or offer test-network items as if they were worth money.
- Hide limitations or overclaim. For example, we did not invent the basic chaining trick (another team published it first). What we add is the safe one-step sale and a working system.

## How we know it can work

Everything below happened on the Zcash test network, which uses play money.

| Date | What we showed |
|---|---|
| 2026-09-29 | Created two art pieces, capped the supply, moved one piece, and blocked three kinds of cheating |
| 2026-09-30 | Showed that a web browser can do the heavy math in about 2 to 4 seconds |
| 2026-09-30 | **Sold a piece in one transaction**: the art went to the buyer and the payment to the seller together, with no middleman |
| 2026-10-02 | Hardened the sale after an outside review: the buyer checks the piece before paying, the buyer's private details are removed from the shared file, and nine cheating attempts are refused, including a seller changing the deal after the buyer signed (test runs, nothing sent) |

The competing project's own whitepaper lists this kind of one-step sale as its biggest unanswered question. We answered it on the test network. So far both sides' keys lived on one machine; a run with truly separate parties is next.

## The road from here

1. **Make the sale private enough for the public.** Hand a piece's history only to a real buyer over a checked channel, and make sale transactions look like ordinary ones. (Hiding the buyer's details in the shared file is done.)
2. **Make recovery safe.** A user who restores their wallet from its recovery phrase gets their pieces and their proofs back.
3. **Build the browser engine.** One shared core that runs in the browser: verify, build, prove, sign. Check blockchain data against more than one source.
4. **Write the final marketplace plan and get it approved.** Then choose a visual design.
5. **Open a test-network beta.** Real people try it with play money. Nothing is sold for real value.
6. **Security review and legal review.**
7. **Decide on the real network (mainnet).** This is a separate decision, made only after steps 1 to 6.

## Still undecided

- The final product name (zypher is tentative).
- When, and whether, to move to the real network.
- How the marketplace earns money. One option is a small fee paid inside the same sale transaction, so we still never hold funds. This has not been built or tested.
- The visual design.

## Where to read more

- [OVERVIEW.md](OVERVIEW.md): how it works, in more detail
- [research/zecbit-whitepaper.md](research/zecbit-whitepaper.md): the competing design and what we learned from it
- [HANDOFF.md](HANDOFF.md): current state and next engineering tasks
