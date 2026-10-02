//! Chain access for rules R1 to R3 (`carrier_core::verify`) over lightwalletd.
//!
//! Trust: chain data comes from lightwalletd (`LWD`). We check that returned txids match, that
//! block ranges are hash-linked, and that full txs agree with the compact data they were indexed
//! from. With `LWD2`, every compact block in the scanned range (txids and Ironwood actions) and
//! every fetched transaction must be identical on both servers, so one server cannot hide a spend
//! the other serves. None of this validates proof of work.
use crate::*;
pub use carrier_core::verify::*;
use anyhow::{anyhow, bail, Result};
use std::collections::{BTreeSet, HashMap};
use zcash_primitives::transaction::Transaction;


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

    async fn tx_checked(&mut self, txid: &str) -> Result<(Transaction, u64)> {
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


}

impl ChainSource for Chain {
    async fn tx(&mut self, txid: &str) -> Result<(IwBundle, u64)> {
        let (t, h) = self.tx_checked(txid).await?;
        Ok((t.ironwood_bundle().cloned().ok_or_else(|| anyhow!("{txid} has no Ironwood bundle"))?, h))
    }

    fn spent_at(&self, nf: &[u8; 32]) -> Option<NfHit> {
        self.nf_index.get(nf).cloned()
    }

    /// Fetches the tx that revealed `nf` and checks it against the compact data it was indexed from.
    async fn spend_tx(&mut self, hit: &NfHit, nf: &[u8; 32]) -> Result<IwBundle> {
        let (tx, h) = self.tx_checked(&hit.txid).await?;
        let a = tx
            .ironwood_bundle()
            .and_then(|b| b.actions().get(hit.action))
            .ok_or_else(|| anyhow!("spend tx {} has no action {}", hit.txid, hit.action))?;
        if h != hit.height || a.nullifier().to_bytes() != *nf || a.cmx().to_bytes() != hit.cmx {
            bail!("full tx {} disagrees with the compact block it was indexed from", hit.txid);
        }
        tx.ironwood_bundle().cloned().ok_or_else(|| anyhow!("no Ironwood bundle"))
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

