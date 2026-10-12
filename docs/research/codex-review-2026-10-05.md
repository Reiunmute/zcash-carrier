# Codex re-review 2, 2026-10-05

Reviewer: Codex CLI 0.144.5, model `gpt-5.6-sol`, reasoning effort `xhigh`, read-only sandbox, no network.
Input: a clean clone at commit `7008bd4` (no `state/`; all 214 blobs added since `987d881` scanned clean for the seed and spending keys).
Scope: every change after `987d881` (the follow-ups to the first re-review) up to `7008bd4`. Prompt: protocol security against SPEC draft 4, code/SPEC/doc mismatches, denial of service, deployment security.

Kept verbatim below, except that absolute paths were rewritten to repo-relative paths and em dashes to colons (docs rule). Follow-ups: see the commits after `56e6bf1` and the status notes in `docs/OPEN_ISSUES.md`.

## Follow-up status (2026-10-12)

Code changes made on 2026-10-05 and committed on 2026-10-12. Checked with `cargo test --workspace` (13 tests pass), `bash verify.sh` (OK) and `scripts/orderbook_e2e.sh` (OK, DRY, nothing broadcast). No new testnet transactions.

| # | Finding | Status |
|---|---|---|
| 1 | Browser trusts one lightwalletd | open |
| 2 | Native verifier truncates to the lower tip | fixed: both servers must report the same tip; the verifier waits up to 300 s, then fails (`src/verifier.rs`) |
| 3 | No explicit `rho == nf` check on successors | fixed in `successor_valid` and in every record-walk hop (`crates/carrier-core/src/verify.rs`) |
| 4 | Recovery accepts HOP-shaped outputs without provenance | fixed: each recovered record is walked to its issuance and checked with R1 to R3 before it is shown (`verify_recovered`, `src/bin/carrier.rs`) |
| 5 | Vault index allocation not concurrency-safe | fixed: `STATE/vault.lock`, index reserved before proving |
| 6 | Lease renewal not reachable from the CLI | fixed: `sale-renew` and `sale-extend` |
| 7 | Paid testnet listings required | fixed: price 0 by default; the order book accepts only price 0 (SPEC D4) |
| 8 | "Mined" checks trust the server's height | fixed in the `verify` CLI's mined and payment checks: `included_at` requires the txid in the compact block at that height (`src/lib.rs`, `src/bin/verify.rs`) |
| 9 | Verification gate can omit checks | open |
| 10 | Stale status docs | partly: README rewritten 2026-10-12 |
| 11 | Order book exhaustible | fixed: PoW on listings, write tokens per direction, one write per (type, direction, sequence), per-type and total byte caps, intent TTL, per-IP write bucket (`crates/orderbook`) |
| 12 | Unbounded parsing paths | partly: sequence and package hops capped at `MAX_WALK_HOPS` |
| 13 | Fresh deploy lacks the WASM bundle | open |
| 14 | Browser integrity controls absent | open |
| 15 | Envoy limits are global | open |

---

# Review result

I found no critical flaw in the PCZT atomic-exchange construction itself, assuming an honest consensus chain view. I found **4 high, 10 medium, and 1 low** issue. The highest risks are false `HELD` results from incomplete lightwalletd verification, overly permissive recovery, and trivial order-book denial of service.

No files were changed.

## 1. Protocol correctness and security

### 1. High: The browser trusts one hard-coded lightwalletd

The browser verifier uses only ChainSafe’s endpoint and constructs one `BrowserChain`: [verify.mjs:6](web/prototype/verify.mjs:6), [verify.mjs:14-23](web/prototype/verify.mjs:14). The WASM code explicitly leaves two-server comparison to its caller: [browser.rs:6](crates/carrier-wasm/src/browser.rs:6).

This contradicts the required independent-server agreement in [marketplace/SPEC.md:187-194](docs/marketplace/SPEC.md:187) and the claim in [OPEN_ISSUES.md:54-56](docs/OPEN_ISSUES.md:54).

**Failure scenario:** a compromised server can hide a carrier spend and show an already-sold item as `HELD`. More seriously, a malicious seller cooperating with that server can supply fabricated, unproved Actions that appear to establish counterfeit provenance. The client does not validate consensus proofs or headers.

