//! The order book's state and rules, without HTTP. It holds only what SPEC 3.1 and 3.2 allow:
//! public listings, intents (ephemeral key, nonce, proof of work) and opaque ciphertext. It never
//! sees a vault FVK, a note, a record, a pay address or a user key.
//!
//! Write rules (Codex re-review 2026-10-05, #11): a listing needs a proof-of-work stamp over its
//! id; only the seller (delete token) writes seller-to-buyer messages and only the intent's
//! author (write token) writes buyer-to-seller ones; each (type, direction, sequence) is written
//! once; bodies have per-type caps and all bodies share one byte budget; intents nobody answered
//! expire after `INTENT_TTL_SECS`.
use carrier_core::channel;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_LISTINGS: usize = 10_000;
pub const MAX_INTENTS_PER_LISTING: usize = 32;
/// Messages kept per transcript.
pub const MAX_MSGS_PER_TRANSCRIPT: usize = 16;
/// Ciphertext caps per message type: 48 bytes of HPKE overhead plus the plaintext. A redacted
/// 3-Action PCZT is about 15 KB; a disclosure is 244 bytes plus its package (about 500 bytes a hop).
pub const MAX_INTENT_MSG: usize = 4096 + 48;
pub const MAX_DISCLOSURE_MSG: usize = 256 * 1024 + 48;
pub const MAX_PCZT_MSG: usize = 64 * 1024 + 48;
pub const MAX_BODY: usize = MAX_DISCLOSURE_MSG;
/// All message bodies together.
pub const MAX_TOTAL_BYTES: usize = 256 << 20;
/// An intent the seller has not answered is dropped after this (the buyer may post a new one).
pub const INTENT_TTL_SECS: u64 = 10 * 60;
/// Longest listing life, in blocks past the tip (about 9 days on testnet).
pub const MAX_LISTING_TTL: u64 = 10_000;
pub use channel::POW_BITS;

/// Public listing (SPEC 3.1). Byte fields are hex.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Listing {
    pub listing_id: String,
    pub coll: String,
    pub num: u32,
    pub price: u64,
    pub expiry_height: u64,
    pub seller_channel_key: String,
}

