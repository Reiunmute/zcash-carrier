//! The verification procedure, shared by the `verify` CLI and the buyer side of `carrier sale-buy`.
//!
//!  R1 issuance set  = carriers in the genesis tx + carriers in txs that advance the sequence
//!                     chain (a spend of the live sequence note bound to its successor in the
//!                     SAME action), up to a FINAL memo. Each issuance tx has exactly one sequence
//!                     note, and sequence notes and carriers have value 0. Carriers have hop 0.
//!                     A FINAL sent to `seal_address(coll)` is a hard seal (OPEN_ISSUES #2).
//!  R2 transfer hop  = spend of the current carrier note, with the successor output in the SAME
//!                     action (successor rho == spent nullifier, enforced by the Action circuit).
//!                     The successor must have value 0 and a HOP memo for the same collection and
//!                     item, with hop = previous hop + 1. Otherwise the item is burned.
//!  R3 holder        = the holder key opens the current carrier and its nullifier has not
//!                     appeared on chain. A valid chain whose current carrier is already spent is
//!                     reported as SPENT, not as held.
//!
//! Trust: chain data comes from lightwalletd (`LWD`). We check that returned txids match, that
//! block ranges are hash-linked, and that full txs agree with the compact data they were indexed
//! from. With `LWD2`, every compact block in the scanned range (txids and Ironwood actions) and
//! every fetched transaction must be identical on both servers, so one server cannot hide a spend
//! the other serves. None of this validates proof of work.
use crate::*;
use anyhow::{anyhow, bail, Result};
use orchard::keys::FullViewingKey;
use std::collections::{BTreeSet, HashMap};
use zcash_primitives::transaction::Transaction;

/// Where a nullifier was revealed, as indexed from compact blocks.
#[derive(Clone, Debug)]
pub struct NfHit {
    pub txid: String,
    pub height: u64,
    pub action: usize,
    cmx: [u8; 32],
}

pub struct Chain {
    pub c: Client,
    /// Second, independent server (`LWD2`). When set, all chain data is compared with it.
    c2: Option<(Client, String)>,
    nf_index: HashMap<[u8; 32], NfHit>,
    pub from: u64,
    pub to: u64,
    /// Heights of every block whose data a verdict relied on.
    pub used_heights: BTreeSet<u64>,
    txs_compared: usize,
}

/// The parts of a compact block a verdict depends on: hash links, txids and Ironwood actions.
fn block_view(b: &zcash_client_backend::proto::compact_formats::CompactBlock) -> (u64, Vec<u8>, Vec<u8>, Vec<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>)>) {
    let txs = b
        .vtx
        .iter()
        .map(|t| (t.txid.clone(), t.ironwood_actions.iter().map(|a| (a.nullifier.clone(), a.cmx.clone(), a.ephemeral_key.clone(), a.ciphertext.clone())).collect()))
        .collect();
    (b.height, b.hash.clone(), b.prev_hash.clone(), txs)
}

impl Chain {
    /// Indexes every Ironwood nullifier revealed in `from..=tip`. With `LWD2`, the range ends at the
    /// lower of the two tips and every block must be identical on both servers.
    pub async fn index(mut c: Client, from: u64) -> Result<Self> {
        let mut c2 = match std::env::var("LWD2") {
            Ok(url) => Some((connect_url(&url).await?, url)),
            Err(_) => None,
        };
        let mut to = tip(&mut c).await?;
        if let Some((c2, _)) = c2.as_mut() {
            to = to.min(tip(c2).await?);
        }
        let bs = blocks(&mut c, from, to).await?;
        if let Some((c2, url)) = c2.as_mut() {
            let bs2 = blocks(c2, from, to).await?;
            for (a, b) in bs.iter().zip(&bs2) {
                if block_view(a) != block_view(b) {
                    bail!("cross-check: {url} serves different data than {} at height {}", lwd_url(), a.height);
                }
            }
        }
        let mut nf_index = HashMap::new();
        for b in bs {
            for tx in &b.vtx {
                for (i, a) in tx.ironwood_actions.iter().enumerate() {
                    let nf: [u8; 32] = a.nullifier.clone().try_into().map_err(|_| anyhow!("compact nullifier length"))?;
                    let cmx: [u8; 32] = a.cmx.clone().try_into().map_err(|_| anyhow!("compact cmx length"))?;
                    let hit = NfHit { txid: txid_hex(&tx.txid), height: b.height, action: i, cmx };
                    if nf_index.insert(nf, hit).is_some() {
                        bail!("lightwalletd served a duplicate nullifier at height {}", b.height);
                    }
                }
            }
        }
        Ok(Chain { c, c2, nf_index, from, to, used_heights: BTreeSet::from([to]), txs_compared: 0 })
    }