**Smallest fix:** establish two independent sessions, compare exact compact-block content and raw transaction bytes/heights, scan through the highest mutually covered tip, and fail closed on disagreement or lag. Do not present browser verification as authoritative until this exists.

### 2. High: Native verification truncates liveness to the lower server tip

The native verifier tolerates three blocks of skew and sets the scan end to `min(tip1, tip2)`: [verifier.rs:50-60](src/verifier.rs:50). Only that shortened range is compared and indexed: [verifier.rs:60-83](src/verifier.rs:60).

**Failure scenario:** server A is at height H and contains the carrier spend; server B is honestly or maliciously at H−1. The verifier scans only through H−1 and returns `HELD`, even though its own highest-known tip proves that the result is stale.

**Smallest fix:** never return current `HELD` unless both servers cover the decision height. On skew, wait/fail or return `UNKNOWN: verified only through H−1`, rather than silently truncating.

### 3. Medium: Successor verification omits an explicit `rho == spent nullifier` check

Issuance performs the explicit same-Action binding: [verify.rs:100-103](crates/carrier-core/src/verify.rs:100). R2 and `successor_valid`, however, check the output commitment, memo, value, item and hop without explicitly checking that the decrypted successor note’s `rho` equals that Action’s input nullifier: [verify.rs:182-187](crates/carrier-core/src/verify.rs:182), [verify.rs:219-237](crates/carrier-core/src/verify.rs:219). The record walk checks the previous carrier nullifier against the current Action input, but not the successor note’s `rho`: [verify.rs:278-280](crates/carrier-core/src/verify.rs:278).

A valid mined Orchard transaction enforces this through the circuit, so this is principally a defense against fabricated LWD data.

**Failure scenario:** a malicious server supplies a structurally consistent but unproved Action whose output commitment decrypts correctly while its `rho` belongs to another Action. The local structural verifier accepts the missing binding.

**Smallest fix:** add the explicit equality check in R2 and every record-walk successor step. This is cheap defense in depth, though it does not replace consensus/header verification.

### 4. High: Recovery accepts a HOP-shaped output without proving provenance

Recovery accepts an `ACCOUNT` record and searches the same transaction for any output satisfying `successor_valid`: [carrier.rs:963-978](src/bin/carrier.rs:963). It does not walk records to an R1 issuance or perform full R1–R3 verification, yet reports the result as recovered and “unspent (held)”: [carrier.rs:1011-1017](src/bin/carrier.rs:1011). That is weaker than [marketplace/SPEC.md:156-161](docs/marketplace/SPEC.md:156).

**Failure scenario:** anyone who knows the account recovery address can send an encrypted `ACCOUNT` record plus an unrelated zero-value HOP-shaped output to the derived vault address. Recovery reports an arbitrary fake item as held and may create index conflicts.

**Smallest fix:** after decoding `ACCOUNT`, require the exact same-Action binding, walk the complete record chain to a valid R1 issuance, and run R1–R3 before displaying `HELD`. Treat failures as invalid records, not recovered items.

### 5. Medium: Automatic vault-index allocation is not concurrency-safe

The CLI scans for and chooses an index before constructing and proving the transaction: [carrier.rs:731-740](src/bin/carrier.rs:731). It reserves the index only afterward: [carrier.rs:833-835](src/bin/carrier.rs:833). The implementation acknowledges its one-process assumption: [carrier.rs:984-1002](src/bin/carrier.rs:984), while the specification requires a lock: [marketplace/SPEC.md:35](docs/marketplace/SPEC.md:35).

**Failure scenario:** two simultaneous purchases select the same vault index. Later disclosure of that vault FVK for one item lets the counterparty decrypt and track the other item and causes recovery ambiguity. It does not reveal the spending key.

**Smallest fix:** take a per-seed lock and atomically persist/reserve a monotonic index before proof construction.

### 6. Medium: The advertised lease renewal cannot work through the CLI

The specification calls leases renewable and idempotent: [marketplace/SPEC.md:85](docs/marketplace/SPEC.md:85). The CLI reconstructs its replay set and rejects an already-seen nonce before reaching lease acquisition/renewal: [carrier.rs:679-693](src/bin/carrier.rs:679). A new nonce creates a different transcript and therefore a different holder.

