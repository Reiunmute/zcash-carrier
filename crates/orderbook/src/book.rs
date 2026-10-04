//! The order book's state and rules, without HTTP. It holds only what SPEC 3.1 and 3.2 allow:
//! public listings, intents (ephemeral key, nonce, proof of work) and opaque ciphertext. It never
//! sees a vault FVK, a note, a record, a pay address or a user key.
use carrier_core::channel;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_LISTINGS: usize = 10_000;
pub const MAX_INTENTS_PER_LISTING: usize = 32;
/// Messages kept per transcript. Receivers try each one, so junk posted under someone's
/// transcript can delay but not replace the real message.
pub const MAX_MSGS_PER_TRANSCRIPT: usize = 16;
pub const MAX_BODY: usize = channel::MAX_PLAINTEXT + 48;
/// Longest listing life, in blocks past the tip (about 9 days on testnet).
pub const MAX_LISTING_TTL: u64 = 10_000;
pub use channel::POW_BITS;
const MAX_MONEY: u64 = 21_000_000 * 100_000_000;

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
    /// BLAKE2b-256(personal "CarrierListDel01", token): the seller removes the listing with `token`.
    pub delete_token_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Intent {
    pub buyer_channel_key: String,
    pub nonce: String,
    pub pow: u64,
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

/// A refusal: HTTP status and reason.
#[derive(Debug, PartialEq)]
pub struct Reject(pub u16, pub String);

fn bad(why: impl Into<String>) -> Reject {
    Reject(400, why.into())
}

fn h32(s: &str, what: &str) -> Result<[u8; 32], Reject> {
    hex::decode(s).ok().and_then(|b| b.try_into().ok()).ok_or_else(|| bad(format!("{what} must be 32 bytes of hex")))
}

pub fn delete_hash(token: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new().hash_length(32).personal(b"CarrierListDel01").hash(token).as_bytes().try_into().unwrap()
}

struct Entry {
    listing: Listing,
    delete_hash: [u8; 32],
    intents: Vec<IntentOut>,
    nonces: HashSet<[u8; 32]>,
}

#[derive(Default)]
pub struct Book {
    listings: HashMap<[u8; 32], Entry>,
    /// transcript -> listing id
    by_t: HashMap<[u8; 32], [u8; 32]>,
    msgs: HashMap<[u8; 32], Vec<Message>>,
}

impl Book {
    pub fn add_listing(&mut self, n: NewListing, tip: u64) -> Result<(), Reject> {
        let l = n.listing;
        let (id, coll, key) = (h32(&l.listing_id, "listing_id")?, h32(&l.coll, "coll")?, h32(&l.seller_channel_key, "seller_channel_key")?);
        if channel::listing_id(&coll, l.num, l.price, l.expiry_height, &key) != id {
            return Err(bad("listing_id does not match the listing's terms"));
        }
        if l.num == 0 || l.price == 0 || l.price > MAX_MONEY {
            return Err(bad("item number and price must be positive, price at most 21M ZEC"));
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

    pub fn add_intent(&mut self, id: &str, i: Intent, tip: u64) -> Result<String, Reject> {
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
        self.by_t.insert(t, id);
        Ok(hex::encode(t))
    }

    pub fn intents(&self, id: &str) -> Result<Vec<IntentOut>, Reject> {
        Ok(self.listings.get(&h32(id, "listing_id")?).ok_or_else(|| Reject(404, "no such listing".into()))?.intents.clone())
    }

    pub fn add_message(&mut self, m: Message) -> Result<(), Reject> {
        let t = h32(&m.transcript, "transcript")?;
        if !self.by_t.contains_key(&t) {
            return Err(Reject(404, "no intent with this transcript".into()));
        }
        let ok = matches!(
            (m.kind, m.direction),
            (channel::TYPE_INTENT, channel::BUYER_TO_SELLER) | (channel::TYPE_DISCLOSURE, channel::SELLER_TO_BUYER) | (channel::TYPE_PCZT, channel::BUYER_TO_SELLER)
        );
        if !ok {
            return Err(bad("type and direction do not match (1 or 3 buyer to seller, 2 seller to buyer)"));
        }
        let n = m.body.len() / 2;
        if m.body.len() % 2 != 0 || n < 48 || n > MAX_BODY || hex::decode(&m.body).is_err() {
            return Err(bad(format!("body must be 48 to {MAX_BODY} bytes of hex")));
        }
        let v = self.msgs.entry(t).or_default();
        if v.len() >= MAX_MSGS_PER_TRANSCRIPT {
            return Err(Reject(429, "too many messages for this transcript".into()));
        }
        v.push(m);
        Ok(())
    }

    pub fn messages(&self, t: &str, kind: Option<u8>, direction: Option<u8>) -> Result<Vec<Message>, Reject> {
        let t = h32(t, "transcript")?;
        Ok(self.msgs.get(&t).into_iter().flatten().filter(|m| kind.is_none_or(|k| m.kind == k) && direction.is_none_or(|d| m.direction == d)).cloned().collect())
    }

    /// Drops listings that expired at `tip`, with their intents and messages.
    pub fn prune(&mut self, tip: u64) -> usize {
        let gone: Vec<[u8; 32]> = self.listings.iter().filter(|(_, e)| e.listing.expiry_height <= tip).map(|(id, _)| *id).collect();
        for id in &gone {
            self.drop_listing(id);
        }
        gone.len()
    }

    fn drop_listing(&mut self, id: &[u8; 32]) {
        if let Some(e) = self.listings.remove(id) {
            for i in e.intents {
                if let Ok(t) = h32(&i.transcript, "transcript") {
                    self.by_t.remove(&t);
                    self.msgs.remove(&t);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIP: u64 = 4_000_000;

    fn listing(price: u64, expiry: u64) -> (NewListing, [u8; 32]) {
        let (coll, key) = ([2u8; 32], [3u8; 32]);
        let id = channel::listing_id(&coll, 1, price, expiry, &key);
        let l = Listing { listing_id: hex::encode(id), coll: hex::encode(coll), num: 1, price, expiry_height: expiry, seller_channel_key: hex::encode(key) };
        (NewListing { listing: l, delete_token_hash: hex::encode(delete_hash(b"secret")) }, id)
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

    fn intent(l: &Listing, nonce: u8) -> Intent {
        let (key, nonce) = ([4u8; 32], [nonce; 32]);
        let t = channel::transcript(&channel::Terms {
            listing_id: h32(&l.listing_id, "").unwrap(),
            coll: h32(&l.coll, "").unwrap(),
            num: l.num,
            price: l.price,
            expiry_height: l.expiry_height,
            seller_key: h32(&l.seller_channel_key, "").unwrap(),
            buyer_key: key,
            nonce,
        });
        Intent { buyer_channel_key: hex::encode(key), nonce: hex::encode(nonce), pow: channel::pow_find(&t, POW_BITS) }
    }

    #[test]
    fn listings_are_checked() {
        let mut b = Book::default();
        let (mut n, _) = listing(10_000, TIP + 100);
        n.listing.price = 1; // id no longer matches
        assert_eq!(b.add_listing(n, TIP).unwrap_err().0, 400);
        assert_eq!(b.add_listing(listing(10_000, TIP).0, TIP).unwrap_err().0, 400); // expired
        assert_eq!(b.add_listing(listing(10_000, TIP + MAX_LISTING_TTL + 1).0, TIP).unwrap_err().0, 400); // too long
        b.add_listing(listing(10_000, TIP + 100).0, TIP).unwrap();
        assert_eq!(b.add_listing(listing(10_000, TIP + 100).0, TIP).unwrap_err().0, 409);
        assert_eq!(b.listings(None).len(), 1);
        assert_eq!(b.listings(Some(&hex::encode([9u8; 32]))).len(), 0);
    }

    #[test]
    fn intents_messages_prune_remove() {
        let mut b = Book::default();
        let (n, id) = listing(10_000, TIP + 100);
        let l = n.listing.clone();
        b.add_listing(n, TIP).unwrap();
        let id = hex::encode(id);
        let mut weak = intent(&l, 1);
        weak.pow = 0; // deterministic inputs; stamp 0 does not reach POW_BITS for them
        assert_eq!(b.add_intent(&id, weak, TIP).unwrap_err().0, 400);
        // A stamp good enough for 16 bits but not for POW_BITS (20) is refused.
        let mut low = intent(&l, 5);
        let tl = transcript_of(&l, &low);
        low.pow = (0u64..).find(|&st| channel::pow_ok(&tl, 16, st) && !channel::pow_ok(&tl, POW_BITS, st)).unwrap();
        assert_eq!(b.add_intent(&id, low, TIP).unwrap_err().0, 400);
        let t = b.add_intent(&id, intent(&l, 2), TIP).unwrap();
        assert_eq!(b.add_intent(&id, intent(&l, 2), TIP).unwrap_err().0, 409); // replayed nonce
        assert_eq!(b.add_intent(&id, intent(&l, 3), TIP + 100).unwrap_err().0, 410); // listing expired
        let msg = |kind, direction, body: &str| Message { transcript: t.clone(), kind, direction, seq: 0, body: body.into() };
        let body = "00".repeat(60);
        b.add_message(msg(channel::TYPE_DISCLOSURE, channel::SELLER_TO_BUYER, &body)).unwrap();
        assert_eq!(b.add_message(msg(channel::TYPE_DISCLOSURE, channel::BUYER_TO_SELLER, &body)).unwrap_err().0, 400);
        assert_eq!(b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, "00")).unwrap_err().0, 400); // too short
        let mut unknown = msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, &body);
        unknown.transcript = hex::encode([7u8; 32]);
        assert_eq!(b.add_message(unknown).unwrap_err().0, 404);
        for _ in 1..MAX_MSGS_PER_TRANSCRIPT {
            b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, &body)).unwrap();
        }
        assert_eq!(b.add_message(msg(channel::TYPE_PCZT, channel::BUYER_TO_SELLER, &body)).unwrap_err().0, 429);
        assert_eq!(b.messages(&t, Some(channel::TYPE_DISCLOSURE), Some(channel::SELLER_TO_BUYER)).unwrap().len(), 1);
        assert_eq!(b.remove(&id, &hex::encode(b"wrong")).unwrap_err().0, 403);
        b.remove(&id, &hex::encode(b"secret")).unwrap();
        assert!(b.messages(&t, None, None).unwrap().is_empty() && b.listings(None).is_empty());
        b.add_listing(listing(10_000, TIP + 50).0, TIP).unwrap();
        assert_eq!((b.prune(TIP + 49), b.prune(TIP + 50)), (0, 1));
    }
}
