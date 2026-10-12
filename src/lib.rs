//! Shared helpers for the note-bound carrier PoC (Zcash testnet, Ironwood pool).
use anyhow::{anyhow, bail, Context, Result};
use incrementalmerkletree::{frontier::CommitmentTree, witness::IncrementalWitness};
use orchard::{
    note::ExtractedNoteCommitment,
    tree::{MerkleHashOrchard, MerklePath},
    Anchor,
};
use zcash_client_backend::proto::{
    compact_formats::CompactBlock,
    service::{compact_tx_streamer_client::CompactTxStreamerClient, BlockId, BlockRange, ChainSpec, TxFilter},
};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{BlockHeight, BranchId, TEST_NETWORK};

pub use carrier_core::*;

/// Trial-decrypts the Ironwood outputs of a transaction with `fvk` (external scope).
pub fn decrypt_ivk(tx: &Transaction, fvk: &orchard::keys::FullViewingKey) -> Vec<Found> {
    decrypt_bundle_ivk(tx.ironwood_bundle(), fvk)
}

/// Recovers the Ironwood outputs a transaction sent with `fvk`'s outgoing viewing key.
pub fn recover_ovk(tx: &Transaction, fvk: &orchard::keys::FullViewingKey) -> Vec<Found> {
    recover_bundle_ovk(tx.ironwood_bundle(), fvk)
}
pub mod verifier;

pub type Client = CompactTxStreamerClient<tonic::transport::Channel>;
pub const DEFAULT_LWD: &str = "https://testnet.zec.rocks:443";

/// The lightwalletd endpoint in use (`LWD`, default testnet.zec.rocks). Every chain fact this
/// crate reports is as served by this endpoint; see `verifier::cross_check` for a second source.
pub fn lwd_url() -> String {
    std::env::var("LWD").unwrap_or_else(|_| DEFAULT_LWD.to_string())
}

pub async fn connect() -> Result<Client> {
    connect_url(&lwd_url()).await
}

pub async fn connect_url(url: &str) -> Result<Client> {
    let ep = tonic::transport::Channel::from_shared(url.to_string())?;
    // TLS for https:// URLs; plain http:// only for a lightwalletd on a private network
    // (deploy/: the order book talks to its own node inside the compose network).
    let ep = if url.starts_with("https://") { ep.tls_config(tonic::transport::ClientTlsConfig::new().with_webpki_roots())? } else { ep };
    let ch = ep.connect().await.context("lightwalletd connect")?;
    Ok(CompactTxStreamerClient::new(ch).max_decoding_message_size(64 << 20))
}

// ---------- bounded input ----------
/// Largest file this crate reads from another party or a published package.
pub const MAX_FILE_BYTES: u64 = 1 << 20;

/// Reads at most `MAX_FILE_BYTES` (checked while reading, not before, so a growing file cannot
/// slip past the limit).
pub fn read_capped(p: &std::path::Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = vec![];
    std::fs::File::open(p)
        .with_context(|| p.display().to_string())?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_FILE_BYTES {
        bail!("{} is over the {MAX_FILE_BYTES} byte limit", p.display());
    }
    Ok(buf)
}

// ---------- chain access ----------
pub async fn tip(c: &mut Client) -> Result<u64> {
    Ok(c.get_latest_block(ChainSpec {}).await?.into_inner().height)
}

/// Returns (tx, mined height or None).
/// The server's height for a tx proves nothing by itself: require the txid in the compact block
/// at that height.
pub async fn included_at(c: &mut Client, txid: &str, h: u64) -> Result<()> {
    let b = blocks(c, h, h).await?;
    if b.iter().flat_map(|b| &b.vtx).any(|t| txid_hex(&t.txid) == txid) {
        Ok(())
    } else {
        bail!("{txid} is not in the compact block at height {h}")
    }
}

