//! Shared helpers for the note-bound carrier PoC (Zcash testnet, Ironwood pool).
use anyhow::{anyhow, bail, Context, Result};
use incrementalmerkletree::{frontier::CommitmentTree, witness::IncrementalWitness};
use orchard::{
    keys::{FullViewingKey, PreparedIncomingViewingKey, Scope, SpendingKey},
    note::{ExtractedNoteCommitment, RandomSeed, Rho},
    note_encryption::IronwoodDomain,
    tree::{MerkleHashOrchard, MerklePath},
    value::NoteValue,
    Address, Anchor, Note, NoteVersion,
};
use serde::{Deserialize, Serialize};
use zcash_client_backend::proto::{
    compact_formats::CompactBlock,
    service::{compact_tx_streamer_client::CompactTxStreamerClient, BlockId, BlockRange, ChainSpec, TxFilter},
};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{BlockHeight, BranchId, TEST_NETWORK};

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
    let ch = tonic::transport::Channel::from_shared(url.to_string())?
        .tls_config(tonic::transport::ClientTlsConfig::new().with_webpki_roots())?
        .connect()
        .await
        .context("lightwalletd connect")?;
    Ok(CompactTxStreamerClient::new(ch).max_decoding_message_size(64 << 20))
}

// ---------- memo format (ZIP 302 arbitrary-data memo, lead byte 0xF5) ----------
pub const MAGIC: &[u8; 4] = b"ZCR1";
pub const K_SEQ: u8 = 1;
pub const K_CARRIER: u8 = 2;
pub const K_HOP: u8 = 3;
pub const K_FINAL: u8 = 4;

#[derive(Clone, Debug)]
pub struct Memo {
    pub kind: u8,
    pub coll: [u8; 32],
    pub num: u32,
    pub hop: u32,
    pub art: Vec<u8>,
}

pub fn encode_memo(kind: u8, coll: &[u8; 32], num: u32, hop: u32, art: &[u8]) -> [u8; 512] {
    let mut m = [0u8; 512];
    m[0] = 0xF5;
    m[1..5].copy_from_slice(MAGIC);
    m[5] = kind;
    m[6..38].copy_from_slice(coll);
    m[38..42].copy_from_slice(&num.to_le_bytes());
    m[42..46].copy_from_slice(&hop.to_le_bytes());
    assert!(art.len() <= 512 - 48);
    m[46..48].copy_from_slice(&(art.len() as u16).to_le_bytes());
    m[48..48 + art.len()].copy_from_slice(art);
    m
}

pub fn decode_memo(m: &[u8; 512]) -> Option<Memo> {
    if m[0] != 0xF5 || &m[1..5] != MAGIC {
        return None;
    }
    let len = u16::from_le_bytes([m[46], m[47]]) as usize;
    if 48 + len > 512 {
        return None;
    }
    Some(Memo {
        kind: m[5],
        coll: m[6..38].try_into().unwrap(),
        num: u32::from_le_bytes(m[38..42].try_into().unwrap()),
        hop: u32::from_le_bytes(m[42..46].try_into().unwrap()),
        art: m[48..48 + len].to_vec(),
    })
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

// ---------- FINAL hard seal (OPEN_ISSUES #2) ----------
pub const SEAL_DOMAIN: &str = "carrier:final-seal:v1";
const SEAL_DIVERSIFIER: &[u8; 11] = b"carrierseal";

/// An Orchard-protocol address whose spending key nobody knows. Its transmission key `pk_d` is a
/// hash-to-curve point of the collection id, so nobody knows an `ivk` with `pk_d = [ivk] g_d`.
/// A FINAL note sent here can never be spent, so the sequence chain can never continue. Anyone
/// can recompute the address from `coll`.
pub fn seal_address(coll: &[u8; 32]) -> Address {
    use group::GroupEncoding;
    use pasta_curves::arithmetic::CurveExt;
    let hasher = pasta_curves::pallas::Point::hash_to_curve(SEAL_DOMAIN);
    for ctr in 0u32.. {
        let mut msg = coll.to_vec();
        msg.extend_from_slice(&ctr.to_le_bytes());
        let mut raw = [0u8; 43];
        raw[..11].copy_from_slice(SEAL_DIVERSIFIER);
        raw[11..].copy_from_slice(&hasher(&msg).to_bytes());
        if let Some(a) = Option::from(Address::from_raw_address_bytes(&raw)) {
            return a; // fails only for the identity point, which hash_to_curve practically never yields
        }
    }
    unreachable!()
}

// ---------- keys / notes ----------
pub fn spending_key(seed: &[u8; 32], account: u32) -> SpendingKey {
    SpendingKey::from_zip32_seed(seed, 1, zip32::AccountId::try_from(account).unwrap()).unwrap()
}

pub fn fvk_hex(fvk: &FullViewingKey) -> String {
    hex::encode(fvk.to_bytes())
}

pub fn fvk_from_hex(s: &str) -> Result<FullViewingKey> {
    let b: [u8; 96] = hex::decode(s)?.try_into().map_err(|_| anyhow!("fvk len"))?;
    FullViewingKey::from_bytes(&b).ok_or_else(|| anyhow!("bad fvk"))
}

pub fn ua(addr: &Address) -> String {
    use zcash_address::unified::{self, Encoding};
    let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(addr.to_raw_address_bytes())]).unwrap();
    ua.encode(&zcash_protocol::consensus::NetworkType::Test)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NoteJson {
    pub recipient: String,
    pub value: u64,
    pub rho: String,
    pub rseed: String,
}

