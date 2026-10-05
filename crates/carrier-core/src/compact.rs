//! Just enough protobuf to read lightwalletd `CompactBlock` messages (field numbers from
//! zcash_client_backend 0.24.0 `proto/compact_formats.rs`), so the browser can index nullifiers
//! from grpc-web without a protobuf dependency. Unknown fields are skipped.
use anyhow::{anyhow, bail, Result};

pub struct Action {
    pub nullifier: [u8; 32],
    pub cmx: [u8; 32],
    /// ephemeral key (3) and the first 52 bytes of the note ciphertext (4), for trial decryption
    pub epk: [u8; 32],
    pub ciphertext: Vec<u8>,
}

pub struct Tx {
    /// protocol byte order, as served
    pub txid: [u8; 32],
    pub ironwood: Vec<Action>,
}

pub struct Block {
    pub height: u64,
    pub hash: Vec<u8>,
    pub prev_hash: Vec<u8>,
    pub vtx: Vec<Tx>,
}

enum Val<'a> {
    Int(u64),
    Bytes(&'a [u8]),
}

fn varint(b: &[u8], i: &mut usize) -> Result<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let x = *b.get(*i).ok_or_else(|| anyhow!("protobuf: truncated varint"))?;
        *i += 1;
        v |= u64::from(x & 0x7f) << shift;
        if x & 0x80 == 0 {
            return Ok(v);
        }
    }
    bail!("protobuf: varint too long")
}

fn fields(b: &[u8]) -> Result<Vec<(u64, Val<'_>)>> {
    let (mut i, mut out) = (0, vec![]);
    while i < b.len() {
        let key = varint(b, &mut i)?;
        let take = |i: &mut usize, n: usize| -> Result<&[u8]> {
            let end = i.checked_add(n).filter(|e| *e <= b.len()).ok_or_else(|| anyhow!("protobuf: truncated field"))?;
            let s = &b[*i..end];
            *i = end;
            Ok(s)
        };
        let v = match key & 7 {
            0 => Val::Int(varint(b, &mut i)?),
            1 => Val::Bytes(take(&mut i, 8)?),
            2 => {
                let n = usize::try_from(varint(b, &mut i)?)?;
                Val::Bytes(take(&mut i, n)?)
            }
            5 => Val::Bytes(take(&mut i, 4)?),
            w => bail!("protobuf: wire type {w}"),
        };
        out.push((key >> 3, v));
    }
    Ok(out)
}

fn arr32(v: &[u8], what: &str) -> Result<[u8; 32]> {
    v.try_into().map_err(|_| anyhow!("compact block: {what} is {} bytes", v.len()))
}

/// Parses one `CompactBlock`: height (2), hash (3), prevHash (4), vtx (7); per tx the txid (2)
/// and Ironwood actions (9) with nullifier (1), cmx (2), ephemeral key (3) and ciphertext (4).
pub fn parse_block(b: &[u8]) -> Result<Block> {
    let mut blk = Block { height: 0, hash: vec![], prev_hash: vec![], vtx: vec![] };
    for (f, v) in fields(b)? {
        match (f, v) {
            (2, Val::Int(h)) => blk.height = h,
            (3, Val::Bytes(h)) => blk.hash = h.to_vec(),
            (4, Val::Bytes(h)) => blk.prev_hash = h.to_vec(),
            (7, Val::Bytes(t)) => {
                let mut tx = Tx { txid: [0; 32], ironwood: vec![] };
                for (f, v) in fields(t)? {
                    match (f, v) {
                        (2, Val::Bytes(id)) => tx.txid = arr32(id, "txid")?,
                        (9, Val::Bytes(a)) => {
                            let (mut nf, mut cmx, mut epk, mut ciphertext) = (None, None, [0u8; 32], vec![]);
                            for (f, v) in fields(a)? {
                                match (f, v) {
                                    (1, Val::Bytes(x)) => nf = Some(arr32(x, "nullifier")?),
                                    (2, Val::Bytes(x)) => cmx = Some(arr32(x, "cmx")?),
                                    (3, Val::Bytes(x)) => epk = arr32(x, "ephemeral key")?,
                                    (4, Val::Bytes(x)) => ciphertext = x.to_vec(),
                                    _ => {}
                                }
                            }
                            let (Some(nullifier), Some(cmx)) = (nf, cmx) else { bail!("compact block: Ironwood action without nullifier or cmx") };
                            tx.ironwood.push(Action { nullifier, cmx, epk, ciphertext });
                        }
                        _ => {}
                    }
                }
                blk.vtx.push(tx);
            }
            _ => {}
        }
    }
    if blk.hash.len() != 32 || blk.prev_hash.len() != 32 {
        bail!("compact block {}: hash or prevHash missing", blk.height);
    }
    Ok(blk)
}

/// Whether `ivk` can decrypt a compact Ironwood action, and the note if so. This only finds which
/// actions are ours; the memo needs the full transaction.
pub fn trial_decrypt(a: &Action, ivk: &orchard::keys::PreparedIncomingViewingKey) -> Option<orchard::Note> {
    use orchard::note_encryption::{CompactAction, IronwoodDomain};
    let ct: [u8; 52] = a.ciphertext.as_slice().try_into().ok()?;
    let nf = Option::from(orchard::note::Nullifier::from_bytes(&a.nullifier))?;
    let cmx = Option::from(orchard::note::ExtractedNoteCommitment::from_bytes(&a.cmx))?;
    let ca = CompactAction::from_parts(nf, cmx, zcash_note_encryption::EphemeralKeyBytes(a.epk), ct);
    zcash_note_encryption::try_compact_note_decryption(&IronwoodDomain::for_compact_action(&ca), ivk, &ca).map(|(n, _)| n)
}
