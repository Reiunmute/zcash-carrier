//! carrier-sale: the two-party atomic sale (marketplace SPEC section 3) as pure functions, so the
//! CLI and the browser share one implementation. The buyer builds the joint tx as a PCZT, proves
//! it, signs only its funding spend and redacts it; the seller checks the PCZT against its
//! listing, signs the carrier spend and extracts the tx (which verifies the proof and every
//! signature). Chain facts (R1 to R3, witnesses, the tip, liveness) are inputs: callers fetch
//! and verify them first.
use anyhow::{anyhow, bail, Result};
use carrier_core::decrypt_bundle_ivk;
use orchard::{
    keys::{FullViewingKey, OutgoingViewingKey, SpendAuthorizingKey},
    tree::{Anchor, MerklePath},
    Address, Note,
};
use pczt::{
    roles::{creator::Creator, io_finalizer::IoFinalizer, prover::Prover, redactor::Redactor, signer::Signer, tx_extractor::TransactionExtractor, updater::Updater},
    Pczt,
};
use rand_core::{CryptoRng, RngCore};
use zcash_primitives::transaction::{
    builder::{BuildConfig, BundlePadding, Builder},
    fees::zip317::FeeRule,
    Transaction,
};
use zcash_protocol::{
    consensus::{BlockHeight, BranchId, TEST_NETWORK},
    memo::MemoBytes,
    value::Zatoshis,
};

/// Every sale has 2 spends and 3 outputs, so exactly 3 Actions (SPEC 3, 5).
pub const SALE_ACTIONS: usize = 3;
/// PCZT global proprietary key carrying the listing id the buyer built against.
pub const LISTING_KEY: &str = "carrier/listing-id/v1";
/// Longest PCZT validity a seller accepts, in blocks past the current tip (SPEC 3.4).
pub const MAX_PCZT_TTL: u64 = 40;
/// Builds until the carrier spend and its successor share an Action.
pub const MAX_TRIES: usize = 64;

pub struct Out {
    pub to: Address,
    pub value: u64,
    pub memo: [u8; 512],
    pub ovk: Option<OutgoingViewingKey>,
}

pub struct BuyerSale<'a> {
    /// the seller's vault key, from the listing (the carrier spend stays unsigned)
    pub vault: &'a FullViewingKey,
    pub carrier: Note,
    pub carrier_path: MerklePath,
    pub fund_fvk: &'a FullViewingKey,
    pub fund_sak: &'a SpendAuthorizingKey,
    pub fund: Note,
    pub fund_path: MerklePath,
    pub anchor: Anchor,
    /// successor (shares the carrier spend's Action), payment, change
    pub outs: [Out; 3],
    pub target: BlockHeight,
    pub expiry: BlockHeight,
    pub listing_id: [u8; 32],
    /// byte strings that must not survive redaction (the buyer's key, note fields, addresses)
    pub secrets: Vec<(&'static str, Vec<u8>)>,
}

pub struct BuiltSale {
    pub pczt: Vec<u8>,
    pub carrier_action: usize,
    pub expiry: u32,
}

