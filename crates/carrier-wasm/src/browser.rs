//! The verifier in the browser (SPEC 3.2 step 3, 6.2): rules R1 to R3 from `carrier-core` over
//! chain data that JavaScript fetches (grpc-web). JavaScript feeds compact blocks in order and
//! supplies `fetch_tx(txid) -> Promise<{data: Uint8Array, height: number}>`; everything it returns
//! is checked here: block heights are contiguous and hash-linked, nullifiers are unique, each tx
//! hashes to the txid asked for, and spend txs agree with the compact data they were indexed from.
//! Comparing two servers (6.2) is the caller's job: run two chains and compare the verdicts.
use anyhow::{anyhow, bail, Result};
use carrier_core::{verify::*, *};
use js_sys::{Function, Promise, Reflect, Uint8Array};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, JsFuture};
use zcash_primitives::transaction::Transaction;

struct JsChain {
    fetch_tx: Function,
    nf: HashMap<[u8; 32], NfHit>,
    /// txid (display hex) -> height, for every tx in the indexed blocks
    txs: HashMap<String, u64>,
    /// (first height, last height, last hash)
    range: Option<(u64, u64, Vec<u8>)>,
}

fn js(e: JsValue) -> anyhow::Error {
    anyhow!("{}", e.as_string().unwrap_or_else(|| format!("{e:?}")))
}

impl JsChain {
    fn add_block(&mut self, bytes: &[u8]) -> Result<()> {
        let b = compact::parse_block(bytes)?;
        if let Some((_, last, hash)) = &self.range {
            if b.height != last + 1 || b.prev_hash != *hash {
                bail!("block {} does not extend block {last} (heights must be contiguous and hash-linked)", b.height);
            }
        }
        for tx in &b.vtx {
            self.txs.insert(txid_hex(&tx.txid), b.height);
            for (i, a) in tx.ironwood.iter().enumerate() {
                let hit = NfHit { txid: txid_hex(&tx.txid), height: b.height, action: i, cmx: a.cmx };
                if self.nf.insert(a.nullifier, hit).is_some() {
                    bail!("duplicate nullifier at height {}", b.height);
                }
            }
        }
        let first = self.range.as_ref().map_or(b.height, |r| r.0);
        self.range = Some((first, b.height, b.hash));
        Ok(())
    }

    async fn raw_tx(&self, txid: &str) -> Result<(Transaction, u64)> {
        let p: Promise = self.fetch_tx.call1(&JsValue::NULL, &JsValue::from_str(txid)).map_err(js)?.into();
        let v = JsFuture::from(p).await.map_err(js)?;
        let data = Uint8Array::new(&Reflect::get(&v, &"data".into()).map_err(js)?).to_vec();
        let h = Reflect::get(&v, &"height".into()).map_err(js)?.as_f64().ok_or_else(|| anyhow!("fetch_tx: no height"))?;
        if !(h >= 1.0 && h <= u32::MAX as f64) {
            bail!("{txid} is not mined");
        }
        // v5 and later carry their branch id; every tx the rules read is post-NU5.
        let tx = Transaction::read(&data[..], zcash_protocol::consensus::BranchId::Nu6_3)?;
        if tx.txid().to_string() != txid {
            bail!("the server returned tx {} for {txid}", tx.txid());
        }
        // A txid fixes the content, not whether it was mined: the tx must be in an indexed block,
        // at the height the server claims.
        match self.txs.get(txid) {
            Some(&ih) if ih == h as u64 => Ok((tx, ih)),
            Some(&ih) => bail!("{txid} is at height {ih} in the indexed blocks, not {h}"),
            None => bail!("{txid} is not in the indexed blocks (not mined, or before the indexed range)"),
        }
    }
}

impl ChainSource for JsChain {
    async fn tx(&mut self, txid: &str) -> Result<(IwBundle, u64)> {
        let (t, h) = self.raw_tx(txid).await?;
        Ok((t.ironwood_bundle().cloned().ok_or_else(|| anyhow!("{txid} has no Ironwood bundle"))?, h))
    }

    fn spent_at(&self, nf: &[u8; 32]) -> Option<NfHit> {
        self.nf.get(nf).cloned()
    }

    async fn spend_tx(&mut self, hit: &NfHit, nf: &[u8; 32]) -> Result<IwBundle> {
        let (t, h) = self.raw_tx(&hit.txid).await?;
        let b = t.ironwood_bundle().ok_or_else(|| anyhow!("no Ironwood bundle"))?;
        let a = b.actions().get(hit.action).ok_or_else(|| anyhow!("spend tx {} has no action {}", hit.txid, hit.action))?;
        if h != hit.height || a.nullifier().to_bytes() != *nf || a.cmx().to_bytes() != hit.cmx {
            bail!("full tx {} disagrees with the compact block it was indexed from", hit.txid);
        }
        Ok(b.clone())
    }
}

