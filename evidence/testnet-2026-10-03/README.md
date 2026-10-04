# Testnet run 2026-10-03: proof of work at 20 bits (DRY)

Nothing was broadcast.

- `orderbook_e2e_20bit_dry.log`: `scripts/orderbook_e2e.sh` against the order book rebuilt with `channel::POW_BITS = 20` (approved 2026-10-03). The seller is the buyer seed's vault 17 (sealed-collection item #1, hop 1), the buyer the main seed; the disclosure carried no package, so the buyer verified from the vault record alone. Extraction succeeded (DRY).
- The order book refuses a stamp that meets 16 bits but not 20 (unit test `crates/orderbook`, run by `verify.sh`). An earlier run the same day reached a server still built at 16 bits; the binary is now rebuilt and that run is not counted.