pub async fn get_tx(c: &mut Client, txid: &str) -> Result<(Transaction, Option<u64>)> {
    let raw = c
        .get_transaction(TxFilter { block: None, index: 0, hash: txid_bytes(txid)?.to_vec() })
        .await?
        .into_inner();
    let mined = if raw.height == 0 || raw.height > u32::MAX as u64 { None } else { Some(raw.height) };
    let h = mined.unwrap_or(tip(c).await? + 1);
    let branch = BranchId::for_height(&TEST_NETWORK, BlockHeight::from_u32(h as u32));
    let tx = Transaction::read(&raw.data[..], branch).context("tx parse")?;
    // Do not trust the server to return the transaction we asked for.
    let got = tx.txid().to_string();
    if got != txid {
        bail!("lightwalletd returned tx {got} for requested {txid}");
    }
    Ok((tx, mined))
}

pub async fn blocks(c: &mut Client, start: u64, end: u64) -> Result<Vec<CompactBlock>> {
    let mut s = c
        .get_block_range(BlockRange {
            start: Some(BlockId { height: start, hash: vec![] }),
            end: Some(BlockId { height: end, hash: vec![] }),
            pool_types: vec![],
        })
        .await?
        .into_inner();
    let mut out: Vec<CompactBlock> = vec![];
    while let Some(b) = s.message().await? {
        // The range must be contiguous and hash-linked. This catches a server that splices or
        // drops blocks inside one response; it does not validate proof of work.
        if let Some(p) = out.last() {
            if b.height != p.height + 1 || b.prev_hash != p.hash {
                bail!("lightwalletd block range not linked at height {}", b.height);
            }
        } else if b.height != start {
            bail!("lightwalletd block range starts at {} not {start}", b.height);
        }
        out.push(b);
    }
    if end >= start && out.last().map(|b| b.height) != Some(end) {
        bail!("lightwalletd block range {start}..={end} incomplete");
    }
    Ok(out)
}

pub async fn ironwood_tree(c: &mut Client, h: u64) -> Result<CommitmentTree<MerkleHashOrchard, 32>> {
    Ok(c.get_tree_state(BlockId { height: h, hash: vec![] }).await?.into_inner().ironwood_tree()?)
}

fn node(cmx: &[u8]) -> Result<MerkleHashOrchard> {
    let b: [u8; 32] = cmx.try_into().map_err(|_| anyhow!("cmx len"))?;
    let c = Option::from(ExtractedNoteCommitment::from_bytes(&b)).ok_or_else(|| anyhow!("cmx"))?;
    Ok(MerkleHashOrchard::from_cmx(&c))
}

/// Builds a witness for `cmx` (mined at `note_h`) against the Ironwood root at `anchor_h`,
/// and cross-checks our computed root against the server's treestate.
pub async fn witness(c: &mut Client, cmx: &[u8; 32], note_h: u64, anchor_h: u64) -> Result<(MerklePath, Anchor)> {
    let mut tree = ironwood_tree(c, note_h.checked_sub(1).ok_or_else(|| anyhow!("note height 0"))?).await?;
    let mut w: Option<IncrementalWitness<MerkleHashOrchard, 32>> = None;
    for b in blocks(c, note_h, anchor_h).await? {
        for tx in &b.vtx {
            for a in &tx.ironwood_actions {
                let n = node(&a.cmx)?;
                match w.as_mut() {
                    Some(w) => w.append(n).map_err(|_| anyhow!("witness full"))?,
                    None => {
                        tree.append(n).map_err(|_| anyhow!("tree full"))?;
                        if a.cmx.as_slice() == cmx {
                            w = IncrementalWitness::from_tree(tree.clone());
                        }
                    }
                }
            }
        }
    }
    let w = w.ok_or_else(|| anyhow!("note cmx not found in blocks {note_h}..={anchor_h}"))?;
    let expected = ironwood_tree(c, anchor_h).await?.root();
    if w.root() != expected {
        bail!("computed Ironwood root != server treestate at {anchor_h}");
    }
    Ok((w.path().ok_or_else(|| anyhow!("path"))?.into(), Anchor::from(w.root())))
}