#[derive(Deserialize)]
pub struct NewListing {
    #[serde(flatten)]
    pub listing: Listing,
    /// BLAKE2b-256(personal "CarrierListDel01", token): the seller removes the listing and writes
    /// its seller-to-buyer messages with `token`.
    pub delete_token_hash: String,
    /// proof-of-work stamp over the listing id (`channel::pow_ok`, `POW_BITS`)
    pub pow: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Intent {
    pub buyer_channel_key: String,
    pub nonce: String,
    pub pow: u64,
}

#[derive(Deserialize)]
pub struct NewIntent {
    #[serde(flatten)]
    pub intent: Intent,
    /// BLAKE2b-256(personal "CarrierMsgWrite1", token): the buyer writes its messages with `token`.
    pub write_token_hash: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct IntentOut {
    #[serde(flatten)]
    pub intent: Intent,
    pub transcript: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub transcript: String,
    pub kind: u8,
    pub direction: u8,
    pub seq: u32,
    pub body: String,
}

#[derive(Deserialize)]
pub struct NewMessage {
    #[serde(flatten)]
    pub message: Message,
    /// the listing's delete token (seller to buyer) or the intent's write token (buyer to seller)
    pub token: String,
}

/// A refusal: HTTP status and reason.
#[derive(Debug, PartialEq)]
pub struct Reject(pub u16, pub String);

fn bad(why: impl Into<String>) -> Reject {
    Reject(400, why.into())
}

fn h32(s: &str, what: &str) -> Result<[u8; 32], Reject> {
    hex::decode(s).ok().and_then(|b| b.try_into().ok()).ok_or_else(|| bad(format!("{what} must be 32 bytes of hex")))
}

fn token_hash(personal: &[u8; 16], token: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new().hash_length(32).personal(personal).hash(token).as_bytes().try_into().unwrap()
}

pub fn delete_hash(token: &[u8]) -> [u8; 32] {
    token_hash(b"CarrierListDel01", token)
}

pub fn write_hash(token: &[u8]) -> [u8; 32] {
    token_hash(b"CarrierMsgWrite1", token)
}

struct Entry {
    listing: Listing,
    delete_hash: [u8; 32],
    intents: Vec<IntentOut>,
    nonces: HashSet<[u8; 32]>,
}

/// Per transcript: the buyer's write token hash, when the intent arrived, and its messages.
struct Thread {
    listing: [u8; 32],
    write_hash: [u8; 32],
    since: u64,
    msgs: Vec<Message>,
}

#[derive(Default)]
pub struct Book {
    listings: HashMap<[u8; 32], Entry>,
    by_t: HashMap<[u8; 32], Thread>,
    bytes: usize,
}

impl Book {
    pub fn add_listing(&mut self, n: NewListing, tip: u64) -> Result<(), Reject> {
        let l = n.listing;
        let (id, coll, key) = (h32(&l.listing_id, "listing_id")?, h32(&l.coll, "coll")?, h32(&l.seller_channel_key, "seller_channel_key")?);
        if channel::listing_id(&coll, l.num, l.price, l.expiry_height, &key) != id {
            return Err(bad("listing_id does not match the listing's terms"));
        }
        if l.num == 0 {
            return Err(bad("item number must be positive"));
        }
        // SPEC D4 default (until decided): free transfers only on testnet, no TAZ prices.
        if l.price != 0 {
            return Err(bad("testnet listings are free transfers: price must be 0 (SPEC D4)"));
        }
        if !channel::pow_ok(&id, POW_BITS, n.pow) {
            return Err(bad(format!("listing proof of work needs {POW_BITS} bits")));
        }
        if l.expiry_height <= tip || l.expiry_height > tip.saturating_add(MAX_LISTING_TTL) {
            return Err(bad(format!("expiry_height must be in {}..={}", tip + 1, tip.saturating_add(MAX_LISTING_TTL))));
        }
        if self.listings.contains_key(&id) {
            return Err(Reject(409, "listing exists".into()));
        }
        if self.listings.len() >= MAX_LISTINGS {
            return Err(Reject(503, "order book full".into()));
        }
        let delete_hash = h32(&n.delete_token_hash, "delete_token_hash")?;
        self.listings.insert(id, Entry { listing: l, delete_hash, intents: vec![], nonces: HashSet::new() });
        Ok(())
    }

    pub fn listings(&self, coll: Option<&str>) -> Vec<Listing> {
        let mut v: Vec<Listing> = self.listings.values().map(|e| e.listing.clone()).filter(|l| coll.is_none_or(|c| l.coll == c)).collect();
        v.sort_by(|a, b| (&a.coll, a.num, &a.listing_id).cmp(&(&b.coll, b.num, &b.listing_id)));
        v
    }

    pub fn listing(&self, id: &str) -> Result<Listing, Reject> {
        self.listings.get(&h32(id, "listing_id")?).map(|e| e.listing.clone()).ok_or_else(|| Reject(404, "no such listing".into()))
    }

    /// Removes a listing (sold or withdrawn) with its intents and messages. No public event.
    pub fn remove(&mut self, id: &str, token: &str) -> Result<(), Reject> {
        let id = h32(id, "listing_id")?;
        let token = hex::decode(token).map_err(|_| bad("token must be hex"))?;
        match self.listings.get(&id) {
            None => Err(Reject(404, "no such listing".into())),
            Some(e) if e.delete_hash != delete_hash(&token) => Err(Reject(403, "wrong delete token".into())),
            Some(_) => {
                self.drop_listing(&id);
                Ok(())
            }
        }
    }

    pub fn add_intent(&mut self, id: &str, n: NewIntent, tip: u64, now: u64) -> Result<String, Reject> {
        let (i, write_hash) = (n.intent, h32(&n.write_token_hash, "write_token_hash")?);
        let id = h32(id, "listing_id")?;
        let e = self.listings.get_mut(&id).ok_or_else(|| Reject(404, "no such listing".into()))?;
        if e.listing.expiry_height <= tip {
            return Err(Reject(410, "listing expired".into()));
        }
        let (key, nonce) = (h32(&i.buyer_channel_key, "buyer_channel_key")?, h32(&i.nonce, "nonce")?);
        let l = &e.listing;
        let t = channel::transcript(&channel::Terms {
            listing_id: id,
            coll: h32(&l.coll, "coll")?,
            num: l.num,
            price: l.price,
            expiry_height: l.expiry_height,
            seller_key: h32(&l.seller_channel_key, "seller_channel_key")?,
            buyer_key: key,
            nonce,
        });
        if !channel::pow_ok(&t, POW_BITS, i.pow) {
            return Err(bad(format!("proof of work needs {POW_BITS} bits")));
        }
        if e.nonces.contains(&nonce) {
            return Err(Reject(409, "nonce already used for this listing".into()));
        }
        if e.intents.len() >= MAX_INTENTS_PER_LISTING {
            return Err(Reject(429, "too many intents for this listing".into()));
        }
        e.nonces.insert(nonce);
        e.intents.push(IntentOut { intent: i, transcript: hex::encode(t) });
        self.by_t.insert(t, Thread { listing: id, write_hash, since: now, msgs: vec![] });
        Ok(hex::encode(t))
    }