/// Chain data for one verification session, fed by JavaScript.
#[wasm_bindgen]
pub struct BrowserChain(Rc<RefCell<JsChain>>);

#[wasm_bindgen]
impl BrowserChain {
    #[wasm_bindgen(constructor)]
    pub fn new(fetch_tx: Function) -> BrowserChain {
        BrowserChain(Rc::new(RefCell::new(JsChain { fetch_tx, nf: HashMap::new(), txs: HashMap::new(), range: None })))
    }

    /// Indexes one serialized `CompactBlock`. Blocks must arrive in height order.
    pub fn add_block(&self, block: &[u8]) -> Result<(), JsError> {
        self.0.borrow_mut().add_block(block).map_err(|e| JsError::new(&format!("{e:#}")))
    }

    /// `[first, last]` indexed heights, or null.
    pub fn range(&self) -> Option<Vec<f64>> {
        self.0.borrow().range.as_ref().map(|r| vec![r.0 as f64, r.1 as f64])
    }

    /// R1 to R3 for `item` of the collection, from `package_json`, or from the vault records
    /// starting at the holder's carrier in `walk_txid` (SPEC 4.2). `min_tip` is the highest tip
    /// any server reported; the indexed blocks must reach it, or a server that hides recent
    /// blocks could make a sold carrier look held. Resolves to JSON
    /// `{verdict, hop?, finalized, sealed, items, range, log}`; verdict is HELD, SPENT, BURNED,
    /// UNVERIFIED or FAIL (with `error`).
    pub fn verify(&self, collection_json: String, item: u32, package_json: Option<String>, walk_txid: Option<String>, holder_fvk: Option<String>, min_tip: f64) -> Promise {
        let ch = self.0.clone();
        future_to_promise(async move {
            let mut log = vec![];
            let mut ch = ch.borrow_mut();
            let out = run(&mut ch, &collection_json, item, package_json, walk_txid, holder_fvk, min_tip, &mut log).await;
            let range = ch.range.as_ref().map(|r| vec![r.0, r.1]);
            let mut v = out.unwrap_or_else(|e| serde_json::json!({ "verdict": "FAIL", "error": format!("{e:#}") }));
            v["log"] = log.into();
            v["range"] = range.into();
            Ok(JsValue::from_str(&v.to_string()))
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(ch: &mut JsChain, col: &str, item: u32, package: Option<String>, walk: Option<String>, holder: Option<String>, min_tip: f64, log: &mut Vec<String>) -> Result<serde_json::Value> {
    let col: Collection = serde_json::from_str(col)?;
    let (first, last) = ch.range.as_ref().map(|r| (r.0, r.1)).ok_or_else(|| anyhow!("no blocks indexed"))?;
    if !(min_tip >= 1.0) || (last as f64) < min_tip {
        bail!("indexed blocks end at {last}, below the highest known tip {min_tip}");
    }
    // Liveness needs every block from the genesis on.
    let (_, gh) = ch.tx(&col.genesis_txid).await?;
    if first > gh {
        bail!("indexed blocks start at {first}, after the genesis at {gh}");
    }
    let holder = holder.map(|h| fvk_from_hex(&h)).transpose()?;
    let set = issuance_set(ch, &col, &mut |l| log.push(l)).await?;
    let pkg: Package = match (package, walk) {
        (Some(p), _) => serde_json::from_str(&p)?,
        (None, Some(txid)) => {
            let hf = holder.as_ref().ok_or_else(|| anyhow!("a record walk needs the holder key"))?;
            let (start, _) = ch.tx(&txid).await?;
            walk_records(ch, &col, &set, item, &start, hf, &mut |l| log.push(l)).await?
        }
        (None, None) => bail!("give a package or a txid to walk from"),
    };
    let verdict = verify_item(ch, &col, &set, &pkg, holder.as_ref(), &mut |l| log.push(l)).await?;
    let mut v = serde_json::json!({ "finalized": set.finalized, "sealed": set.sealed, "items": set.items.len(), "tip_checked": last });
    match verdict {
        Verdict::Held { hop, txid, .. } => { v["verdict"] = "HELD".into(); v["hop"] = hop.into(); v["txid"] = txid.into(); }
        Verdict::Spent { .. } => v["verdict"] = "SPENT".into(),
        Verdict::Burned { hop, why } => { v["verdict"] = "BURNED".into(); v["hop"] = hop.into(); v["why"] = why.into(); }
        Verdict::Unverified { .. } => v["verdict"] = "UNVERIFIED".into(),
    }
    Ok(v)
}
