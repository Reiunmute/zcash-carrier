//! Rules R1 to R3, independent of how chain data is fetched (see `ChainSource`).
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
//! Progress lines go to the `log` callback, so a browser can show the same verdict text as the CLI.
use crate::*;
use anyhow::{anyhow, bail, Result};
use orchard::keys::FullViewingKey;

/// Where a nullifier was revealed, as indexed from compact blocks.
#[derive(Clone, Debug)]
pub struct NfHit {
    pub txid: String,
    pub height: u64,
    pub action: usize,
    pub cmx: [u8; 32],
}

/// Chain data the rules need. The CLI implements it over lightwalletd (`carrier::verifier::Chain`);
/// the browser implements it over grpc-web.
#[allow(async_fn_in_trait)]
pub trait ChainSource {
    /// The Ironwood bundle of a mined transaction, and its height. Implementations must check
    /// that the transaction they fetched has the requested txid.
    async fn tx(&mut self, txid: &str) -> Result<(IwBundle, u64)>;
    /// Where a nullifier was revealed in the indexed range, if it was.
    fn spent_at(&self, nf: &[u8; 32]) -> Option<NfHit>;
    /// The transaction that revealed `nf`, checked against the compact data it was indexed from.
    async fn spend_tx(&mut self, hit: &NfHit, nf: &[u8; 32]) -> Result<IwBundle>;
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
pub async fn issuance_set<C: ChainSource>(ch: &mut C, col: &Collection, log: &mut dyn FnMut(String)) -> Result<IssuanceSet> {
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
        let mut seqs: Vec<(Found, Memo)> = decrypt_bundle_ivk(Some(&tx), &issuer)
            .into_iter()
            .chain(recover_bundle_ovk(Some(&tx), &issuer).into_iter().filter(|f| f.note.recipient() == seal))
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
        for f in recover_bundle_ovk(Some(&tx), &issuer) {
            let Some(m) = decode_memo(&f.memo) else { continue };
            if m.kind != K_CARRIER || m.coll != coll {
                continue;
            }
            let v = f.note.value().inner();
            if v != 0 || m.hop != 0 || m.num == 0 || m.art.len() != ART_V0_LEN {
                log(format!("R1 REJECT carrier #{} in {}..: value {v} hop {} art {}B (need value 0, hop 0, item >= 1, {ART_V0_LEN}B v0 art)", m.num, &txid[..12], m.hop, m.art.len()));
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
pub async fn verify_item<C: ChainSource>(ch: &mut C, col: &Collection, set: &IssuanceSet, pkg: &Package, holder: Option<&FullViewingKey>, log: &mut dyn FnMut(String)) -> Result<Verdict> {
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
        let f = decrypt_bundle_ivk(Some(&ctx), &vfvk)
            .into_iter()
            .find(|f| f.action == at.2 && f.cmx == cur_cmx)
            .ok_or_else(|| anyhow!("hop {k}: disclosed vault key cannot open the current carrier"))?;
        // R2 validity of the successor created by the previous hop.
        if k > 0 {
            if let Err(why) = successor_valid(&f, &coll, pkg.item, k as u32) {
                return Ok(burned(pkg.item, k - 1, why, log));
            }
            log(format!("R2 hop {}: successor valid (HOP, value 0)", k - 1));
        }
        let nf = f.note.nullifier(&vfvk).to_bytes();
        let hit = ch.spent_at(&nf).ok_or_else(|| anyhow!("hop {k}: carrier not spent on chain"))?;
        let tx = ch.spend_tx(&hit, &nf).await?;
        cur_cmx = tx.actions()[hit.action].cmx().to_bytes();
        log(format!("R2 hop {k}: spent in {}..@{}, successor cmx {}.. (same action)", &hit.txid[..12], hit.height, &hex::encode(cur_cmx)[..12]));
        at = (hit.txid.clone(), hit.height, hit.action);
    }
    log(format!("== current carrier cmx {}..", &hex::encode(cur_cmx)[..12]));
    let n = pkg.hops.len();
    let Some(hf) = holder else {
        if n > 0 {
            log(format!("R2 hop {}: successor validity unverified (needs holder key)", n - 1));
        }
        log(format!("R3 holder check skipped (needs holder key)"));
        return Ok(Verdict::Unverified { cmx: cur_cmx });
    };
    let (ctx, _) = ch.tx(&at.0).await?;
    let mine = decrypt_bundle_ivk(Some(&ctx), hf)
        .into_iter()
        .find(|f| f.action == at.2 && f.cmx == cur_cmx)
        .ok_or_else(|| anyhow!("holder cannot open current carrier"))?;
    if n > 0 {
        if let Err(why) = successor_valid(&mine, &coll, pkg.item, n as u32) {
            return Ok(burned(pkg.item, n - 1, why, log));
        }
        log(format!("R2 hop {}: successor valid (HOP, value 0)", n - 1));
    }
    let live = ch.spent_at(&mine.note.nullifier(hf).to_bytes()).is_none();
    log(format!("R3 holder opens current carrier: yes, unspent={live}"));
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

fn burned(item: u32, hop: usize, why: String, log: &mut dyn FnMut(String)) -> Verdict {
    log(format!("R2 hop {hop}: successor invalid ({why})"));
    log(format!("VERIFY BURNED: item #{item} burned at hop {hop}; later hops ignored"));
    Verdict::Burned { hop, why }
}

/// Sums every output in `tx` that `fvk` can decrypt (external scope). Overflow is an error.
pub fn paid_to(b: &IwBundle, fvk: &FullViewingKey) -> Result<u64> {
    decrypt_bundle_ivk(Some(b), fvk)
        .iter()
        .try_fold(0u64, |acc, f| acc.checked_add(f.note.value().inner()))
        .ok_or_else(|| anyhow!("payment sum overflows"))
}
