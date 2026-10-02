//! carrier-core: everything about carriers that does not touch the network or the filesystem.
//! Compiles for native targets and for `wasm32-unknown-unknown`, so the CLI and the browser
//! marketplace share one implementation of the memo format, keys, decryption and rules R1 to R3.
use anyhow::{anyhow, Result};
use orchard::{
    keys::{FullViewingKey, PreparedIncomingViewingKey, Scope, SpendingKey},
    note::{ExtractedNoteCommitment, RandomSeed, Rho},
    note_encryption::IronwoodDomain,
    value::NoteValue,
    Address, Note, NoteVersion,
};
use serde::{Deserialize, Serialize};
/// An Ironwood bundle as mined (authorized). Rules operate on bundles, so this crate needs no
/// transaction parser; callers extract the bundle from their own transaction type.
pub type IwBundle = orchard::Bundle<orchard::bundle::Authorized, zcash_protocol::value::ZatBalance>;

pub mod keys;
pub mod verify;

// ---------- memo format (ZIP 302 arbitrary-data memo, lead byte 0xF5) ----------
pub const MAGIC: &[u8; 4] = b"ZCR1";
pub const K_SEQ: u8 = 1;
pub const K_CARRIER: u8 = 2;
pub const K_HOP: u8 = 3;
pub const K_FINAL: u8 = 4;

#[derive(Clone, Debug)]
pub struct Memo {
    pub kind: u8,
    pub coll: [u8; 32],
    pub num: u32,
    pub hop: u32,
    pub art: Vec<u8>,
}

pub fn encode_memo(kind: u8, coll: &[u8; 32], num: u32, hop: u32, art: &[u8]) -> [u8; 512] {
    let mut m = [0u8; 512];
    m[0] = 0xF5;
    m[1..5].copy_from_slice(MAGIC);
    m[5] = kind;
    m[6..38].copy_from_slice(coll);
    m[38..42].copy_from_slice(&num.to_le_bytes());
    m[42..46].copy_from_slice(&hop.to_le_bytes());
    assert!(art.len() <= 512 - 48);
    m[46..48].copy_from_slice(&(art.len() as u16).to_le_bytes());
    m[48..48 + art.len()].copy_from_slice(art);
    m
}

pub fn decode_memo(m: &[u8; 512]) -> Option<Memo> {
    if m[0] != 0xF5 || &m[1..5] != MAGIC {
        return None;
    }
    let len = u16::from_le_bytes([m[46], m[47]]) as usize;
    if 48 + len > 512 {
        return None;
    }
    Some(Memo {
        kind: m[5],
        coll: m[6..38].try_into().unwrap(),
        num: u32::from_le_bytes(m[38..42].try_into().unwrap()),
        hop: u32::from_le_bytes(m[42..46].try_into().unwrap()),
        art: m[48..48 + len].to_vec(),
    })
}

// ---------- records (marketplace SPEC 4.2, 4.3) ----------
/// Memo kind of the buyer's change output in a sale: the account index record.
pub const K_ACCOUNT: u8 = 5;
pub const PTR_ISSUANCE: u8 = 0;
pub const PTR_TRANSFER: u8 = 1;
pub const VAULT_RECORD_LEN: usize = 135;

/// Vault record v1, the payload of a successor's HOP memo: the previous vault's FVK and where
/// the carrier it held was created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRecord {
    pub fvk: [u8; 96],
    /// `PTR_ISSUANCE` or `PTR_TRANSFER`
    pub kind: u8,
    /// protocol byte order
    pub txid: [u8; 32],
    pub action: u32,
}

impl VaultRecord {
    pub fn encode(&self) -> [u8; VAULT_RECORD_LEN] {
        let mut r = [0u8; VAULT_RECORD_LEN];
        r[0] = 0x01;
        r[1..97].copy_from_slice(&self.fvk);
        r[97] = self.kind;
        r[98..130].copy_from_slice(&self.txid);
        r[130..134].copy_from_slice(&self.action.to_le_bytes());
        r
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != VAULT_RECORD_LEN || b[0] != 0x01 || b[134] != 0 || b[97] > PTR_TRANSFER {
            return Err(anyhow!("not a v1 vault record ({} bytes)", b.len()));
        }
        Ok(VaultRecord { fvk: b[1..97].try_into()?, kind: b[97], txid: b[98..130].try_into()?, action: u32::from_le_bytes(b[130..134].try_into()?) })
    }
}