pub fn note_to_json(n: &Note) -> NoteJson {
    NoteJson {
        recipient: hex::encode(n.recipient().to_raw_address_bytes()),
        value: n.value().inner(),
        rho: hex::encode(n.rho().to_bytes()),
        rseed: hex::encode(n.rseed().as_bytes()),
    }
}

fn arr<const N: usize>(s: &str) -> Result<[u8; N]> {
    hex::decode(s)?.try_into().map_err(|_| anyhow!("hex length"))
}

pub fn note_from_json(j: &NoteJson) -> Result<Note> {
    let recipient = Option::from(Address::from_raw_address_bytes(&arr::<43>(&j.recipient)?)).ok_or_else(|| anyhow!("addr"))?;
    let rho: Rho = Option::from(Rho::from_bytes(&arr::<32>(&j.rho)?)).ok_or_else(|| anyhow!("rho"))?;
    let rseed = Option::from(RandomSeed::from_bytes(arr::<32>(&j.rseed)?, &rho)).ok_or_else(|| anyhow!("rseed"))?;
    Option::from(Note::from_parts(recipient, NoteValue::from_raw(j.value), rho, rseed, NoteVersion::V3))
        .ok_or_else(|| anyhow!("note"))
}

pub fn cmx_of(n: &Note) -> [u8; 32] {
    ExtractedNoteCommitment::from(n.commitment()).to_bytes()
}

// ---------- chain access ----------
pub fn txid_bytes(hex_display: &str) -> Result<[u8; 32]> {
    let mut b = arr::<32>(hex_display)?;
    b.reverse();
    Ok(b)
}

pub fn txid_hex(protocol_order: &[u8]) -> String {
    let mut b = protocol_order.to_vec();
    b.reverse();
    hex::encode(b)
}

pub async fn tip(c: &mut Client) -> Result<u64> {
    Ok(c.get_latest_block(ChainSpec {}).await?.into_inner().height)
}

/// Returns (tx, mined height or None).
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

// ---------- decryption ----------
pub struct Found {
    pub action: usize,
    pub note: Note,
    pub memo: [u8; 512],
    pub cmx: [u8; 32],
    pub spent_nf: [u8; 32],
}

pub fn decrypt_ivk(tx: &Transaction, fvk: &FullViewingKey) -> Vec<Found> {
    let pivk = PreparedIncomingViewingKey::new(&fvk.to_ivk(Scope::External));
    let mut out = vec![];
    if let Some(b) = tx.ironwood_bundle() {
        for (i, a) in b.actions().iter().enumerate() {
            let d = IronwoodDomain::for_action(a);
            if let Some((note, _, memo)) = zcash_note_encryption::try_note_decryption(&d, &pivk, a) {
                out.push(Found { action: i, note, memo, cmx: a.cmx().to_bytes(), spent_nf: a.nullifier().to_bytes() });
            }
        }
    }
    out
}

pub fn recover_ovk(tx: &Transaction, fvk: &FullViewingKey) -> Vec<Found> {
    let ovk = fvk.to_ovk(Scope::External);
    let mut out = vec![];
    if let Some(b) = tx.ironwood_bundle() {
        for (i, a) in b.actions().iter().enumerate() {
            let d = IronwoodDomain::for_action(a);
            if let Some((note, _, memo)) = zcash_note_encryption::try_output_recovery_with_ovk(
                &d,
                &ovk,
                a,
                a.cv_net(),
                &a.encrypted_note().out_ciphertext,
            ) {
                out.push(Found { action: i, note, memo, cmx: a.cmx().to_bytes(), spent_nf: a.nullifier().to_bytes() });
            }
        }
    }
    out
}

// ---------- published artefacts ----------
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Collection {
    pub network: String,
    pub coll: String,
    pub issuer_fvk: String,
    pub genesis_txid: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Hop {
    pub note: NoteJson,
    pub vault_fvk: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Package {
    pub item: u32,
    pub label: String,
    pub hops: Vec<Hop>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_address_is_deterministic_and_per_collection() {
        let (a, b) = ([7u8; 32], [8u8; 32]);
        assert_eq!(seal_address(&a).to_raw_address_bytes(), seal_address(&a).to_raw_address_bytes());
        assert_ne!(seal_address(&a).to_raw_address_bytes(), seal_address(&b).to_raw_address_bytes());
        // not the address of any key we derive
        let fvk = FullViewingKey::from(&spending_key(&[1u8; 32], 0));
        assert_eq!(fvk.scope_for_address(&seal_address(&a)), None);
    }

    #[test]
    fn memo_round_trip() {
        let m = encode_memo(K_HOP, &[3u8; 32], 5, 2, &[]);
        let d = decode_memo(&m).unwrap();
        assert_eq!((d.kind, d.coll, d.num, d.hop), (K_HOP, [3u8; 32], 5, 2));
    }
}