**Failure scenario:** a buyer’s proof workflow exceeds 30 minutes. Replaying the intent is rejected; using a new nonce cannot renew the existing lease, so the buyer loses its reservation and computation costs.

**Smallest fix:** define an authenticated renewal request proving possession of the buyer channel secret, persist lease/replay state, and make re-sending the disclosure idempotent for that authenticated holder.

## 2. Specification and evidence mismatches

### 7. Medium: The implementation requires paid testnet listings

The approved draft says the default testnet marketplace is free-transfer only: [marketplace/SPEC.md:245](docs/marketplace/SPEC.md:245). The repository also says not to sell testnet assets: [CLAUDE.md:24](CLAUDE.md:24).

The order book rejects zero prices: [book.rs:99-100](crates/orderbook/src/book.rs:99). The CLI does the same and defaults to 10,000 zatoshis: [carrier.rs:622-625](src/bin/carrier.rs:622). The UI requires a positive TAZ amount: [app.js:343-366](web/prototype/app.js:343).

**Failure scenario:** the only workflow accepted by the deployed testnet implementation violates the approved policy and project rule.

**Smallest fix:** require `price == 0` on testnet in the order book, CLI and UI. Keep the required payment Action structurally present with zero value if necessary.

### 8. Medium: Some “mined” evidence checks prove only that an LWD returned a height

The payment-file, payment and mined modes call `get_transaction` and accept a nonzero returned height without proving compact-block inclusion: [src/bin/verify.rs:38-60](src/bin/verify.rs:38), [src/bin/verify.rs:71-75](src/bin/verify.rs:71).

**Failure scenario:** a malicious LWD returns the expected raw transaction bytes with a fabricated height. The evidence gate reports it as mined even though it has not demonstrated inclusion in that block.

**Smallest fix:** locate the txid in the compact block at the claimed height, compare two servers, and report the exact verified height/tip.

### 9. Medium: The verification gate can silently omit important checks

`verify.sh` says the PCZT negative cases are log evidence rather than rerun tests: [verify.sh:10-11](verify.sh:10). WASM/browser checks are conditionally skipped when targets or tools are absent, yet the script can still complete successfully: [verify.sh:42-72](verify.sh:42). The carrier-sale Rust test is primarily a happy-path selftest: [carrier-sale/src/lib.rs:333-339](crates/carrier-sale/src/lib.rs:333).

**Failure scenario:** a regression exposing redacted PCZT fields, accepting a mutated payment, or mishandling tip skew passes a nominally successful gate because the relevant test was skipped or only represented by old logs.

**Smallest fix:** make required CI prerequisites fail closed and add deterministic offline adversarial tests for PCZT mutation/redaction, recovery forgery, tip skew, record cycles and order-book exhaustion.

### 10. Low: Status documentation is stale or internally inconsistent

Examples include README claims that separate-seed support is pending and that the marketplace remains draft 3: [README.md:58](README.md:58), [README.md:73](README.md:73). CLAUDE says there is no web app: [CLAUDE.md:7](CLAUDE.md:7).

**Failure scenario:** an operator or reviewer follows obsolete constraints and gives the current implementation more or less assurance than intended.

**Smallest fix:** update these status documents together and distinguish implemented, tested, deployed and deferred guarantees.

## 3. Denial of service and resource limits

### 11. High: The order book is trivially exhaustible

The global limits are fixed counts without a global byte budget: [book.rs:8-15](crates/orderbook/src/book.rs:8). Listings require neither proof of ownership nor PoW, so an attacker can occupy all 10,000 slots: [book.rs:93-112](crates/orderbook/src/book.rs:93).

Each listing allows 32 intents: [book.rs:140-169](crates/orderbook/src/book.rs:140). Each transcript can hold 16 large hex-encoded ciphertexts, with no global storage bound, write authentication, sequence uniqueness or consumption: [book.rs:177-198](crates/orderbook/src/book.rs:177). The HTTP limit is approximately 2 MiB per JSON body: [main.rs:145](crates/orderbook/src/main.rs:145).

**Failure scenario:** an unauthenticated attacker fills all listing slots. Alternatively, after paying one cheap intent PoW, it fills transcript message slots before the legitimate PCZT arrives or allocates hundreds of large messages per listing. Legitimate writes then remain rejected until expiry.

