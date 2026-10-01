//! verify: independent verifier. Uses only public chain data + published collection record
//! + the provenance package a buyer receives. It never reads the local seed.
//!
//! Rules checked:
//!  R1 issuance set  = carriers in the genesis tx + carriers in txs that advance the sequence
//!                     chain (a spend of the live sequence note bound to its successor in the
//!                     SAME action), up to a FINAL memo. Anything else claiming the collection is
//!                     rejected.
//!  R2 transfer hop  = spend of the current carrier note, with the successor output in the SAME
//!                     action (successor rho == spent nullifier, enforced by the Action circuit).
//!                     The successor must have value 0 and a HOP memo for the same collection and
//!                     item. Otherwise the item is burned (exit 3). Needs the successor's key: the
//!                     next hop's vault key, or HOLDER_FVK for the last hop.
//!  R3 liveness      = the current carrier's nullifier has not appeared on chain.
use anyhow::{anyhow, bail, Result};
use carrier::*;
use orchard::keys::FullViewingKey;
use std::fs;
use zcash_primitives::transaction::Transaction;

fn load<T: serde::de::DeserializeOwned>(p: &str) -> Result<T> {
    Ok(serde_json::from_str(&fs::read_to_string(p)?)?)
}

struct Chain {
    c: Client,
    nf_index: std::collections::HashMap<[u8; 32], (String, u64, usize)>, // nf -> (txid, height, action)
}

impl Chain {
    async fn index(c: Client, from: u64) -> Result<Self> {
        let mut me = Chain { c, nf_index: Default::default() };
        let to = tip(&mut me.c).await?;
        for b in blocks(&mut me.c, from, to).await? {
            for tx in &b.vtx {
                for (i, a) in tx.ironwood_actions.iter().enumerate() {
                    let nf: [u8; 32] = a.nullifier.clone().try_into().map_err(|_| anyhow!("nf"))?;
                    me.nf_index.insert(nf, (txid_hex(&tx.txid), b.height, i));
                }
            }
        }
        Ok(me)
    }
    async fn tx(&mut self, txid: &str) -> Result<(Transaction, u64)> {
        let (t, h) = get_tx(&mut self.c, txid).await?;
        Ok((t, h.ok_or_else(|| anyhow!("{txid} unmined"))?))
    }
}

/// Given a spent note's nullifier, return (txid, action) where it was spent, if ever.
fn spent_at(ch: &Chain, nf: &[u8; 32]) -> Option<(String, u64, usize)> {
    ch.nf_index.get(nf).cloned()
}

fn rho_is(action_nf: &[u8; 32], note: &orchard::Note) -> bool {
    note.rho().to_bytes() == *action_nf
}

async fn issuance_set(ch: &mut Chain, col: &Collection, issuer: &FullViewingKey) -> Result<(Vec<(u32, [u8; 32], Vec<u8>, String)>, bool)> {
    // returns [(item, carrier_cmx, art, txid)], finalized?
    let coll = hex::decode(&col.coll)?;
    let mut items = vec![];
    let mut txid = col.genesis_txid.clone();
    let mut expect_rho: Option<[u8; 32]> = None; // successor rho the next sequence note must have
    let mut finalized = false;
    loop {
        let (tx, _h) = ch.tx(&txid).await?;
        // sequence note (decryptable by issuer ivk) in this tx
        let seq = decrypt_ivk(&tx, issuer)
            .into_iter()
            .filter_map(|f| decode_memo(&f.memo).map(|m| (f, m)))
            .find(|(_, m)| (m.kind == K_SEQ || m.kind == K_FINAL) && m.coll.as_slice() == coll.as_slice())
            .ok_or_else(|| anyhow!("no sequence note in {txid}"))?;
        if let Some(r) = expect_rho {
            if !(rho_is(&r, &seq.0.note) && seq.0.spent_nf == r) {
                bail!("sequence successor in {txid} not bound in the same action");
            }
        }
        // carriers created in this issuance step: outputs recovered with the issuer ovk
        for f in recover_ovk(&tx, issuer) {
            if let Some(m) = decode_memo(&f.memo) {
                if m.kind == K_CARRIER && m.coll.as_slice() == coll.as_slice() {
                    items.push((m.num, f.cmx, m.art.clone(), txid.clone()));
                }
            }
        }
        if seq.1.kind == K_FINAL {
            finalized = true;
            break;
        }
        let nf = seq.0.note.nullifier(issuer).to_bytes();
        match spent_at(ch, &nf) {
            None => break, // open sequence: the set may still grow
            Some((t, _, _)) => {
                expect_rho = Some(nf);
                txid = t;
            }
        }
    }
    Ok((items, finalized))
}