/// Account index record v1 payload: the ZIP-32 account index of the buyer's new vault.
pub fn encode_account_record(vault_index: u32) -> [u8; 5] {
    let mut r = [0x01, 0, 0, 0, 0];
    r[1..].copy_from_slice(&vault_index.to_le_bytes());
    r
}

pub fn decode_account_record(b: &[u8]) -> Option<u32> {
    (b.len() == 5 && b[0] == 0x01).then(|| u32::from_le_bytes(b[1..].try_into().unwrap()))
}

// ---------- FINAL hard seal (OPEN_ISSUES #2) ----------
pub const SEAL_DOMAIN: &str = "carrier:final-seal:v1";
const SEAL_DIVERSIFIER: &[u8; 11] = b"carrierseal";

/// An Orchard-protocol address whose spending key nobody knows. Its transmission key `pk_d` is a
/// hash-to-curve point of the collection id, so nobody knows an `ivk` with `pk_d = [ivk] g_d`.
/// A FINAL note sent here can never be spent, so the sequence chain can never continue. Anyone
/// can recompute the address from `coll`.
pub fn seal_address(coll: &[u8; 32]) -> Address {
    use group::GroupEncoding;
    use pasta_curves::arithmetic::CurveExt;
    let hasher = pasta_curves::pallas::Point::hash_to_curve(SEAL_DOMAIN);
    for ctr in 0u32.. {
        let mut msg = coll.to_vec();
        msg.extend_from_slice(&ctr.to_le_bytes());
        let mut raw = [0u8; 43];
        raw[..11].copy_from_slice(SEAL_DIVERSIFIER);
        raw[11..].copy_from_slice(&hasher(&msg).to_bytes());
        if let Some(a) = Option::from(Address::from_raw_address_bytes(&raw)) {
            return a; // fails only for the identity point, which hash_to_curve practically never yields
        }
    }
    unreachable!()
}

// ---------- keys / notes ----------
pub fn spending_key(seed: &[u8; 32], account: u32) -> SpendingKey {
    SpendingKey::from_zip32_seed(seed, 1, zip32::AccountId::try_from(account).unwrap()).unwrap()
}

pub fn fvk_hex(fvk: &FullViewingKey) -> String {
    hex::encode(fvk.to_bytes())
}

pub fn fvk_from_hex(s: &str) -> Result<FullViewingKey> {
    let b: [u8; 96] = hex::decode(s)?.try_into().map_err(|_| anyhow!("fvk len"))?;
    FullViewingKey::from_bytes(&b).ok_or_else(|| anyhow!("bad fvk"))
}

