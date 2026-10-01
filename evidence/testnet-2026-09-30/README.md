# Testnet run 2026-09-30: two-party atomic sale via PCZT

Task 2 in `docs/HANDOFF.md`. Item #1 of collection `c7fe2aeadb5a6200` (see `../testnet-2026-09-29/`) moved from the seller's vault to the buyer's new vault, and the payment went to the seller in the **same transaction**. The tx was built and signed by two key sets that only met through the PCZT file.

Roles (`STATE=state OFF=10`):

| Role | Account | Signs |
|---|---|---|
| seller vault (held item #1) | acct3 | carrier spend |
| seller pay address | acct5 | nothing |
| buyer funding | acct0 | funding spend |
| buyer new vault | acct6 | nothing |

## Transaction

`6bdeb823ff6556dce7ef8c254c8ceaa257913cc820c43da271c52dabdc98ba5b`, mined at height 4425592.

| Action | Content | Source |
|---|---|---|
| 2 | spend of carrier `b36afb0725fe..` (acct3) and successor `5cb2cdaab67c..` to acct6, value 0, HOP memo item 1 hop 2 | `sale.json`, `verify_sale.log` |
| 0 | payment 10,000 zats to acct5 | unspent check with the acct5 FVK |
| 1 | buyer change 9,885,000 zats to acct0 (9,910,000 - 10,000 - fee 15,000) | unspent check with the acct0 FVK |

The funding note spent was the acct0 change of `78a9a662..` (9,910,000 zats).

## Flow

```bash
export STATE=state OFF=10
carrier sale-list     # seller: listing.json (price, pay address, expiry, carrier note, vault FVK, package)
carrier sale-buy      # buyer: PCZT, bound carrier pair, proof, buyer signature, buyer FVK and witness redacted
carrier sale-accept   # seller: checks, carrier signature, extract (proof and signatures verified), broadcast
```

The seller's checks run on the PCZT effects (ciphertexts and nullifiers), not on its plaintext fields: expiry height, consensus branch, carrier nullifier present, and payment trial-decrypted with the seller pay key equal to the listed price.

## Negative checks (`negative_checks.log`, all `DRY=1`, before the broadcast)

- Buyer alone cannot extract a tx: `Ironwood(Extract(MissingSpendAuthSig))`.
- Tampered amount (buyer pays 9,000 against a 10,000 listing): seller refuses, `payment 9000 zats != listed price 10000`.
- Expired PCZT (expiry 4424924, tip 4424984): seller refuses.

## Verification (`verify_sale.log`)

- With the acct6 FVK as holder: hop 0 and hop 1 successors valid, `R3 holder opens current carrier: yes, unspent=true`.
- With the acct3 (seller) FVK as holder: `VERIFY FAIL: holder cannot open current carrier`.
- `verify.sh` re-checks this package.

## Not covered

- The PCZT is not published here. Its plaintext output fields still include the buyer's change note. Redacting those before handing the PCZT to the seller is a follow-up.
- Unverified: a seller who modifies the effects after the buyer signed should fail extraction, because the buyer's spend signature covers the shielded sighash. This was not tested.