pub fn proving_key() -> orchard::circuit::ProvingKey {
    orchard::circuit::ProvingKey::build(orchard::bundle::BundleVersion::ironwood_v3().circuit_version())
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Buyer: build, prove, sign the funding spend, redact (SPEC 3.3) and self-check.
pub fn buyer_build<R: RngCore + CryptoRng>(s: BuyerSale, pk: &orchard::circuit::ProvingKey, rng: &mut R, log: &mut dyn FnMut(String)) -> Result<BuiltSale> {
    let mut built = None;
    for attempt in 1..=MAX_TRIES {
        let mut b = Builder::new(
            TEST_NETWORK,
            s.target,
            BuildConfig::Standard {
                sapling_anchor: None,
                orchard_anchor: None,
                ironwood_anchor: Some(s.anchor),
                orchard_padding: BundlePadding::DEFAULT,
                ironwood_padding: BundlePadding::DEFAULT,
            },
        )
        .with_expiry_height(s.expiry);
        b.add_ironwood_spend::<std::convert::Infallible>(s.vault.clone(), s.carrier, s.carrier_path.clone()).map_err(|e| anyhow!("spend: {e}"))?;
        b.add_ironwood_spend::<std::convert::Infallible>(s.fund_fvk.clone(), s.fund, s.fund_path.clone()).map_err(|e| anyhow!("spend: {e}"))?;
        for o in &s.outs {
            let memo = MemoBytes::from_bytes(&o.memo).map_err(|e| anyhow!("memo {e:?}"))?;
            let v = Zatoshis::from_u64(o.value).map_err(|_| anyhow!("amount {} out of range", o.value))?;
            b.add_ironwood_output::<std::convert::Infallible>(o.ovk.clone(), o.to, v, memo).map_err(|e| anyhow!("output: {e}"))?;
        }
        let res = b.build_for_pczt(&mut *rng, &FeeRule::standard()).map_err(|e| anyhow!("build_for_pczt: {e}"))?;
        let m = &res.ironwood_meta;
        let idx = |o: Option<usize>| o.ok_or_else(|| anyhow!("bundle metadata"));
        let (cai, fai) = (idx(m.spend_action_index(0))?, idx(m.spend_action_index(1))?);
        if m.output_action_index(0) != Some(cai) {
            log(format!("  attempt {attempt}: binding not satisfied, rebuilding"));
            continue;
        }
        let (pai, xai) = (idx(m.output_action_index(1))?, idx(m.output_action_index(2))?);
        log(format!("  attempt {attempt}: bound ok, carrier action {cai}, funding action {fai}"));
        let p = Creator::build_from_parts(res.pczt_parts).ok_or_else(|| anyhow!("tx version not PCZT-compatible"))?;
        // Bind the PCZT to the listing version it was built against (not signed; checked by the seller).
        let p = Updater::new(p).update_global_with(|mut g| g.set_proprietary(LISTING_KEY.into(), s.listing_id.to_vec())).finish();
        built = Some((p, cai, fai, pai, xai));
        break;
    }
    let (p, cai, fai, pai, xai) = built.ok_or_else(|| anyhow!("binding not satisfied after {MAX_TRIES} tries"))?;
    let expiry = *p.global().expiry_height();
    log(format!("  pczt expiry_height {expiry}"));
    let p = IoFinalizer::new(p).finalize_io().map_err(|e| anyhow!("io_finalize: {e:?}"))?;
    let p = Prover::new(p).create_ironwood_proof(pk).map_err(|e| anyhow!("prove: {e:?}"))?.finish();
    let mut sg = Signer::new(p).map_err(|e| anyhow!("signer: {e:?}"))?;
    sg.sign_ironwood(fai, s.fund_sak).map_err(|e| anyhow!("sign funding: {e:?}"))?;
    // Redaction (SPEC 3.3): nothing about the buyer beyond what the effects must show.
    let p = Redactor::new(sg.finish())
        .redact_ironwood_with(|mut r| {
            r.redact_action(fai, |mut a| {
                a.clear_spend_fvk();
                a.clear_spend_witness();
                a.clear_spend_zip32_derivation();
                a.clear_spend_recipient();
                a.clear_spend_value();
                a.clear_spend_rho();
                a.clear_spend_rseed();
                a.clear_spend_alpha();
                a.clear_spend_proprietary();
            });
            for oi in [cai, pai, xai] {
                r.redact_action(oi, |mut a| {
                    a.clear_output_recipient();
                    a.clear_output_value();
                    a.clear_output_rseed();
                    a.clear_output_ock();
                    a.clear_output_zip32_derivation();
                    a.clear_output_user_address();
                    a.clear_output_proprietary();
                });
            }
            r.redact_actions(|mut a| {
                a.clear_rcv();
                a.clear_spend_dummy_sk();
            });
        })
        .finish();
    // Self-checks. 1: the buyer alone cannot produce a valid tx (the carrier spend is unsigned).
    match TransactionExtractor::new(p.clone()).extract() {
        Ok(tx) => bail!("UNEXPECTED: buyer alone extracted tx {}", tx.txid()),
        Err(e) => log(format!("  buyer-alone extract refused: {e:?}")),
    }
    // 2: the redacted PCZT still parses for the seller's signer.
    Signer::new(p.clone()).map_err(|e| anyhow!("redacted PCZT no longer signable: {e:?}"))?;
    let bytes = p.serialize().map_err(|e| anyhow!("serialize: {e:?}"))?;
    // 3: none of the buyer's secrets appear in the bytes.
    for (what, b) in &s.secrets {
        if contains(&bytes, b) {
            bail!("redaction check failed: {what} is still in the PCZT");
        }
    }
    log("  redaction check: no funding FVK, funding-note fields or buyer addresses in the PCZT".into());
    Ok(BuiltSale { pczt: bytes, carrier_action: cai, expiry })
}

/// What the seller listed, plus the chain facts it checked (tip). Liveness of the carrier is the
/// caller's check: it needs the chain's nullifier set.
pub struct SellerTerms<'a> {
    pub listing_id: [u8; 32],
    pub listing_expiry: u64,
    pub price: u64,
    pub pay: Address,
    pub pay_fvk: &'a FullViewingKey,
    pub carrier_nf: [u8; 32],
    pub tip: u64,
}