**Smallest fix:** add listing PoW and per-client quotas, authenticated transcript write tokens, `(direction, sequence, type)` uniqueness, message consumption/TTL, strict type-specific message sizes, and a process-wide byte budget. Apply external per-IP rate limits.

### 12. Medium: Chain parsing and indexing have several unbounded paths

The gRPC-web parser trusts server-provided frame lengths and repeatedly extends its buffer without a frame/response cap or timeout: [lwd.mjs:33-47](web/prototype/lib/lwd.mjs:33). Request chunks are bounded, but the total requested range is not: [lwd.mjs:57-63](web/prototype/lib/lwd.mjs:57).

`BrowserChain` keeps every observed txid and nullifier from the scan range in memory: [browser.rs:17-20](crates/carrier-wasm/src/browser.rs:17), [browser.rs:36-44](crates/carrier-wasm/src/browser.rs:36). R1 ancestry and package-hop loops lack the record walker’s explicit caps: [verify.rs:76](crates/carrier-core/src/verify.rs:76), [verify.rs:163](crates/carrier-core/src/verify.rs:163). Native loading permits a 64 MiB protobuf block and accumulates ranges into a `Vec`: [src/lib.rs:48](src/lib.rs:48), [src/lib.rs:93-118](src/lib.rs:93).

**Failure scenario:** a malicious LWD advertises an enormous frame or tip and exhausts browser memory; a hostile package forces excessive transaction fetches; ordinary long-term chain growth makes the browser index increasingly expensive.

**Smallest fix:** cap frame, transaction, block, range, hop and total-byte counts; use request deadlines; stream-process compact blocks; retain only relevant Ironwood data plus a bounded reorg window.

## 4. Deployment security

### 13. Medium: A fresh deployment does not contain the required WASM bundle

The verifier imports `./pkg/carrier_wasm.js`: [verify.mjs:3](web/prototype/verify.mjs:3), but `web/prototype/pkg` is ignored: [.gitignore:10](.gitignore:10). It is only generated by a separate script: [build_web.sh:3-11](scripts/build_web.sh:3). Compose mounts the source directory directly: [docker-compose.yml:73-75](deploy/docker-compose.yml:73), while the deployment instructions do not build WASM first: [deploy/README.md:31-35](deploy/README.md:31).

**Failure scenario:** a fresh-clone deployment returns 404 for the verifier module. Verification never becomes ready, and sale workflows wait or fail.

**Smallest fix:** build an immutable static-site image containing the WASM package, fail the image build if it is absent, and add a fresh-deployment smoke test.

### 14. Medium: Required browser integrity controls are absent

The approved specification requires release hashes, SRI and strict CSP: [marketplace/SPEC.md:171-177](docs/marketplace/SPEC.md:171). Caddy sets transport/isolation headers but no CSP: [Caddyfile:10-20](deploy/Caddyfile:10). Scripts have no integrity metadata: [index.html:26-27](web/prototype/index.html:26), and the demo includes inline script: [demo.html:45-56](web/prototype/demo.html:45).

**Failure scenario:** once the UI handles real seeds, signatures or PCZTs, a source-host compromise or future DOM injection can execute arbitrary JavaScript and steal those values.

**Smallest fix:** externalize inline code, deploy a strict CSP, publish a reproducible release manifest/hash, use SRI or CSP hashes, and test that no unexpected network origin is reachable.

### 15. Medium: Envoy’s LWD protection is global and does not bound expensive streams

The entire CompactTxStreamer prefix is exposed with no method allowlist and zero stream timeout: [envoy.yaml:23-27](deploy/envoy.yaml:23). Its token bucket is global rather than per client: [envoy.yaml:37-45](deploy/envoy.yaml:37).

**Failure scenario:** an attacker opens many long or oversized block-range streams, consuming node resources and the shared rate bucket. Honest browser requests are then denied.

**Smallest fix:** allowlist required read-only methods, enforce per-IP request/concurrency limits, cap block ranges and response bytes, and apply finite stream deadlines.

## What I checked and found sound