async fn run() -> Result<i32> {
    let a: Vec<String> = std::env::args().collect();
    let col: Collection = load(&a[1])?;
    let issuer = fvk_from_hex(&col.issuer_fvk)?;
    let from: u64 = a[2].parse()?;
    let c = connect().await?;
    let mut ch = Chain::index(c, from).await?;
    println!("== collection {}", &col.coll[..16]);

    let (items, finalized) = issuance_set(&mut ch, &col, &issuer).await?;
    println!("R1 issuance set: {} item(s), finalized={finalized}", items.len());
    let mut nums: Vec<u32> = items.iter().map(|i| i.0).collect();
    nums.sort();
    let dup = nums.windows(2).any(|w| w[0] == w[1]);
    if dup {
        bail!("R1 duplicate item numbers in issuance set");
    }
    for (n, cmx, art, t) in &items {
        println!("   #{n} cmx {}.. art {}B in {}..", &hex::encode(cmx)[..12], art.len(), &t[..12]);
        if !art.is_empty() {
            fs::write(format!("out/item{n}_from_chain.bin"), art)?;
        }
    }

    // Stray claims: scan for any tx the issuer ovk can open that carries a CARRIER memo but is
    // not in the issuance set (attack C). Checked for txids supplied on the command line.
    for extra in a.iter().skip(4) {
        let (tx, _) = ch.tx(extra).await?;
        for f in recover_ovk(&tx, &issuer).into_iter().chain(decrypt_ivk(&tx, &issuer)) {
            if let Some(m) = decode_memo(&f.memo) {
                if m.kind == K_CARRIER && !items.iter().any(|i| i.1 == f.cmx) {
                    println!("R1 REJECT claimed item #{} in {}..: not in finalized issuance set", m.num, &extra[..12]);
                }
            }
        }
    }

    // Provenance package
    if let Some(pp) = a.get(3) {
        let pkg: Package = load(pp)?;
        println!("== package item #{} ({})", pkg.item, pkg.label);
        let (_, genesis_cmx, _, _) = items.iter().find(|i| i.0 == pkg.item).ok_or_else(|| anyhow!("item not issued"))?;
        let coll = hex::decode(&col.coll)?;
        let mut cur_cmx = *genesis_cmx;
        // (spend tx, action index) of the previous hop. Its output is the successor to validate.
        let mut prev: Option<(Transaction, usize)> = None;
        for (k, hop) in pkg.hops.iter().enumerate() {
            let note = note_from_json(&hop.note)?;
            let vfvk = fvk_from_hex(&hop.vault_fvk)?;
            if cmx_of(&note) != cur_cmx {
                bail!("hop {k}: disclosed note does not open the current carrier commitment");
            }
            if vfvk.scope_for_address(&note.recipient()).is_none() {
                bail!("hop {k}: disclosed vault key does not own the carrier note");
            }
            // R2 successor validity of the previous hop, opened with this hop's vault key.
            if let Some((ptx, pai)) = &prev {
                let f = decrypt_ivk(ptx, &vfvk)
                    .into_iter()
                    .find(|f| f.action == *pai)
                    .ok_or_else(|| anyhow!("hop {k}: vault key cannot decrypt the successor output"))?;
                if let Err(why) = successor_valid(&f, &coll, pkg.item) {
                    return Ok(burned(pkg.item, k - 1, &why));
                }
                println!("R2 hop {}: successor valid (HOP, value 0)", k - 1);
            }
            let nf = note.nullifier(&vfvk).to_bytes();
            let (t, h, _) = spent_at(&ch, &nf).ok_or_else(|| anyhow!("hop {k}: carrier not spent on chain"))?;
            // find the successor: the output in the same action whose rho == nf. Its cmx is the new carrier.
            let (tx, _) = ch.tx(&t).await?;
            let ai = tx
                .ironwood_bundle()
                .unwrap()
                .actions()
                .iter()
                .position(|x| x.nullifier().to_bytes() == nf)
                .ok_or_else(|| anyhow!("action"))?;
            cur_cmx = tx.ironwood_bundle().unwrap().actions()[ai].cmx().to_bytes();
            println!("R2 hop {k}: spent in {}..@{h}, successor cmx {}.. (same action)", &t[..12], &hex::encode(cur_cmx)[..12]);
            prev = Some((tx, ai));
        }
        println!("== current carrier cmx {}..", &hex::encode(cur_cmx)[..12]);
        std::fs::write("out/current_carrier_cmx.txt", hex::encode(cur_cmx))?;
        // holder side: the current owner proves it by decrypting the carrier with its own key.
        // The last successor can be opened only by its recipient, so its validity is checked here.
        match (std::env::var("HOLDER_FVK").ok(), &prev) {
            (Some(holder), Some((tx, ai))) => {
                let hf = fvk_from_hex(&holder)?;
                let mine = decrypt_ivk(tx, &hf).into_iter().find(|f| f.action == *ai && f.cmx == cur_cmx).ok_or_else(|| anyhow!("holder cannot open current carrier"))?;
                if let Err(why) = successor_valid(&mine, &coll, pkg.item) {
                    return Ok(burned(pkg.item, pkg.hops.len() - 1, &why));
                }
                println!("R2 hop {}: successor valid (HOP, value 0)", pkg.hops.len() - 1);
                let live = spent_at(&ch, &mine.note.nullifier(&hf).to_bytes()).is_none();
                println!("R3 holder opens current carrier: yes, unspent={live}");
            }
            (None, Some(_)) => println!("R2 hop {}: successor validity unverified (needs holder key)", pkg.hops.len() - 1),
            (_, None) => {}
        }
    }
    Ok(0)
}

/// OPEN_ISSUES #7: a successor is a valid carrier only if it has value 0 and a HOP memo for the
/// same collection and item number. Anything else burns the item.
fn successor_valid(f: &Found, coll: &[u8], item: u32) -> std::result::Result<(), String> {
    let v = f.note.value().inner();
    if v != 0 {
        return Err(format!("value {v} != 0"));
    }
    let m = decode_memo(&f.memo).ok_or_else(|| "memo is not a carrier memo".to_string())?;
    if m.kind != K_HOP {
        return Err(format!("memo kind {} != HOP", m.kind));
    }
    if m.coll.as_slice() != coll {
        return Err("memo collection differs".into());
    }
    if m.num != item {
        return Err(format!("memo item #{} != #{item}", m.num));
    }
    Ok(())
}

/// Burned verdict. Later hops are ignored. Exit code 3.
fn burned(item: u32, hop: usize, why: &str) -> i32 {
    println!("R2 hop {hop}: successor invalid ({why})");
    println!("VERIFY BURNED: item #{item} burned at hop {hop}; later hops ignored");
    3
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(e) => {
            println!("VERIFY FAIL: {e:#}");
            std::process::exit(2);
        }
    }
}
