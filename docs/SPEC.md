# Carrier spec (draft 0)

## Terms

- **carrier**: a zero-value shielded note in the Ironwood pool that represents one NFT item.
- **vault**: a dedicated key used only to hold one carrier. Its full viewing key is disclosed to the next buyer.
- **sequence note**: a zero-value note held by the issuer. Its chain of successors defines the issuance set.
- **same-Action binding**: an input note and an output note are placed in the same Action. The Action circuit enforces `rho(output) = nf(input)` (orchard `circuit.rs`, `rho_new = nf_old`; ZIP 2005 for Ironwood).

## Memo layout (512 bytes, ZIP 302 arbitrary data)

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | `0xF5` |
| 1 | 4 | magic `ZCR1` |
| 5 | 1 | kind: 1 SEQ, 2 CARRIER, 3 HOP, 4 FINAL |
| 6 | 32 | collection id |
| 38 | 4 | item number, little endian |
| 42 | 4 | hop counter, little endian |
| 46 | 2 | payload length, little endian |
| 48 | n | payload (artwork for CARRIER) |

Artwork payload v0: 48-byte palette (16 x RGB), then 338 bytes of 4-bit pixel indices for 26x26, row-major, high nibble first. The total is 386 bytes.

## Issuance

1. Genesis tx: outputs `SEQ` to the issuer and one `CARRIER` per item to vaults. **Every issuance output uses the issuer OVK.**
2. Each later issuance tx spends the live sequence note and outputs its successor in the same Action. The successor is `SEQ`, or `FINAL` to close. The tx also outputs new carriers, all with the issuer OVK.
3. The collection record `{network, coll, issuer_fvk, genesis_txid}` is published.

## Verification rules

- **R1 issuance set**: start at `genesis_txid`. In each tx, find the sequence note using the issuer IVK. Collect carriers using the issuer OVK. If the sequence note is not `FINAL`, look up its nullifier. The spending tx is the next step, and its sequence output must have `rho == that nullifier` and sit in the same Action. Stop at `FINAL`. Reject duplicate item numbers. Any carrier claiming the collection outside this set is invalid.
- **R2 transfer hop**: for each disclosed hop `(note, vault_fvk)`:
  - `cmx(note)` equals the current carrier commitment.
  - `vault_fvk` owns `note.recipient`.
  - `nf = note.nullifier(vault_fvk)` appears on chain.
  - The new current carrier is the `cmx` of the Action that revealed `nf`.
- **R3 holder**: the claimed holder can trial-decrypt the current carrier with its own key, and that note's nullifier has not appeared on chain.

A zero-value spend skips the in-circuit Merkle membership check ("either v_old = 0 or root = anchor"). R2 therefore always takes the new commitment from the Action that revealed the nullifier. It never accepts a commitment that the seller merely claims.

## Wallet requirement

Standard builders shuffle spends and outputs independently. The tool rebuilds and re-proves until `BundleMetadata` places each bound pair in the same Action. For 2 Actions, each build succeeds with probability 1/2.