pub struct Checked {
    pub pczt: Pczt,
    pub carrier_action: usize,
    /// Actions paying the listing's pay address
    pub payment_actions: Vec<usize>,
    pub summary: String,
}

/// Seller: every check on the buyer's PCZT before signing (SPEC 3.2 to 3.4). Reads the effects
/// (ciphertexts, nullifiers), not the PCZT's plaintext fields.
pub fn seller_check(bytes: &[u8], t: &SellerTerms) -> Result<Checked> {
    let p = Pczt::parse(bytes).map_err(|e| anyhow!("parse pczt: {e:?}"))?;
    // 0. the PCZT was built against this exact listing version
    if p.global().proprietary().get(LISTING_KEY).map(|v| v.as_slice()) != Some(&t.listing_id[..]) {
        bail!("REFUSED: PCZT was not built against this listing version");
    }
    // 1. the listing is still open
    if t.tip >= t.listing_expiry {
        bail!("REFUSED: listing expired at {} (tip {})", t.listing_expiry, t.tip);
    }
    // 2. PCZT expiry: minable a few blocks from now, and no later than the listing or tip + MAX_PCZT_TTL
    let exp = *p.global().expiry_height() as u64;
    let (Some(soon), Some(cap)) = (t.tip.checked_add(3), t.tip.checked_add(MAX_PCZT_TTL)) else { bail!("bad tip height") };
    if exp < soon {
        bail!("REFUSED: PCZT expired or about to (expiry_height {exp}, tip {})", t.tip);
    }
    if exp > t.listing_expiry || exp > cap {
        bail!("REFUSED: PCZT expiry_height {exp} is past the listing expiry {} or tip + {MAX_PCZT_TTL}", t.listing_expiry);
    }
    // 3. network upgrade
    let next = u32::try_from(t.tip.checked_add(1).ok_or_else(|| anyhow!("bad tip height"))?)?;
    let branch = u32::from(BranchId::for_height(&TEST_NETWORK, BlockHeight::from_u32(next)));
    if *p.global().consensus_branch_id() != branch {
        bail!("REFUSED: consensus branch {:08x} != {branch:08x}", p.global().consensus_branch_id());
    }
    // 4. only an Ironwood bundle, of the sale shape
    if !p.orchard().actions().is_empty() || !p.sapling().spends().is_empty() || !p.sapling().outputs().is_empty() || !p.transparent().inputs().is_empty() || !p.transparent().outputs().is_empty() {
        bail!("REFUSED: PCZT uses a value pool other than Ironwood");
    }
    let fx = p.clone().into_effects().map_err(|e| anyhow!("effects: {e:?}"))?;
    let b = fx.ironwood_bundle().ok_or_else(|| anyhow!("REFUSED: no Ironwood bundle"))?;
    if b.actions().len() != SALE_ACTIONS {
        bail!("REFUSED: {} actions, a sale has exactly {SALE_ACTIONS}", b.actions().len());
    }
    // 5. the carrier spend is in the tx
    let ai = b.actions().iter().position(|a| a.nullifier().to_bytes() == t.carrier_nf).ok_or_else(|| anyhow!("REFUSED: carrier spend not in PCZT"))?;
    // 6. payment: outputs to the listing's pay address sum to the price
    let paid_outs: Vec<_> = decrypt_bundle_ivk(Some(b), t.pay_fvk).into_iter().filter(|f| f.note.recipient() == t.pay).collect();
    let paid = paid_outs.iter().try_fold(0u64, |acc, f| acc.checked_add(f.note.value().inner())).ok_or_else(|| anyhow!("REFUSED: payment sum overflows"))?;
    if paid != t.price {
        bail!("REFUSED: payment {paid} zats != listed price {}", t.price);
    }
    let summary = format!("listing open, expiry {exp} (tip {}), branch {branch:08x}, {} actions, carrier spend in action {ai}, payment {paid} zats", t.tip, b.actions().len());
    Ok(Checked { pczt: p, carrier_action: ai, payment_actions: paid_outs.iter().map(|f| f.action).collect(), summary })
}