    pub async fn tx(&mut self, txid: &str) -> Result<(Transaction, u64)> {
        let (t, h) = get_tx(&mut self.c, txid).await?;
        let h = h.ok_or_else(|| anyhow!("{txid} unmined"))?;
        if let Some((c2, url)) = self.c2.as_mut() {
            let (t2, h2) = get_tx(c2, txid).await?;
            let (mut a, mut b) = (vec![], vec![]);
            t.write(&mut a)?;
            t2.write(&mut b)?;
            if h2 != Some(h) || a != b {
                bail!("cross-check: {url} serves tx {txid} differently (height {h2:?} vs {h})");
            }
            self.txs_compared += 1;
        }
        self.used_heights.insert(h);
        Ok((t, h))
    }

    pub fn spent_at(&self, nf: &[u8; 32]) -> Option<&NfHit> {
        self.nf_index.get(nf)
    }

    /// Fetches the tx that revealed `nf` and checks it against the compact data it was indexed from.
    pub async fn spend_tx(&mut self, hit: &NfHit, nf: &[u8; 32]) -> Result<Transaction> {
        let (tx, h) = self.tx(&hit.txid).await?;
        let a = tx
            .ironwood_bundle()
            .and_then(|b| b.actions().get(hit.action))
            .ok_or_else(|| anyhow!("spend tx {} has no action {}", hit.txid, hit.action))?;
        if h != hit.height || a.nullifier().to_bytes() != *nf || a.cmx().to_bytes() != hit.cmx {
            bail!("full tx {} disagrees with the compact block it was indexed from", hit.txid);
        }
        Ok(tx)
    }
}

/// Prints what the verdict's chain data rests on. The comparison itself happens in `Chain::index`
/// and `Chain::tx`, which fail on any difference between the two servers.
pub async fn cross_check(ch: &mut Chain) -> Result<()> {
    match &ch.c2 {
        None => println!("trust: chain data from one lightwalletd ({}), not cross-checked (set LWD2)", lwd_url()),
        Some((_, url)) => println!(
            "trust: compact blocks {}..={} and {} full tx(s) identical on {} and {url}",
            ch.from,
            ch.to,
            ch.txs_compared,
            lwd_url()
        ),
    }
    Ok(())
}

/// Artwork payload v0 (docs/SPEC.md): 48-byte palette + 338 bytes of 4-bit pixels.
pub const ART_V0_LEN: usize = 386;

pub struct Issued {
    pub num: u32,
    pub cmx: [u8; 32],
    pub art: Vec<u8>,
    pub txid: String,
    pub height: u64,
    pub action: usize,
}

pub struct IssuanceSet {
    pub items: Vec<Issued>,
    pub finalized: bool,
    /// FINAL was sent to the collection's seal address: nobody can continue the sequence.
    pub sealed: bool,
}

fn coll_of(col: &Collection) -> Result<[u8; 32]> {
    hex::decode(&col.coll)?.try_into().map_err(|_| anyhow!("collection id must be 32 bytes"))
}

