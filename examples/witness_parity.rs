//! carrier_sale::witness (what the browser uses: tree states and compact blocks it fetched) must
//! give the same Merkle path as the CLI's witness for a real note: the P4 sale's successor carrier,
//! opened with the buyer vault's published view key. Read-only; run by verify.sh.
use carrier::*;
use prost::Message;
use zcash_client_backend::proto::service::BlockId;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    const P4_TX: &str = "e96845ecfc84008674195d8ee77fd433cdb7cdfc3464711ae92221d58ada3b1d";
    let keys: serde_json::Value = serde_json::from_slice(&read_capped(std::path::Path::new("evidence/testnet-2026-10-02/viewing_keys.json"))?)?;
    let fvk = fvk_from_hex(keys["buyer_acct7"]["fvk"].as_str().unwrap())?;
    let mut c = connect().await?;
    let (tx, h) = get_tx(&mut c, P4_TX).await?;
    let (note_h, anchor_h) = (h.unwrap(), h.unwrap() + 50);
    let cmx = decrypt_ivk(&tx, &fvk).first().ok_or_else(|| anyhow::anyhow!("view key opens nothing"))?.cmx;
    let (native_path, native_anchor) = witness(&mut c, &cmx, note_h, anchor_h).await?;
    let tree = |c: &mut Client, h: u64| {
        let mut c = c.clone();
        async move { anyhow::Ok(c.get_tree_state(BlockId { height: h, hash: vec![] }).await?.into_inner().ironwood_tree) }
    };
    let (before, at) = (tree(&mut c, note_h - 1).await?, tree(&mut c, anchor_h).await?);
    let bs = blocks(&mut c, note_h, anchor_h).await?.iter().map(|b| carrier_core::compact::parse_block(&b.encode_to_vec())).collect::<anyhow::Result<Vec<_>>>()?;
    let (path, anchor) = carrier_sale::witness(&before, &bs, &cmx, &at)?;
    let same = anchor == native_anchor && path.position() == native_path.position() && path.auth_path() == native_path.auth_path();
    println!("witness parity: note {}.. at {note_h}, anchor {anchor_h}, position {}, {}", &hex::encode(cmx)[..12], path.position(), if same { "identical" } else { "DIFFERENT" });
    anyhow::ensure!(same, "carrier_sale::witness differs from the CLI's witness");
    Ok(())
}
