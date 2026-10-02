//! The authenticated disclosure channel between buyer and seller (marketplace SPEC 3.2, X3, X6).
//!
//! Messages are HPKE base mode (RFC 9180: DHKEM(X25519, HKDF-SHA256), HKDF-SHA256,
//! ChaCha20-Poly1305) with info `CarrierDisclosureV1`, one HPKE context per message, and an AAD
//! that binds the message version, type, direction and sequence number to the transcript `T`.
//! The seller signs the disclosure with its vault key, so a relay that swaps a channel key is
//! caught by the buyer (`T` names the buyer's own key and nonce).
//!
//! Sequence numbers start at 0 for each direction of one transcript and increase by one per
//! message; the receiver accepts only the next one. Replay sets and leases are plain state the
//! caller keeps (seller tab, buyer tab, relay); time and height are inputs.
use anyhow::{anyhow, bail, Result};
use hpke::{aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256, Deserializable, Kem, OpModeR, OpModeS, Serializable};
use orchard::keys::{FullViewingKey, SpendAuthorizingKey};
use rand_core::{CryptoRng, RngCore};
use std::collections::{HashMap, HashSet};

pub const HPKE_INFO: &[u8] = b"CarrierDisclosureV1";
/// Prefix of the message the seller signs with its vault key.
pub const DISCLOSE_DOMAIN: &[u8] = b"CarrierDisclosureV1";
pub const MSG_VERSION: u8 = 1;
pub const TYPE_INTENT: u8 = 1;
pub const TYPE_DISCLOSURE: u8 = 2;
pub const TYPE_PCZT: u8 = 3;
pub const BUYER_TO_SELLER: u8 = 0;
pub const SELLER_TO_BUYER: u8 = 1;
/// Lease length (X6): 30 minutes, renewable.
pub const LEASE_SECS: u64 = 30 * 60;
/// Largest plaintext accepted (a PCZT is about 15 kB; the cap matches the CLI's PCZT cap).
pub const MAX_PLAINTEXT: usize = 1 << 20;

type K = X25519HkdfSha256;

fn lp(out: &mut Vec<u8>, x: &[u8]) {
    out.extend_from_slice(&(x.len() as u32).to_le_bytes());
    out.extend_from_slice(x);
}

fn blake2b32(personal: &[u8; 16], data: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new().hash_length(32).personal(personal).hash(data).as_bytes().try_into().unwrap()
}

/// What `T` binds: the listing terms, both channel keys and the buyer's single-use nonce.
pub struct Terms {
    pub listing_id: [u8; 32],
    pub coll: [u8; 32],
    pub num: u32,
    pub price: u64,
    pub expiry_height: u64,
    pub seller_key: [u8; 32],
    pub buyer_key: [u8; 32],
    pub nonce: [u8; 32],
}

/// `T = BLAKE2b-256(personal "CarrierDiscloseT", LP(listing_id) || LP(coll) || LE32(num) ||
/// LE64(price) || LE64(expiry_height) || LP(seller_channel_key) || LP(buyer_channel_key) || LP(nonce))`
pub fn transcript(t: &Terms) -> [u8; 32] {
    let mut m = vec![];
    lp(&mut m, &t.listing_id);
    lp(&mut m, &t.coll);
    m.extend_from_slice(&t.num.to_le_bytes());
    m.extend_from_slice(&t.price.to_le_bytes());
    m.extend_from_slice(&t.expiry_height.to_le_bytes());
    lp(&mut m, &t.seller_key);
    lp(&mut m, &t.buyer_key);
    lp(&mut m, &t.nonce);
    blake2b32(b"CarrierDiscloseT", &m)
}

/// `version || type || direction || LE32(sequence) || T`
pub fn aad(kind: u8, direction: u8, seq: u32, t: &[u8; 32]) -> Vec<u8> {
    let mut a = vec![MSG_VERSION, kind, direction];
    a.extend_from_slice(&seq.to_le_bytes());
    a.extend_from_slice(t);
    a
}

/// A fresh X25519 channel key pair: (private, public).
pub fn channel_keypair<R: RngCore + CryptoRng>(rng: &mut R) -> ([u8; 32], [u8; 32]) {
    let (sk, pk) = K::gen_keypair(rng);
    (sk.to_bytes().into(), pk.to_bytes().into())
}