/// R1. Walks the issuer's sequence chain from the genesis tx to FINAL.
pub async fn issuance_set(ch: &mut Chain, col: &Collection) -> Result<IssuanceSet> {
    let issuer = fvk_from_hex(&col.issuer_fvk)?;
    let coll = coll_of(col)?;
    let mut items = vec![];
    let mut txid = col.genesis_txid.clone();
    let mut expect_rho: Option<[u8; 32]> = None; // successor rho the next sequence note must have
    let mut finalized = false;
    let mut sealed = false;
    let mut step: u32 = 0;
    loop {
        let (tx, h) = ch.tx(&txid).await?;
        // Sequence notes the issuer holds (incoming key), plus a FINAL sent to the collection's seal
        // address, which only the issuer's outgoing key can recover (OPEN_ISSUES #2).
        let seal = seal_address(&coll);
        let mut seqs: Vec<(Found, Memo)> = decrypt_ivk(&tx, &issuer)
            .into_iter()
            .chain(recover_ovk(&tx, &issuer).into_iter().filter(|f| f.note.recipient() == seal))
            .filter_map(|f| decode_memo(&f.memo).map(|m| (f, m)))
            .filter(|(f, m)| m.coll == coll && (m.kind == K_SEQ || m.kind == K_FINAL) && (m.kind == K_FINAL || f.note.recipient() != seal))
            .collect();
        seqs.dedup_by(|a, b| a.0.cmx == b.0.cmx);
        if seqs.len() != 1 {
            bail!("R1: {} sequence notes in {txid}, expected exactly one", seqs.len());
        }
        let (seq, sm) = seqs.remove(0);
        if seq.note.value().inner() != 0 {
            bail!("R1: sequence note in {txid} has value {}", seq.note.value().inner());
        }
        // Canonical sequence memos: empty payload, step counter 0, 1, 2, ... in `hop`; SEQ has num 0.
        if !sm.art.is_empty() || sm.hop != step || (sm.kind == K_SEQ && sm.num != 0) {
            bail!("R1: non-canonical sequence memo in {txid} (step {}, expected {step}, num {}, payload {}B)", sm.hop, sm.num, sm.art.len());
        }
        step = step.checked_add(1).ok_or_else(|| anyhow!("R1: sequence too long"))?;
        if let Some(r) = expect_rho {
            if !(seq.note.rho().to_bytes() == r && seq.spent_nf == r) {
                bail!("R1: sequence successor in {txid} not bound in the same action");
            }
        }
        // carriers created in this issuance step: outputs recovered with the issuer ovk
        for f in recover_ovk(&tx, &issuer) {
            let Some(m) = decode_memo(&f.memo) else { continue };
            if m.kind != K_CARRIER || m.coll != coll {
                continue;
            }
            let v = f.note.value().inner();
            if v != 0 || m.hop != 0 || m.num == 0 || m.art.len() != ART_V0_LEN {
                println!("R1 REJECT carrier #{} in {}..: value {v} hop {} art {}B (need value 0, hop 0, item >= 1, {ART_V0_LEN}B v0 art)", m.num, &txid[..12], m.hop, m.art.len());
                continue;
            }
            items.push(Issued { num: m.num, cmx: f.cmx, art: m.art.clone(), txid: txid.clone(), height: h, action: f.action });
        }
        if sm.kind == K_FINAL {
            // FINAL names the number of items issued; every item number is in 1..=that.
            if sm.num as usize != items.len() || items.iter().any(|i: &Issued| i.num > sm.num) {
                bail!("R1: FINAL in {txid} says {} items, the sequence issued {}", sm.num, items.len());
            }
            finalized = true;
            sealed = seq.note.recipient() == seal;
            break;
        }
        let nf = seq.note.nullifier(&issuer).to_bytes();
        match ch.spent_at(&nf) {
            None => break, // open sequence: the set may still grow
            Some(hit) => {
                expect_rho = Some(nf);
                txid = hit.txid.clone();
            }
        }
    }
    let mut nums: Vec<u32> = items.iter().map(|i| i.num).collect();
    nums.sort();
    if nums.windows(2).any(|w| w[0] == w[1]) {
        bail!("R1 duplicate item numbers in issuance set");
    }
    Ok(IssuanceSet { items, finalized, sealed })
}

/// Outcome of R2 and R3 for one item.
pub enum Verdict {
    /// The holder opens the current carrier, it is a valid carrier, and it is unspent.
    Held { cur: Found, txid: String, height: u64, hop: u32 },
    /// The chain is valid up to the holder's carrier, but that carrier has already been spent.
    Spent { cmx: [u8; 32] },
    /// The item was burned at `hop`. Later hops are ignored.
    Burned { hop: usize, why: String },
    /// No holder key was given: the last successor's validity and liveness are unverified.
    Unverified { cmx: [u8; 32] },
}