/// Seller: sign the carrier spend of a checked PCZT.
pub fn seller_sign(c: Checked, sak: &SpendAuthorizingKey) -> Result<Pczt> {
    let mut s = Signer::new(c.pczt).map_err(|e| anyhow!("signer: {e:?}"))?;
    s.sign_ironwood(c.carrier_action, sak).map_err(|e| anyhow!("sign carrier: {e:?}"))?;
    Ok(s.finish())
}

/// Extracts the final tx (verifies the proof and every signature) and checks the carrier spend
/// is still in `carrier_action`. Returns the tx, its raw bytes and the successor commitment.
pub fn extract(p: Pczt, carrier_action: usize, carrier_nf: &[u8; 32]) -> Result<(Transaction, Vec<u8>, [u8; 32])> {
    let tx = TransactionExtractor::new(p).extract().map_err(|e| anyhow!("extract: {e:?}"))?;
    let act = tx.ironwood_bundle().and_then(|b| b.actions().get(carrier_action)).ok_or_else(|| anyhow!("no Ironwood bundle"))?;
    if act.nullifier().to_bytes() != *carrier_nf {
        bail!("extracted tx moved the carrier spend");
    }
    let succ = act.cmx().to_bytes();
    let mut raw = vec![];
    tx.write(&mut raw)?;
    Ok((tx, raw, succ))
}