/// Encrypts `plaintext` to `recipient`. Output: the 32-byte encapsulated key, then the ciphertext.
pub fn seal<R: RngCore + CryptoRng>(recipient: &[u8; 32], kind: u8, direction: u8, seq: u32, t: &[u8; 32], plaintext: &[u8], rng: &mut R) -> Result<Vec<u8>> {
    if plaintext.len() > MAX_PLAINTEXT {
        bail!("message too large");
    }
    let pk = <K as Kem>::PublicKey::from_bytes(recipient).map_err(|e| anyhow!("channel key: {e:?}"))?;
    let (enc, ct) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, K, _>(&OpModeS::Base, &pk, HPKE_INFO, plaintext, &aad(kind, direction, seq, t), rng).map_err(|e| anyhow!("seal: {e:?}"))?;
    let mut out = enc.to_bytes().to_vec();
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypts a message from `seal`. Fails if anything in the AAD (type, direction, sequence, `T`)
/// differs from what the sender used.
pub fn open(sk: &[u8; 32], kind: u8, direction: u8, seq: u32, t: &[u8; 32], msg: &[u8]) -> Result<Vec<u8>> {
    if msg.len() < 32 + 16 || msg.len() > MAX_PLAINTEXT + 48 {
        bail!("message length {} out of range", msg.len());
    }
    let sk = <K as Kem>::PrivateKey::from_bytes(sk).map_err(|e| anyhow!("channel key: {e:?}"))?;
    let enc = <K as Kem>::EncappedKey::from_bytes(&msg[..32]).map_err(|e| anyhow!("encapsulated key: {e:?}"))?;
    hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, K>(&OpModeR::Base, &sk, &enc, HPKE_INFO, &msg[32..], &aad(kind, direction, seq, t)).map_err(|_| anyhow!("message does not open (wrong key, transcript, type, direction or sequence)"))
}

/// The disclosure payload (SPEC 3.2 step 2), version 1:
/// `0x01 || vault FVK (96) || carrier txid (32, protocol order) || LE32(carrier action) ||
/// pay address (43, raw) || LP(provenance package, JSON, empty if records cover every hop)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disclosure {
    pub vault_fvk: [u8; 96],
    pub carrier_txid: [u8; 32],
    pub carrier_action: u32,
    pub pay_addr: [u8; 43],
    pub package: Vec<u8>,
}

impl Disclosure {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0x01];
        b.extend_from_slice(&self.vault_fvk);
        b.extend_from_slice(&self.carrier_txid);
        b.extend_from_slice(&self.carrier_action.to_le_bytes());
        b.extend_from_slice(&self.pay_addr);
        lp(&mut b, &self.package);
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        const HEAD: usize = 1 + 96 + 32 + 4 + 43 + 4;
        if b.len() < HEAD || b[0] != 0x01 {
            bail!("not a v1 disclosure");
        }
        let n = u32::from_le_bytes(b[HEAD - 4..HEAD].try_into()?) as usize;
        if b.len() != HEAD.checked_add(n).ok_or_else(|| anyhow!("length"))? {
            bail!("disclosure length mismatch");
        }
        Ok(Disclosure {
            vault_fvk: b[1..97].try_into()?,
            carrier_txid: b[97..129].try_into()?,
            carrier_action: u32::from_le_bytes(b[129..133].try_into()?),
            pay_addr: b[133..176].try_into()?,
            package: b[HEAD..].to_vec(),
        })
    }
}

fn disclose_msg(t: &[u8; 32], payload: &[u8]) -> Vec<u8> {
    let h = blake2b_simd::Params::new().hash_length(32).hash(payload);
    [DISCLOSE_DOMAIN, t, h.as_bytes()].concat()
}

/// Seller: the signed, encrypted disclosure for the lease holder. Plaintext: 64-byte vault
/// signature over `"CarrierDisclosureV1" || T || BLAKE2b-256(payload)`, then the payload.
pub fn seal_disclosure<R: RngCore + CryptoRng>(d: &Disclosure, vault_sak: &SpendAuthorizingKey, buyer_key: &[u8; 32], seq: u32, t: &[u8; 32], rng: &mut R) -> Result<Vec<u8>> {
    let payload = d.encode();
    let sig = crate::keys::vault_sign(vault_sak, &disclose_msg(t, &payload), &mut *rng);
    seal(buyer_key, TYPE_DISCLOSURE, SELLER_TO_BUYER, seq, t, &[&sig[..], &payload].concat(), rng)
}

/// Buyer: opens a disclosure and checks the vault signature against the disclosed FVK. `t` must
/// be the buyer's own transcript (its key and nonce), which is what catches a relay that swapped
/// keys. Chain checks (record walk, R1 to R3) come next and are the caller's.
pub fn open_disclosure(buyer_sk: &[u8; 32], seq: u32, t: &[u8; 32], msg: &[u8]) -> Result<Disclosure> {
    let pt = open(buyer_sk, TYPE_DISCLOSURE, SELLER_TO_BUYER, seq, t, msg)?;
    if pt.len() < 64 {
        bail!("disclosure too short");
    }
    let (sig, payload) = pt.split_at(64);
    let d = Disclosure::decode(payload)?;
    let fvk = FullViewingKey::from_bytes(&d.vault_fvk).ok_or_else(|| anyhow!("bad vault FVK"))?;
    crate::keys::vault_verify(&fvk, &disclose_msg(t, payload), sig.try_into()?).map_err(|_| anyhow!("disclosure signature does not verify for this transcript"))?;
    Ok(d)
}

