//! Read-only P5 measurement (research/p5-anchor-ages-2026-10-02.md). Usage:
//!   cargo run --release --example anchorage -- [sample_blocks=2000] [lookback_blocks=2000]
use carrier::*;

use std::collections::{BTreeMap, HashMap};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sample: u64 = std::env::args().nth(1).unwrap_or("2000".into()).parse()?;
    let lookback: u64 = std::env::args().nth(2).unwrap_or("2000".into()).parse()?;
    let mut c = connect().await?;
    let tip_h = tip(&mut c).await?;
    let (s0, w0) = (tip_h - sample + 1, tip_h - sample - lookback);
    let mut tree = ironwood_tree(&mut c, w0).await?;
    let mut root_at: HashMap<[u8; 32], u64> = HashMap::new();
    root_at.insert(tree.root().to_bytes(), w0);
    let mut txs = vec![];
    let mut s = w0 + 1;
    while s <= tip_h {
        let e = (s + 4999).min(tip_h);
        for b in blocks(&mut c, s, e).await? {
            for tx in &b.vtx {
                for a in &tx.ironwood_actions {
                    let cmx = orchard::note::ExtractedNoteCommitment::from_bytes(&a.cmx.clone().try_into().unwrap()).unwrap();
                    tree.append(orchard::tree::MerkleHashOrchard::from_cmx(&cmx)).unwrap();
                }
                if b.height >= s0 && !tx.ironwood_actions.is_empty() {
                    txs.push((b.height, txid_hex(&tx.txid), tx.ironwood_actions.len()));
                }
            }
            // first height at which this root became final (roots repeat across empty blocks)
            root_at.entry(tree.root().to_bytes()).or_insert(b.height);
        }
        s = e + 1;
    }
    let server = ironwood_tree(&mut c, tip_h).await?.root();
    anyhow::ensure!(server == tree.root(), "rebuilt root differs from the server treestate at {tip_h}");

    // age histogram per action-count class
    let mut hist: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut n = 0;
    for (h, txid, acts) in &txs {
        let (tx, _) = get_tx(&mut c, txid).await?;
        let Some(b) = tx.ironwood_bundle() else { continue };
        let anchor = b.anchor().to_bytes();
        let class = if *acts == 1 { "1 action".to_string() } else if *acts <= 4 { "2-4 actions".into() } else { "5+ actions".into() };
        let age = match root_at.get(&anchor) {
            Some(ah) if ah < h => {
                let d = h - ah;
                if d < 100 { format!("{d:03}") } else { "100+".into() }
            }
            Some(_) => "same block or later (?)".into(),
            None => if anchor == orchard::tree::Anchor::empty_tree().to_bytes() { "empty tree".into() } else { format!("older than window") },
        };
        *hist.entry((class, age)).or_default() += 1;
        n += 1;
    }
    println!("tip {tip_h}, sample blocks {s0}..={tip_h}, roots from {w0}, {n} txs");
    for ((class, age), k) in &hist {
        println!("{class:12} age {age:24} {k}");
    }
    Ok(())
}