/// The whole sale on synthetic notes in a local two-leaf Merkle tree: buyer build, seller check,
/// sign and extract (proof and signatures verified). Needs no network, so the browser build can
/// run it. Returns the txid.
pub fn selftest<R: RngCore + CryptoRng>(rng: &mut R, log: &mut dyn FnMut(String)) -> Result<String> {
    use carrier_core::*;
    use incrementalmerkletree::{Hashable, Level};
    use orchard::{
        keys::Scope,
        note::{ExtractedNoteCommitment, RandomSeed, Rho},
        tree::MerkleHashOrchard,
        value::NoteValue,
        NoteVersion,
    };
    let (seller_sk, pay_sk, buyer_sk, vault_sk) = (spending_key(&[1; 32], 1), spending_key(&[1; 32], 2), spending_key(&[2; 32], 0), spending_key(&[2; 32], 1));
    let (seller, pay, buyer, bvault) = (FullViewingKey::from(&seller_sk), FullViewingKey::from(&pay_sk), FullViewingKey::from(&buyer_sk), FullViewingKey::from(&vault_sk));
    let mut note = |to: &FullViewingKey, value: u64, rho_lead: u8| -> Result<Note> {
        let mut rb = [0u8; 32];
        rb[0] = rho_lead;
        let rho = Option::from(Rho::from_bytes(&rb)).ok_or_else(|| anyhow!("rho"))?;
        loop {
            let mut r = [0u8; 32];
            rng.fill_bytes(&mut r);
            if let Some(rs) = Option::from(RandomSeed::from_bytes(r, &rho)) {
                if let Some(n) = Option::from(Note::from_parts(to.address_at(0u32, Scope::External), NoteValue::from_raw(value), rho, rs, NoteVersion::V3)) {
                    return Ok(n);
                }
            }
        }
    };
    let coll = [7u8; 32];
    let carrier = note(&seller, 0, 1)?;
    let fund = note(&buyer, 100_000, 2)?;
    let (ccmx, fcmx): (ExtractedNoteCommitment, ExtractedNoteCommitment) = (carrier.commitment().into(), fund.commitment().into());
    let path = |pos: u32, sibling| {
        let mut auth = [MerkleHashOrchard::empty_leaf(); 32];
        auth[0] = MerkleHashOrchard::from_cmx(&sibling);
        for (i, a) in auth.iter_mut().enumerate().skip(1) {
            *a = MerkleHashOrchard::empty_root(Level::from(i as u8));
        }
        MerklePath::from_parts(pos, auth)
    };
    let (cpath, fpath) = (path(0, fcmx), path(1, ccmx));
    let anchor = cpath.root(ccmx);
    if fpath.root(fcmx) != anchor {
        bail!("selftest: anchors differ");
    }
    let (target, price, lid) = (BlockHeight::from_u32(4_500_000), 10_000u64, [9u8; 32]);
    let pay_addr = pay.address_at(5u32, Scope::External);
    let mut no_memo = [0u8; 512];
    no_memo[0] = 0xF6;
    let outs = [
        Out { to: bvault.address_at(0u32, Scope::External), value: 0, memo: encode_memo(K_HOP, &coll, 1, 1, &[]), ovk: Some(bvault.to_ovk(Scope::External)) },
        Out { to: pay_addr, value: price, memo: no_memo, ovk: None },
        Out { to: buyer.address_at(0u32, Scope::External), value: 100_000 - price - 15_000, memo: encode_memo(K_ACCOUNT, &coll, 1, 1, &encode_account_record(1)), ovk: Some(buyer.to_ovk(Scope::External)) },
    ];
    let secrets = vec![("funding FVK", buyer.to_bytes().to_vec())];
    let pk = proving_key();
    let fund_sak = SpendAuthorizingKey::from(&buyer_sk);
    let sale = BuyerSale { vault: &seller, carrier, carrier_path: cpath, fund_fvk: &buyer, fund_sak: &fund_sak, fund, fund_path: fpath, anchor, outs, target, expiry: target + 20, listing_id: lid, secrets };
    let built = buyer_build(sale, &pk, rng, log)?;
    let carrier_nf = carrier.nullifier(&seller).to_bytes();
    let terms = SellerTerms { listing_id: lid, listing_expiry: 4_500_100, price, pay: pay_addr, pay_fvk: &pay, carrier_nf, tip: u64::from(target) - 1 };
    let c = seller_check(&built.pczt, &terms)?;
    log(format!("  checks ok: {}", c.summary));
    let ai = c.carrier_action;
    let (tx, raw, _) = extract(seller_sign(c, &SpendAuthorizingKey::from(&seller_sk))?, ai, &carrier_nf)?;
    log(format!("  extracted tx {}: {} bytes", tx.txid(), raw.len()));
    Ok(tx.txid().to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn sale_selftest() {
        let txid = super::selftest(&mut rand_core::OsRng, &mut |l| println!("{l}")).unwrap();
        assert_eq!(txid.len(), 64);
    }
}
