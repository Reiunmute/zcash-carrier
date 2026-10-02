# Carrier spec (draft 1, 2026-10-02)

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

- **R1 issuance set**: start at `genesis_txid`. Each issuance tx must contain exactly one sequence note (SEQ or FINAL) that the issuer IVK opens, with value 0. Collect carriers using the issuer OVK; a carrier must have value 0 and hop 0, otherwise it is rejected. If the sequence note is not `FINAL`, look up its nullifier. The spending tx is the next step, and its sequence output must have `rho == that nullifier` and sit in the same Action. Stop at `FINAL`. Reject duplicate item numbers. Any carrier claiming the collection outside this set is invalid.
- **R2 transfer hop**: for each disclosed hop `(note, vault_fvk)`:
  - `cmx(note)` equals the current carrier commitment.
  - `vault_fvk` decrypts the current carrier from chain, at the Action that created it. The incoming viewing key derives the note's address, so this also binds the key to the note.
  - For every hop after the first, that note is a valid successor: value 0 and a `HOP` memo for the same collection and item, with hop counter = the replaced note's hop + 1. Otherwise the item is **burned** and later hops are ignored.
  - `nf = note.nullifier(vault_fvk)` appears on chain.
  - The new current carrier is the `cmx` of the Action that revealed `nf`.
- **R3 holder**: the claimed holder's key decrypts the current carrier, that carrier passes the successor rule above (for transferred items), and its nullifier has not appeared on chain.

Verdicts (`verify` exit codes): **HELD** 0, **FAIL** 2, **BURNED** 3, **SPENT** 4 (the chain is valid but the holder's carrier is already spent, so the discloser no longer holds the item), **UNVERIFIED** 5 (no holder key: R1 and R2 up to the last hop only).

Trust: the verifier reads chain data from one lightwalletd server. It checks txids, hash links and compact-versus-full consistency, and can cross-check block hashes with a second server (`LWD2`). It does not validate proof of work (OPEN_ISSUES #5).

A zero-value spend skips the in-circuit Merkle membership check ("either v_old = 0 or root = anchor"). R2 therefore always takes the new commitment from the Action that revealed the nullifier. It never accepts a commitment that the seller merely claims.

## Wallet requirement

Standard builders shuffle spends and outputs independently. The tool rebuilds and re-proves until `BundleMetadata` places each bound pair in the same Action. For 2 Actions, each build succeeds with probability 1/2.