/// Proof of work on an intent: `BLAKE2b-256(personal "CarrierIntentPoW", T || LE64(stamp))` has at
/// least `bits` leading zero bits. The order book sets `bits`.
pub fn pow_ok(t: &[u8; 32], bits: u32, stamp: u64) -> bool {
    let h = blake2b32(b"CarrierIntentPoW", &[&t[..], &stamp.to_le_bytes()].concat());
    let mut zeros = 0;
    for byte in h {
        zeros += byte.leading_zeros();
        if byte != 0 {
            break;
        }
    }
    zeros >= bits
}

pub fn pow_find(t: &[u8; 32], bits: u32) -> u64 {
    (0u64..).find(|s| pow_ok(t, bits, *s)).unwrap()
}

/// Nonces seen per listing, kept until the listing expires (SPEC 3.2). Seller, buyer and relay
/// each keep one.
#[derive(Default)]
pub struct ReplaySet {
    by_listing: HashMap<[u8; 32], (u64, HashSet<[u8; 32]>)>,
}

impl ReplaySet {
    /// True the first time `nonce` is seen for an open listing. Forgets listings expired at `tip`.
    pub fn insert(&mut self, listing_id: [u8; 32], listing_expiry: u64, nonce: [u8; 32], tip: u64) -> bool {
        self.by_listing.retain(|_, (exp, _)| *exp > tip);
        listing_expiry > tip && self.by_listing.entry(listing_id).or_insert_with(|| (listing_expiry, HashSet::new())).1.insert(nonce)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Lease {
    /// This intent holds the listing until `until` (unix seconds).
    Granted { until: u64 },
    /// Another intent holds it until `until`; wait in the queue.
    Reserved { until: u64 },
}

/// One lease per listing (X6), keyed by the holder's transcript. Requests from the holder renew
/// it; others are told until when the listing is reserved.
#[derive(Default)]
pub struct Leases {
    by_listing: HashMap<[u8; 32], ([u8; 32], u64)>,
}

impl Leases {
    pub fn request(&mut self, listing_id: [u8; 32], t: [u8; 32], now: u64) -> Lease {
        match self.by_listing.get(&listing_id) {
            Some((holder, until)) if *until > now && *holder != t => Lease::Reserved { until: *until },
            _ => {
                let until = now.saturating_add(LEASE_SECS);
                self.by_listing.insert(listing_id, (t, until));
                Lease::Granted { until }
            }
        }
    }

    /// The transcript allowed to receive the disclosure and post a PCZT now, if any.
    pub fn holder(&self, listing_id: &[u8; 32], now: u64) -> Option<[u8; 32]> {
        self.by_listing.get(listing_id).filter(|(_, until)| *until > now).map(|(t, _)| *t)
    }

    pub fn release(&mut self, listing_id: &[u8; 32]) {
        self.by_listing.remove(listing_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spending_key;
    use rand_core::OsRng;

    fn terms(seller_key: [u8; 32], buyer_key: [u8; 32], nonce: [u8; 32]) -> Terms {
        Terms { listing_id: [1; 32], coll: [2; 32], num: 1, price: 10_000, expiry_height: 4_500_000, seller_key, buyer_key, nonce }
    }

    #[test]
    fn seal_open_binds_aad() {
        let (sk, pk) = channel_keypair(&mut OsRng);
        let t = transcript(&terms([3; 32], pk, [4; 32]));
        let m = seal(&pk, TYPE_PCZT, BUYER_TO_SELLER, 0, &t, b"pczt bytes", &mut OsRng).unwrap();
        assert_eq!(open(&sk, TYPE_PCZT, BUYER_TO_SELLER, 0, &t, &m).unwrap(), b"pczt bytes");
        assert!(open(&sk, TYPE_DISCLOSURE, BUYER_TO_SELLER, 0, &t, &m).is_err()); // type
        assert!(open(&sk, TYPE_PCZT, SELLER_TO_BUYER, 0, &t, &m).is_err()); // direction
        assert!(open(&sk, TYPE_PCZT, BUYER_TO_SELLER, 1, &t, &m).is_err()); // sequence
        assert!(open(&sk, TYPE_PCZT, BUYER_TO_SELLER, 0, &[0; 32], &m).is_err()); // transcript
        let (other, _) = channel_keypair(&mut OsRng);
        assert!(open(&other, TYPE_PCZT, BUYER_TO_SELLER, 0, &t, &m).is_err()); // key
        let mut bad = m.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(open(&sk, TYPE_PCZT, BUYER_TO_SELLER, 0, &t, &bad).is_err()); // ciphertext
    }

    #[test]
    fn disclosure_signed_and_relay_swap_caught() {
        let vault_sk = spending_key(&[5; 32], 1);
        let d = Disclosure { vault_fvk: FullViewingKey::from(&vault_sk).to_bytes(), carrier_txid: [6; 32], carrier_action: 2, pay_addr: [7; 43], package: b"{}".to_vec() };
        assert_eq!(Disclosure::decode(&d.encode()).unwrap(), d);
        assert!(Disclosure::decode(&d.encode()[..50]).is_err());
        let sak = SpendAuthorizingKey::from(&vault_sk);
        let (seller_sk, seller_pk) = channel_keypair(&mut OsRng);
        let (buyer_sk, buyer_pk) = channel_keypair(&mut OsRng);
        let _ = seller_sk;
        let t = transcript(&terms(seller_pk, buyer_pk, [8; 32]));
        let m = seal_disclosure(&d, &sak, &buyer_pk, 0, &t, &mut OsRng).unwrap();
        assert_eq!(open_disclosure(&buyer_sk, 0, &t, &m).unwrap(), d);
        // A relay swaps the buyer's key for its own in the intent: the seller then signs and
        // encrypts for the relay's transcript. Re-encrypting to the buyer under the buyer's
        // transcript fails the signature check.
        let (relay_sk, relay_pk) = channel_keypair(&mut OsRng);
        let t_relay = transcript(&terms(seller_pk, relay_pk, [8; 32]));
        let to_relay = seal_disclosure(&d, &sak, &relay_pk, 0, &t_relay, &mut OsRng).unwrap();
        let pt = open(&relay_sk, TYPE_DISCLOSURE, SELLER_TO_BUYER, 0, &t_relay, &to_relay).unwrap();
        let forwarded = seal(&buyer_pk, TYPE_DISCLOSURE, SELLER_TO_BUYER, 0, &t, &pt, &mut OsRng).unwrap();
        assert!(open_disclosure(&buyer_sk, 0, &t, &forwarded).is_err());
        // A signature from another vault key fails too.
        let other_sak = SpendAuthorizingKey::from(&spending_key(&[5; 32], 2));
        let m2 = seal_disclosure(&d, &other_sak, &buyer_pk, 0, &t, &mut OsRng).unwrap();
        assert!(open_disclosure(&buyer_sk, 0, &t, &m2).is_err());
    }

    #[test]
    fn transcript_binds_every_field() {
        let base = transcript(&terms([3; 32], [4; 32], [5; 32]));
        assert_ne!(base, transcript(&terms([3; 32], [4; 32], [6; 32])));
        assert_ne!(base, transcript(&terms([3; 32], [9; 32], [5; 32])));
        let mut t = terms([3; 32], [4; 32], [5; 32]);
        t.price = 10_001;
        assert_ne!(base, transcript(&t));
    }

    #[test]
    fn pow_replay_lease() {
        let t = [3u8; 32];
        let s = pow_find(&t, 12);
        assert!(pow_ok(&t, 12, s) && !pow_ok(&[4u8; 32], 40, s));
        let mut r = ReplaySet::default();
        assert!(r.insert([1; 32], 100, [9; 32], 50));
        assert!(!r.insert([1; 32], 100, [9; 32], 51)); // replay
        assert!(r.insert([2; 32], 100, [9; 32], 51)); // other listing
        assert!(!r.insert([1; 32], 100, [8; 32], 100)); // listing expired
        let mut l = Leases::default();
        assert_eq!(l.request([1; 32], [7; 32], 1_000), Lease::Granted { until: 1_000 + LEASE_SECS });
        assert_eq!(l.request([1; 32], [8; 32], 1_001), Lease::Reserved { until: 1_000 + LEASE_SECS });
        assert_eq!(l.request([1; 32], [7; 32], 1_002), Lease::Granted { until: 1_002 + LEASE_SECS }); // renew
        assert_eq!(l.holder(&[1; 32], 1_003), Some([7; 32]));
        assert_eq!(l.request([1; 32], [8; 32], 1_002 + LEASE_SECS), Lease::Granted { until: 1_002 + 2 * LEASE_SECS }); // lapsed
        l.release(&[1; 32]);
        assert_eq!(l.holder(&[1; 32], 1_003 + LEASE_SECS), None);
    }
}