- HPKE uses the required X25519/HKDF-SHA256/ChaCha20-Poly1305 suite, correct info string, transcript-bound AAD and explicit size limits: [channel.rs:18-32](crates/carrier-core/src/channel.rs:18), [channel.rs:65-114](crates/carrier-core/src/channel.rs:65).
- The disclosure signature binds the protocol domain, transcript and complete disclosure payload and verifies against the disclosed FVK: [channel.rs:117-185](crates/carrier-core/src/channel.rs:117).
- `listing_id`, transcript encoding, PoW bit checking and the core replay/lease primitives are internally coherent: [channel.rs:45-85](crates/carrier-core/src/channel.rs:45), [channel.rs:187-218](crates/carrier-core/src/channel.rs:187).
- Collectibles seed derivation and vault authorization signatures are domain-separated and do not expose the spending key: [keys.rs:14-45](crates/carrier-core/src/keys.rs:14).
- Record decoding checks versions, exact lengths and flags. The main record walk has visited-node detection, hop/decrypted-byte caps, pointer validation and previous-nullifier binding: [lib.rs:69-116](crates/carrier-core/src/lib.rs:69), [verify.rs:240-293](crates/carrier-core/src/verify.rs:240), subject to the explicit `rho` gap above.
- R1 canonical issuance, R2 transfer shape and R3 holder decryption/liveness are otherwise checked: [verify.rs:66-141](crates/carrier-core/src/verify.rs:66), [verify.rs:157-215](crates/carrier-core/src/verify.rs:157).
- The buyer builder produces the expected three-Action shape and redacts output details before disclosure: [carrier-sale/src/lib.rs:79-171](crates/carrier-sale/src/lib.rs:79).
- Seller checks cover listing, expiry/branch, pool restrictions, exact Action count, carrier nullifier and exact payment: [carrier-sale/src/lib.rs:194-240](crates/carrier-sale/src/lib.rs:194). Extraction re-verifies proofs/signatures: [carrier-sale/src/lib.rs:242-260](crates/carrier-sale/src/lib.rs:242).
- I found no way for a malicious buyer to reduce payment after seller checking, or for a seller to alter the signed shielded effects without invalidating the final transaction.
- Deployment does not directly expose the internal order-book or lightwalletd containers, and the order-book runtime image is small and non-root: [Dockerfile:3-12](deploy/Dockerfile:3). State/seed files are excluded, and none were tracked in the inspected history.
- Origin-specific order-book CORS and COOP/COEP headers are present. I did not find an active untrusted-data XSS path in the current mock-only UI, though it is not yet hardened for real wallet data.

## Verification performed

I inspected every commit and relevant diff in `987d881..HEAD`, including the required documents and both earlier reviews.

Safe checks run:

- `git diff --check 987d881..HEAD`: only trailing blank-line warnings in `src/lib.rs` and `src/verifier.rs`.
- `bash -n verify.sh scripts/*.sh`: passed.
- `node --check` on the prototype and test modules: passed.
- `cargo fmt --all -- --check`: failed with widespread existing formatting differences.
- Repository/history searches confirmed no tracked state or seed files.
- Working tree remained clean.

I did not run `cargo build`, Rust tests or `verify.sh`: the workspace is read-only, those commands write build/evidence artifacts, and the full gate also requires live network access that was explicitly out of scope.

## Overall verdict

The atomic PCZT sale construction and cryptographic channel primitives are promising and I found no direct critical theft path under an honest chain view. However, the current project is **not ready to claim trustworthy public browser verification or robust public marketplace operation**.

A malicious or stale LWD can produce false `HELD` results, recovery can accept counterfeit provenance, and an unauthenticated client can cheaply disable the order book. Those are central correctness and availability properties, not presentation issues. Keep this classified as an experimental testnet PoC until the high-severity findings are fixed and covered by deterministic adversarial tests.

## Top five fixes

1. Implement fail-closed two-server browser verification and stop truncating native verification to the lower tip; add the explicit successor `rho` binding.
2. Require complete R1–R3 provenance and same-Action validation during recovery.
3. Add order-book authentication/quotas, global byte budgets, message consumption, and external per-client limits.
4. Make vault-index reservation atomic and implement authenticated, persistent lease renewal.
5. Build a complete immutable browser artifact with WASM, CSP/SRI/release hashes and two configured LWD endpoints; enforce zero-price testnet listings.