pub fn ua(addr: &Address) -> String {
    use zcash_address::unified::{self, Encoding};
    let ua = unified::Address::try_from_items(vec![unified::Receiver::Orchard(addr.to_raw_address_bytes())]).unwrap();
    ua.encode(&zcash_protocol::consensus::NetworkType::Test)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NoteJson {
    pub recipient: String,
    pub value: u64,
    pub rho: String,
    pub rseed: String,
}

pub fn note_to_json(n: &Note) -> NoteJson {
    NoteJson {
        recipient: hex::encode(n.recipient().to_raw_address_bytes()),
        value: n.value().inner(),
        rho: hex::encode(n.rho().to_bytes()),
        rseed: hex::encode(n.rseed().as_bytes()),
    }
}

fn arr<const N: usize>(s: &str) -> Result<[u8; N]> {
    hex::decode(s)?.try_into().map_err(|_| anyhow!("hex length"))
}

pub fn note_from_json(j: &NoteJson) -> Result<Note> {
    let recipient = Option::from(Address::from_raw_address_bytes(&arr::<43>(&j.recipient)?)).ok_or_else(|| anyhow!("addr"))?;
    let rho: Rho = Option::from(Rho::from_bytes(&arr::<32>(&j.rho)?)).ok_or_else(|| anyhow!("rho"))?;
    let rseed = Option::from(RandomSeed::from_bytes(arr::<32>(&j.rseed)?, &rho)).ok_or_else(|| anyhow!("rseed"))?;
    Option::from(Note::from_parts(recipient, NoteValue::from_raw(j.value), rho, rseed, NoteVersion::V3))
        .ok_or_else(|| anyhow!("note"))
}

pub fn cmx_of(n: &Note) -> [u8; 32] {
    ExtractedNoteCommitment::from(n.commitment()).to_bytes()
}

pub fn txid_bytes(hex_display: &str) -> Result<[u8; 32]> {
    let mut b = arr::<32>(hex_display)?;
    b.reverse();
    Ok(b)
}

pub fn txid_hex(protocol_order: &[u8]) -> String {
    let mut b = protocol_order.to_vec();
    b.reverse();
    hex::encode(b)
}


// ---------- decryption ----------
pub struct Found {
    pub action: usize,
    pub note: Note,
    pub memo: [u8; 512],
    pub cmx: [u8; 32],
    pub spent_nf: [u8; 32],
}

/// Trial-decrypts every action of an Ironwood bundle (any authorization state) with `fvk`'s
/// external incoming viewing key.
pub fn decrypt_bundle_ivk<A: orchard::bundle::Authorization, V>(b: Option<&orchard::Bundle<A, V>>, fvk: &FullViewingKey) -> Vec<Found> {
    let pivk = PreparedIncomingViewingKey::new(&fvk.to_ivk(Scope::External));
    let mut out = vec![];
    if let Some(b) = b {
        for (i, a) in b.actions().iter().enumerate() {
            let d = IronwoodDomain::for_action(a);
            if let Some((note, _, memo)) = zcash_note_encryption::try_note_decryption(&d, &pivk, a) {
                out.push(Found { action: i, note, memo, cmx: a.cmx().to_bytes(), spent_nf: a.nullifier().to_bytes() });
            }
        }
    }
    out
}

/// Recovers every output of an Ironwood bundle that `fvk`'s external outgoing viewing key sent.
pub fn recover_bundle_ovk<A: orchard::bundle::Authorization, V>(b: Option<&orchard::Bundle<A, V>>, fvk: &FullViewingKey) -> Vec<Found> {
    let ovk = fvk.to_ovk(Scope::External);
    let mut out = vec![];
    if let Some(b) = b {
        for (i, a) in b.actions().iter().enumerate() {
            let d = IronwoodDomain::for_action(a);
            if let Some((note, _, memo)) = zcash_note_encryption::try_output_recovery_with_ovk(
                &d,
                &ovk,
                a,
                a.cv_net(),
                &a.encrypted_note().out_ciphertext,
            ) {
                out.push(Found { action: i, note, memo, cmx: a.cmx().to_bytes(), spent_nf: a.nullifier().to_bytes() });
            }
        }
    }
    out
}

// ---------- published artefacts ----------
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Collection {
    pub network: String,
    pub coll: String,
    pub issuer_fvk: String,
    pub genesis_txid: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Hop {
    pub note: NoteJson,
    pub vault_fvk: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Package {
    pub item: u32,
    pub label: String,
    pub hops: Vec<Hop>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_address_is_deterministic_and_per_collection() {
        let (a, b) = ([7u8; 32], [8u8; 32]);
        assert_eq!(seal_address(&a).to_raw_address_bytes(), seal_address(&a).to_raw_address_bytes());
        assert_ne!(seal_address(&a).to_raw_address_bytes(), seal_address(&b).to_raw_address_bytes());
        // not the address of any key we derive
        let fvk = FullViewingKey::from(&spending_key(&[1u8; 32], 0));
        assert_eq!(fvk.scope_for_address(&seal_address(&a)), None);
    }

    #[test]
    fn memo_round_trip() {
        let m = encode_memo(K_HOP, &[3u8; 32], 5, 2, &[]);
        let d = decode_memo(&m).unwrap();
        assert_eq!((d.kind, d.coll, d.num, d.hop), (K_HOP, [3u8; 32], 5, 2));
    }

    #[test]
    fn records_round_trip_and_reject() {
        let r = VaultRecord { fvk: [4u8; 96], kind: PTR_TRANSFER, txid: [5u8; 32], action: 2 };
        let b = r.encode();
        assert_eq!(VaultRecord::decode(&b).unwrap(), r);
        let in_memo = decode_memo(&encode_memo(K_HOP, &[3u8; 32], 1, 3, &b)).unwrap();
        assert_eq!(VaultRecord::decode(&in_memo.art).unwrap(), r);
        for (i, v) in [(0, 2u8), (97, 2), (134, 1)] {
            let mut bad = b;
            bad[i] = v;
            assert!(VaultRecord::decode(&bad).is_err());
        }
        assert!(VaultRecord::decode(&b[..134]).is_err());
        assert!(VaultRecord::decode(&[]).is_err());
        assert_eq!(decode_account_record(&encode_account_record(17)), Some(17));
        assert_eq!(decode_account_record(&[0x02, 17, 0, 0, 0]), None);
    }
}