/// R2 and R3 for `pkg`, against an issuance set from `issuance_set`.
pub async fn verify_item(ch: &mut Chain, col: &Collection, set: &IssuanceSet, pkg: &Package, holder: Option<&FullViewingKey>) -> Result<Verdict> {
    let coll = coll_of(col)?;
    let issued = set.items.iter().find(|i| i.num == pkg.item).ok_or_else(|| anyhow!("item #{} not in the issuance set", pkg.item))?;
    let mut cur_cmx = issued.cmx;
    // Where the current carrier was created: (txid, height, action).
    let mut at = (issued.txid.clone(), issued.height, issued.action);
    for (k, hop) in pkg.hops.iter().enumerate() {
        let vfvk = fvk_from_hex(&hop.vault_fvk)?;
        if cmx_of(&note_from_json(&hop.note)?) != cur_cmx {
            bail!("hop {k}: disclosed note does not open the current carrier commitment");
        }
        // Open the current carrier from chain with this hop's vault key. The incoming viewing key
        // derives the note's address, so success also binds the key to the note.
        let (ctx, _) = ch.tx(&at.0).await?;
        let f = decrypt_ivk(&ctx, &vfvk)
            .into_iter()
            .find(|f| f.action == at.2 && f.cmx == cur_cmx)
            .ok_or_else(|| anyhow!("hop {k}: disclosed vault key cannot open the current carrier"))?;
        // R2 validity of the successor created by the previous hop.
        if k > 0 {
            if let Err(why) = successor_valid(&f, &coll, pkg.item, k as u32) {
                return Ok(burned(pkg.item, k - 1, why));
            }
            println!("R2 hop {}: successor valid (HOP, value 0)", k - 1);
        }
        let nf = f.note.nullifier(&vfvk).to_bytes();
        let hit = ch.spent_at(&nf).cloned().ok_or_else(|| anyhow!("hop {k}: carrier not spent on chain"))?;
        let tx = ch.spend_tx(&hit, &nf).await?;
        cur_cmx = tx.ironwood_bundle().ok_or_else(|| anyhow!("no Ironwood bundle"))?.actions()[hit.action].cmx().to_bytes();
        println!("R2 hop {k}: spent in {}..@{}, successor cmx {}.. (same action)", &hit.txid[..12], hit.height, &hex::encode(cur_cmx)[..12]);
        at = (hit.txid.clone(), hit.height, hit.action);
    }
    println!("== current carrier cmx {}..", &hex::encode(cur_cmx)[..12]);
    let n = pkg.hops.len();
    let Some(hf) = holder else {
        if n > 0 {
            println!("R2 hop {}: successor validity unverified (needs holder key)", n - 1);
        }
        println!("R3 holder check skipped (needs holder key)");
        return Ok(Verdict::Unverified { cmx: cur_cmx });
    };
    let (ctx, _) = ch.tx(&at.0).await?;
    let mine = decrypt_ivk(&ctx, hf)
        .into_iter()
        .find(|f| f.action == at.2 && f.cmx == cur_cmx)
        .ok_or_else(|| anyhow!("holder cannot open current carrier"))?;
    if n > 0 {
        if let Err(why) = successor_valid(&mine, &coll, pkg.item, n as u32) {
            return Ok(burned(pkg.item, n - 1, why));
        }
        println!("R2 hop {}: successor valid (HOP, value 0)", n - 1);
    }
    let live = ch.spent_at(&mine.note.nullifier(hf).to_bytes()).is_none();
    println!("R3 holder opens current carrier: yes, unspent={live}");
    if !live {
        return Ok(Verdict::Spent { cmx: cur_cmx });
    }
    Ok(Verdict::Held { cur: mine, txid: at.0, height: at.1, hop: n as u32 })
}

/// OPEN_ISSUES #7: a successor is a valid carrier only if it has value 0 and a HOP memo for the
/// same collection and item, with the hop counter one more than the note it replaces.
pub fn successor_valid(f: &Found, coll: &[u8; 32], item: u32, hop: u32) -> std::result::Result<(), String> {
    let v = f.note.value().inner();
    if v != 0 {
        return Err(format!("value {v} != 0"));
    }
    let m = decode_memo(&f.memo).ok_or_else(|| "memo is not a carrier memo".to_string())?;
    if m.kind != K_HOP {
        return Err(format!("memo kind {} != HOP", m.kind));
    }
    if m.coll != *coll {
        return Err("memo collection differs".into());
    }
    if m.num != item {
        return Err(format!("memo item #{} != #{item}", m.num));
    }
    if m.hop != hop {
        return Err(format!("memo hop {} != {hop}", m.hop));
    }
    Ok(())
}

fn burned(item: u32, hop: usize, why: String) -> Verdict {
    println!("R2 hop {hop}: successor invalid ({why})");
    println!("VERIFY BURNED: item #{item} burned at hop {hop}; later hops ignored");
    Verdict::Burned { hop, why }
}

/// Sums every output in `tx` that `fvk` can decrypt (external scope). Overflow is an error.
pub fn paid_to(tx: &Transaction, fvk: &FullViewingKey) -> Result<u64> {
    decrypt_ivk(tx, fvk)
        .iter()
        .try_fold(0u64, |acc, f| acc.checked_add(f.note.value().inner()))
        .ok_or_else(|| anyhow!("payment sum overflows"))
}