    pub fn intents(&self, id: &str) -> Result<Vec<IntentOut>, Reject> {
        Ok(self.listings.get(&h32(id, "listing_id")?).ok_or_else(|| Reject(404, "no such listing".into()))?.intents.clone())
    }

    pub fn add_message(&mut self, n: NewMessage) -> Result<(), Reject> {
        let m = n.message;
        let t = h32(&m.transcript, "transcript")?;
        let th = self.by_t.get(&t).ok_or_else(|| Reject(404, "no intent with this transcript".into()))?;
        let cap = match (m.kind, m.direction) {
            (channel::TYPE_INTENT, channel::BUYER_TO_SELLER) => MAX_INTENT_MSG,
            (channel::TYPE_DISCLOSURE, channel::SELLER_TO_BUYER) => MAX_DISCLOSURE_MSG,
            (channel::TYPE_PCZT, channel::BUYER_TO_SELLER) => MAX_PCZT_MSG,
            _ => return Err(bad("type and direction do not match (1 or 3 buyer to seller, 2 seller to buyer)")),
        };
        let token = hex::decode(&n.token).map_err(|_| bad("token must be hex"))?;
        let allowed = if m.direction == channel::SELLER_TO_BUYER { self.listings.get(&th.listing).map(|e| e.delete_hash) } else { Some(th.write_hash) };
        if allowed != Some(if m.direction == channel::SELLER_TO_BUYER { delete_hash(&token) } else { write_hash(&token) }) {
            return Err(Reject(403, "wrong write token for this transcript and direction".into()));
        }
        let size = m.body.len() / 2;
        if m.body.len() % 2 != 0 || size < 48 || size > cap || hex::decode(&m.body).is_err() {
            return Err(bad(format!("body must be 48 to {cap} bytes of hex for type {}", m.kind)));
        }
        if th.msgs.iter().any(|x| (x.kind, x.direction, x.seq) == (m.kind, m.direction, m.seq)) {
            return Err(Reject(409, "a message with this type, direction and sequence exists".into()));
        }
        if th.msgs.len() >= MAX_MSGS_PER_TRANSCRIPT {
            return Err(Reject(429, "too many messages for this transcript".into()));
        }
        if self.bytes + size > MAX_TOTAL_BYTES {
            return Err(Reject(503, "order book full".into()));
        }
        self.bytes += size;
        self.by_t.get_mut(&t).expect("checked above").msgs.push(m);
        Ok(())
    }

    pub fn messages(&self, t: &str, kind: Option<u8>, direction: Option<u8>) -> Result<Vec<Message>, Reject> {
        let t = h32(t, "transcript")?;
        Ok(self.by_t.get(&t).into_iter().flat_map(|th| &th.msgs).filter(|m| kind.is_none_or(|k| m.kind == k) && direction.is_none_or(|d| m.direction == d)).cloned().collect())
    }

    /// Drops listings that expired at `tip` (with their intents and messages) and intents older
    /// than `INTENT_TTL_SECS` that the seller has not answered. Returns the listings dropped.
    pub fn prune(&mut self, tip: u64, now: u64) -> usize {
        let gone: Vec<[u8; 32]> = self.listings.iter().filter(|(_, e)| e.listing.expiry_height <= tip).map(|(id, _)| *id).collect();
        for id in &gone {
            self.drop_listing(id);
        }
        let stale: Vec<[u8; 32]> =
            self.by_t.iter().filter(|(_, th)| now >= th.since.saturating_add(INTENT_TTL_SECS) && !th.msgs.iter().any(|m| m.direction == channel::SELLER_TO_BUYER)).map(|(t, _)| *t).collect();
        for t in stale {
            self.drop_thread(&t);
        }
        gone.len()
    }

