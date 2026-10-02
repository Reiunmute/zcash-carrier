//! The verification procedure, shared by the `verify` CLI and the buyer side of `carrier sale-buy`.
//!
//!  R1 issuance set  = carriers in the genesis tx + carriers in txs that advance the sequence
//!                     chain (a spend of the live sequence note bound to its successor in the
//!                     SAME action), up to a FINAL memo. Each issuance tx has exactly one sequence
//!                     note, and sequence notes and carriers have value 0. Carriers have hop 0.
//!  R2 transfer hop  = spend of the current carrier note, with the successor output in the SAME
//!                     action (successor rho == spent nullifier, enforced by the Action circuit).
//!                     The successor must have value 0 and a HOP memo for the same collection and
//!                     item, with hop = previous hop + 1. Otherwise the item is burned.
//!  R3 holder        = the holder key opens the current carrier and its nullifier has not
//!                     appeared on chain. A valid chain whose current carrier is already spent is
//!                     reported as SPENT, not as held.
//!
//! Trust: chain data comes from one lightwalletd (`LWD`). We check that returned txids match,
//! that block ranges are hash-linked, and that full txs agree with the compact data they were
//! indexed from. That does not validate proof of work. `cross_check` compares block hashes with
//! a second server (`LWD2`) when one is configured.
use crate::*;
use anyhow::{anyhow, bail, Result};
use orchard::keys::FullViewingKey;
use std::collections::{BTreeSet, HashMap};
use zcash_client_backend::proto::service::BlockId;
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
    nf_index: HashMap<[u8; 32], NfHit>,
    pub to: u64,
    /// Heights of every block whose data a verdict relied on (for `cross_check`).
    pub used_heights: BTreeSet<u64>,
}

impl Chain {
    /// Indexes every Ironwood nullifier revealed in `from..=tip`.
    pub async fn index(mut c: Client, from: u64) -> Result<Self> {
        let to = tip(&mut c).await?;
        let mut nf_index = HashMap::new();
        for b in blocks(&mut c, from, to).await? {
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
        Ok(Chain { c, nf_index, to, used_heights: BTreeSet::from([to]) })
    }

    pub async fn tx(&mut self, txid: &str) -> Result<(Transaction, u64)> {
        let (t, h) = get_tx(&mut self.c, txid).await?;
        let h = h.ok_or_else(|| anyhow!("{txid} unmined"))?;
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

/// Compares block hashes at every height a verdict used with a second lightwalletd (`LWD2`).
/// Prints the trust basis either way.
pub async fn cross_check(ch: &mut Chain) -> Result<()> {
    let Ok(url2) = std::env::var("LWD2") else {
        println!("trust: chain data from one lightwalletd ({}), not cross-checked (set LWD2)", lwd_url());
        return Ok(());
    };
    let mut c2 = connect_url(&url2).await?;
    let tip2 = tip(&mut c2).await?;
    // The two tips can differ by a block or two. Compare a recent height both servers have.
    let recent = ch.to.min(tip2).saturating_sub(3);
    let mut heights: BTreeSet<u64> = ch.used_heights.iter().copied().filter(|h| *h < ch.to).collect();
    heights.insert(recent);
    for h in &heights {
        let a = ch.c.get_block(BlockId { height: *h, hash: vec![] }).await?.into_inner().hash;
        let b = c2.get_block(BlockId { height: *h, hash: vec![] }).await?.into_inner().hash;
        if a != b {
            bail!("cross-check: {url2} disagrees with {} at height {h}", lwd_url());
        }
    }
    println!("trust: block hashes at {} height(s) match between {} and {url2}", heights.len(), lwd_url());
    Ok(())
}

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
    loop {
        let (tx, h) = ch.tx(&txid).await?;
        let mut seqs: Vec<(Found, Memo)> = decrypt_ivk(&tx, &issuer)
            .into_iter()
            .filter_map(|f| decode_memo(&f.memo).map(|m| (f, m)))
            .filter(|(_, m)| (m.kind == K_SEQ || m.kind == K_FINAL) && m.coll == coll)
            .collect();
        if seqs.len() != 1 {
            bail!("R1: {} sequence notes in {txid}, expected exactly one", seqs.len());
        }
        let (seq, sm) = seqs.remove(0);
        if seq.note.value().inner() != 0 {
            bail!("R1: sequence note in {txid} has value {}", seq.note.value().inner());
        }
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
            if v != 0 || m.hop != 0 {
                println!("R1 REJECT carrier #{} in {}..: value {v} hop {} (both must be 0)", m.num, &txid[..12], m.hop);
                continue;
            }
            items.push(Issued { num: m.num, cmx: f.cmx, art: m.art.clone(), txid: txid.clone(), height: h, action: f.action });
        }
        if sm.kind == K_FINAL {
            finalized = true;
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
    Ok(IssuanceSet { items, finalized })
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
