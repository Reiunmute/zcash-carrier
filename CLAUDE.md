# CLAUDE.md

Auto-loaded context for Claude Code in this repo. Read `docs/HANDOFF.md` before doing anything else.

## What this is

A Zcash testnet PoC for collectibles bound to shielded notes, plus the plan for a public marketplace. Rust 2021: a CLI (`carrier`) and an independent verifier (`verify`). Python helper scripts. There is no web app yet.

## Commands

```bash
cargo build --release      # always build release. Proving in debug is very slow.
bash verify.sh             # gate. Run after every code change. Must print "VERIFY.SH OK".
```

`verify.sh` needs network access (testnet lightwalletd). Holder (R3) checks run only when `state/seed.hex` exists.

## Hard rules

1. **Never commit or print anything from `state/`.** It holds the testnet seed and spending keys. It is git-ignored. Keep it that way. Do not paste the seed, spending keys, or `seed.hex` contents into any output, commit, doc, or log.
2. **Broadcasting a testnet transaction needs Ryot's approval in the current session.** Use `DRY=1` to build and prove without sending. Transactions are permanent. Testnet funds are scarce. The faucet pays 0.1 TAZ per claim with daily caps.
3. **Pushing to GitHub needs Ryot's approval.** A global pre-push hook blocks GitHub pushes unless `ALLOW_GITHUB_PUSH=1` is set. Commit locally freely. Push only when asked. Remote: `git@github.com:Reiunmute/zcash-carrier.git` (private). Author: `Ryan <reisummermute@gmail.com>`.
4. **Testnet only.** No mainnet code paths, keys, or endpoints.
5. **Never sell or offer testnet assets.** Ironwood pool only. Do not use the transparent pool for carriers.
6. **Do not collect lists pairing social accounts with Zcash addresses.**
7. **Do not modify `evidence/testnet-2026-09-29/`.** It is the public record of the run. New runs get a new dated folder.
8. **Do not implement `docs/marketplace/SPEC.md` until Ryot approves it.** It is draft 1 and not approved. Feasibility tasks listed in `docs/HANDOFF.md` are allowed.

## Conventions

- Surgical changes. Keep edits to the requested scope.
- Report numbers with their source (command output, file, line).
- Say "unverified" when something is unverified.
- Docs are in English. Ryot reads replies in Korean.
- No em dash (U+2014) in docs.
- Reference crate code before assuming behaviour. Crates live in `~/.cargo/registry/src/index.crates.io-*/`, including `orchard-0.15.5`, `zcash_primitives-0.30.1`, and `zcash_client_backend-0.24.0`.
