# zypher testnet node: runbook

Marketplace SPEC P11 and 6.2: a lightwalletd we run ourselves, on our own Zebra node, so the browser and the CLI can compare two independent servers (ours and `testnet.zec.rocks`). The same server hosts the order book and the static site. Testnet only.

Decided 2026-10-03 (Ryot): one VPS at Hetzner.

## What runs

| Service | Image | Reachable from |
|---|---|---|
| Zebra (testnet full node) | `zfnd/zebra:6.4.2` | P2P port 18233 from the internet; RPC inside the compose network only |
| lightwalletd | `electriccoinco/lightwalletd:v0.5.4` | inside the compose network only |
| Envoy (grpc-web and gRPC) | `envoyproxy/envoy:v1.39.2` | via Caddy at `lwd.DOMAIN` |
| order book | built from `crates/orderbook` | via Caddy at `api.DOMAIN` |
| Caddy (TLS, static site) | `caddy:2.11.6` | ports 80 and 443 |

`testnet.zec.rocks` reported Zebra 6.3.0 with lightwalletd v0.5.4 on 2026-10-03 (`GetLightdInfo`). No keys live on this server: the order book holds public listings and ciphertext only.

## Server

Zebra recommends 4 CPU cores and 16 GB RAM, about 10 GB of disk for testnet state, and roughly 300 GB of traffic a month (Zebra book, "System Requirements", read 2026-10-03). The images are amd64 only, so no ARM plan.

Pick a shared-vCPU amd64 plan with at least 4 vCPU, 16 GB RAM and 160 GB SSD, Ubuntu 24.04. Price and plan names: check Hetzner's current list (not verified here; on the order of 30 EUR a month). An EU location is the cheapest.

## Steps (Ryot)

1. Create the Hetzner Cloud project and the server above. Add your SSH key. Add a firewall: TCP 22 from your IP only; TCP 80 and 443; UDP 443; TCP 18233.
2. DNS: point three A records at the server: `DOMAIN`, `api.DOMAIN`, `lwd.DOMAIN`. Any domain you own works for testnet; the production domain is a separate decision (SPEC D9).
3. On the server (as a sudo user, not root):
   ```bash
   sudo apt-get update && sudo apt-get install -y docker.io docker-compose-v2 git
   sudo usermod -aG docker "$USER"   # log out and back in
   git clone git@github.com:Reiunmute/zcash-carrier.git && cd zcash-carrier
   cp deploy/.env.example deploy/.env && nano deploy/.env
   docker compose -f deploy/docker-compose.yml up -d --build
   ```
   The repo is private: give the server a read-only deploy key. A clone never contains `state/` (git-ignored), and `.dockerignore` keeps it out of image builds.
4. Watch the sync: `docker compose -f deploy/docker-compose.yml logs -f zebra lightwalletd`. Testnet sync time is not measured yet.

## Checks once synced

From this repo on a laptop:
```bash
LWD=https://lwd.DOMAIN:443 ./target/release/verify mined e96845ecfc84008674195d8ee77fd433cdb7cdfc3464711ae92221d58ada3b1d
LWD2=https://lwd.DOMAIN:443 bash verify.sh
```
The second line is the P11 exit check: every block and tx the verdicts rely on must be identical on `testnet.zec.rocks` and our node, and the trust line must name both.

Browser: `node research/stage1-checks/stream_check.mjs` with its URL set to `https://lwd.DOMAIN` (grpc-web streaming and CORS).

Order book: `curl https://api.DOMAIN/tip`.

## Limits

- Rate limiting: Envoy has one global token bucket (100 requests a second, burst 200); Caddy and the order book have none per client. Add per-IP limits before heavy public use.
- The order book is in memory; listings are re-posted by sellers after a restart.
- Zebra RPC runs without cookie auth; it is not published outside the compose network.
- Not yet tested end to end: the configs were syntax-checked with `docker compose config` only. `envoy --mode validate`, `caddy validate` and the order book image build need the images pulled.