    fn drop_thread(&mut self, t: &[u8; 32]) {
        if let Some(th) = self.by_t.remove(t) {
            self.bytes -= th.msgs.iter().map(|m| m.body.len() / 2).sum::<usize>();
            if let Some(e) = self.listings.get_mut(&th.listing) {
                let ts = hex::encode(t);
                e.intents.retain(|i| i.transcript != ts);
            }
        }
    }

    fn drop_listing(&mut self, id: &[u8; 32]) {
        if let Some(e) = self.listings.remove(id) {
            for i in e.intents {
                if let Ok(t) = h32(&i.transcript, "transcript") {
                    if let Some(th) = self.by_t.remove(&t) {
                        self.bytes -= th.msgs.iter().map(|m| m.body.len() / 2).sum::<usize>();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIP: u64 = 4_000_000;
    const NOW: u64 = 1_800_000_000;

    fn listing(price: u64, expiry: u64) -> (NewListing, [u8; 32]) {
        let (coll, key) = ([2u8; 32], [3u8; 32]);
        let id = channel::listing_id(&coll, 1, price, expiry, &key);
        let l = Listing { listing_id: hex::encode(id), coll: hex::encode(coll), num: 1, price, expiry_height: expiry, seller_channel_key: hex::encode(key) };
        (NewListing { listing: l, delete_token_hash: hex::encode(delete_hash(b"secret")), pow: channel::pow_find(&id, POW_BITS) }, id)
    }

    fn transcript_of(l: &Listing, i: &Intent) -> [u8; 32] {
        channel::transcript(&channel::Terms {
            listing_id: h32(&l.listing_id, "").unwrap(),
            coll: h32(&l.coll, "").unwrap(),
            num: l.num,
            price: l.price,
            expiry_height: l.expiry_height,
            seller_key: h32(&l.seller_channel_key, "").unwrap(),
            buyer_key: h32(&i.buyer_channel_key, "").unwrap(),
            nonce: h32(&i.nonce, "").unwrap(),
        })
    }

    fn intent(l: &Listing, nonce: u8) -> NewIntent {
        let mut i = Intent { buyer_channel_key: hex::encode([4u8; 32]), nonce: hex::encode([nonce; 32]), pow: 0 };
        i.pow = channel::pow_find(&transcript_of(l, &i), POW_BITS);
        NewIntent { intent: i, write_token_hash: hex::encode(write_hash(b"buyer")) }
    }

    #[test]
    fn listings_are_checked() {
        let mut b = Book::default();
        let (mut n, _) = listing(0, TIP + 100);
        n.listing.num = 2; // id no longer matches
        assert_eq!(b.add_listing(n, TIP).unwrap_err().0, 400);
        assert_eq!(b.add_listing(listing(10_000, TIP + 100).0, TIP).unwrap_err().1, "testnet listings are free transfers: price must be 0 (SPEC D4)");
        let (mut weak, _) = listing(0, TIP + 100);
        weak.pow = 0; // deterministic inputs; stamp 0 does not reach POW_BITS for them
        assert!(b.add_listing(weak, TIP).unwrap_err().1.contains("proof of work"));
        assert_eq!(b.add_listing(listing(0, TIP).0, TIP).unwrap_err().0, 400); // expired
        assert_eq!(b.add_listing(listing(0, TIP + MAX_LISTING_TTL + 1).0, TIP).unwrap_err().0, 400); // too long
        b.add_listing(listing(0, TIP + 100).0, TIP).unwrap();
        assert_eq!(b.add_listing(listing(0, TIP + 100).0, TIP).unwrap_err().0, 409);
        assert_eq!(b.listings(None).len(), 1);
        assert_eq!(b.listings(Some(&hex::encode([9u8; 32]))).len(), 0);
    }

    #[test]
    fn intents_messages_prune_remove() {
        let mut b = Book::default();
        let (n, id) = listing(0, TIP + 100);
        let l = n.listing.clone();
        b.add_listing(n, TIP).unwrap();
        let id = hex::encode(id);
        let mut weak = intent(&l, 1);
        weak.intent.pow = 0;
        assert_eq!(b.add_intent(&id, weak, TIP, NOW).unwrap_err().0, 400);
        // A stamp good enough for 16 bits but not for POW_BITS (20) is refused.
        let mut low = intent(&l, 5);
        let tl = transcript_of(&l, &low.intent);
        low.intent.pow = (0u64..).find(|&st| channel::pow_ok(&tl, 16, st) && !channel::pow_ok(&tl, POW_BITS, st)).unwrap();
        assert_eq!(b.add_intent(&id, low, TIP, NOW).unwrap_err().0, 400);
        let t = b.add_intent(&id, intent(&l, 2), TIP, NOW).unwrap();
        assert_eq!(b.add_intent(&id, intent(&l, 2), TIP, NOW).unwrap_err().0, 409); // replayed nonce
        assert_eq!(b.add_intent(&id, intent(&l, 3), TIP + 100, NOW).unwrap_err().0, 410); // listing expired
        let msg = |kind, direction, seq, body: &str, token: &[u8]| NewMessage { message: Message { transcript: t.clone(), kind, direction, seq, body: body.into() }, token: hex::encode(token) };
        let body = "00".repeat(60);
        // Writes need the right token for the direction: seller = delete token, buyer = write token.
        assert_eq!(b.add_message(msg(channel::TYPE_DISCLOSURE, channel::SELLER_TO_BUYER, 0, &body, b"buyer")).unwrap_err().0, 403);
        assert_eq!(b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, 0, &body, b"secret")).unwrap_err().0, 403);
        b.add_message(msg(channel::TYPE_DISCLOSURE, channel::SELLER_TO_BUYER, 0, &body, b"secret")).unwrap();
        assert_eq!(b.add_message(msg(channel::TYPE_DISCLOSURE, channel::SELLER_TO_BUYER, 0, &body, b"secret")).unwrap_err().0, 409); // same slot
        assert_eq!(b.add_message(msg(channel::TYPE_DISCLOSURE, channel::BUYER_TO_SELLER, 0, &body, b"buyer")).unwrap_err().0, 400);
        assert_eq!(b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, 0, "00", b"buyer")).unwrap_err().0, 400); // too short
        assert_eq!(b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, 0, &"00".repeat(MAX_PCZT_MSG + 1), b"buyer")).unwrap_err().0, 400); // too long for a PCZT
        assert_eq!(b.add_message(msg(channel::TYPE_INTENT, channel::BUYER_TO_SELLER, 0, &"00".repeat(MAX_INTENT_MSG + 1), b"buyer")).unwrap_err().0, 400);
        let mut unknown = msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, 0, &body, b"buyer");
        unknown.message.transcript = hex::encode([7u8; 32]);
        assert_eq!(b.add_message(unknown).unwrap_err().0, 404);
        for seq in 1..MAX_MSGS_PER_TRANSCRIPT as u32 {
            b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, seq, &body, b"buyer")).unwrap();
        }
        assert_eq!(b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, 99, &body, b"buyer")).unwrap_err().0, 429);
        assert_eq!(b.bytes, 60 * MAX_MSGS_PER_TRANSCRIPT);
        assert_eq!(b.messages(&t, Some(channel::TYPE_DISCLOSURE), Some(channel::SELLER_TO_BUYER)).unwrap().len(), 1);
        // An answered intent survives the intent TTL; an unanswered one does not.
        let t2 = b.add_intent(&id, intent(&l, 4), TIP, NOW).unwrap();
        assert_eq!(b.prune(TIP, NOW + INTENT_TTL_SECS), 0);
        assert_eq!(b.intents(&id).unwrap().iter().map(|i| i.transcript.clone()).collect::<Vec<_>>(), vec![t.clone()]);
        assert_eq!(b.messages(&t2, None, None).unwrap().len(), 0);
        assert_eq!(b.remove(&id, &hex::encode(b"wrong")).unwrap_err().0, 403);
        b.remove(&id, &hex::encode(b"secret")).unwrap();
        assert!(b.messages(&t, None, None).unwrap().is_empty() && b.listings(None).is_empty() && b.bytes == 0);
        b.add_listing(listing(0, TIP + 50).0, TIP).unwrap();
        assert_eq!((b.prune(TIP + 49, NOW), b.prune(TIP + 50, NOW)), (0, 1));
    }
}
