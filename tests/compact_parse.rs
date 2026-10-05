//! carrier_core::compact (hand-rolled protobuf, used by the browser) reads the same fields as the
//! prost types lightwalletd's protocol is generated into, including blocks with fields it skips.
use prost::Message;
use zcash_client_backend::proto::compact_formats::{ChainMetadata, CompactBlock, CompactOrchardAction, CompactSaplingOutput, CompactTx};

fn action(n: u8) -> CompactOrchardAction {
    CompactOrchardAction { nullifier: vec![n; 32], cmx: vec![n + 1; 32], ephemeral_key: vec![n + 2; 32], ciphertext: vec![n + 3; 52] }
}

#[test]
fn compact_block_parses_like_prost() {
    let b = CompactBlock {
        height: 4_435_628,
        hash: vec![7; 32],
        prev_hash: vec![6; 32],
        time: 1_790_000_000,
        header: vec![1, 2, 3],
        vtx: vec![
            CompactTx { index: 0, txid: vec![9; 32], ironwood_actions: vec![action(10)], ..Default::default() },
            CompactTx { index: 1, txid: vec![8; 32], outputs: vec![CompactSaplingOutput { cmu: vec![1; 32], ephemeral_key: vec![2; 32], ciphertext: vec![3; 52] }], actions: vec![action(40)], ironwood_actions: vec![action(20), action(30)], ..Default::default() },
        ],
        chain_metadata: Some(ChainMetadata { sapling_commitment_tree_size: 5, orchard_commitment_tree_size: 6, ..Default::default() }),
    };
    let p = carrier_core::compact::parse_block(&b.encode_to_vec()).unwrap();
    assert_eq!((p.height, p.hash.clone(), p.prev_hash.clone()), (b.height, b.hash.clone(), b.prev_hash.clone()));
    assert_eq!(p.vtx.len(), 2);
    for (pt, bt) in p.vtx.iter().zip(&b.vtx) {
        assert_eq!(pt.txid.to_vec(), bt.txid);
        let want: Vec<_> = bt.ironwood_actions.iter().map(|a| (a.nullifier.clone(), a.cmx.clone(), a.ephemeral_key.clone(), a.ciphertext.clone())).collect();
        let got: Vec<_> = pt.ironwood.iter().map(|a| (a.nullifier.to_vec(), a.cmx.to_vec(), a.epk.to_vec(), a.ciphertext.clone())).collect();
        assert_eq!(got, want); // Orchard actions (field 6) are not mixed in
    }
    // truncated input is an error, not a panic
    let bytes = b.encode_to_vec();
    assert!(carrier_core::compact::parse_block(&bytes[..bytes.len() - 5]).is_err());
}
