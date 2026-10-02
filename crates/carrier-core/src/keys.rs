//! Key material for the marketplace (SPEC section 2.1 and 3.2).
//!
//! - `collectibles_seed`: the seed derived from a wallet's deterministic signature (step 4).
//! - `vault_sign` / `vault_verify`: a vault proves it holds a disclosed vault key by signing with
//!   its spend authorizing key; anyone verifies against the `ak` inside the disclosed FVK. This is
//!   the signature over the disclosure transcript `T` (SPEC 3.2). The byte encoding of `T` itself is
//!   still open (proposal X3), so these functions take the message bytes as given.
use anyhow::{anyhow, Result};
use group::ff::Field;
use orchard::keys::{FullViewingKey, SpendAuthorizingKey, SpendValidatingKey};
use orchard::primitives::redpallas::{Signature, SpendAuth};
use rand_core::{CryptoRng, RngCore};

/// Domain separator for vault signatures. A vault signature can never be a valid spend
/// authorization, because spends sign a transaction sighash with a randomized key.
pub const VAULT_SIG_DOMAIN: &[u8] = b"CarrierVaultSig1";

fn framed(msg: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(VAULT_SIG_DOMAIN.len() + 8 + msg.len());
    m.extend_from_slice(VAULT_SIG_DOMAIN);
    m.extend_from_slice(&(msg.len() as u64).to_le_bytes());
    m.extend_from_slice(msg);
    m
}

/// Signs `msg` with the vault's spend authorizing key (unrandomized).
pub fn vault_sign<R: RngCore + CryptoRng>(sak: &SpendAuthorizingKey, msg: &[u8], rng: R) -> [u8; 64] {
    let sig: Signature<SpendAuth> = sak.randomize(&pasta_curves::pallas::Scalar::ZERO).sign(rng, &framed(msg));
    <[u8; 64]>::from(&sig)
}

/// Verifies a `vault_sign` signature against the spend validating key inside `fvk`.
pub fn vault_verify(fvk: &FullViewingKey, msg: &[u8], sig: &[u8; 64]) -> Result<()> {
    let vk = SpendValidatingKey::from(fvk.clone()).randomize(&pasta_curves::pallas::Scalar::ZERO);
    vk.verify(&framed(msg), &Signature::<SpendAuth>::from(*sig)).map_err(|_| anyhow!("vault signature does not verify"))
}

/// SPEC 2.1 step 4: `seed = HKDF-SHA256(ikm = signature, salt = "carrier/v1", info = M)`.
/// `M` is the exact connect message the wallet signed (its bytes are to be frozen, proposal X5).
pub fn collectibles_seed(wallet_signature: &[u8], m: &[u8]) -> Result<[u8; 32]> {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"carrier/v1"), wallet_signature);
    let mut out = [0u8; 32];
    hk.expand(m, &mut out).map_err(|_| anyhow!("hkdf expand"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchard::keys::SpendingKey;
    use rand_core::OsRng;

    fn key(seed: u8, account: u32) -> SpendingKey {
        SpendingKey::from_zip32_seed(&[seed; 32], 1, zip32::AccountId::try_from(account).unwrap()).unwrap()
    }

    #[test]
    fn vault_signature_round_trip_and_binding() {
        let (sk, other) = (key(1, 16), key(1, 17));
        let msg = b"listing transcript";
        let sig = vault_sign(&SpendAuthorizingKey::from(&sk), msg, OsRng);
        assert!(vault_verify(&FullViewingKey::from(&sk), msg, &sig).is_ok());
        assert!(vault_verify(&FullViewingKey::from(&sk), b"other transcript", &sig).is_err());
        assert!(vault_verify(&FullViewingKey::from(&other), msg, &sig).is_err());
    }

    #[test]
    fn collectibles_seed_is_deterministic_and_message_bound() {
        let a = collectibles_seed(b"sig", b"Carrier collectibles key v1").unwrap();
        assert_eq!(a, collectibles_seed(b"sig", b"Carrier collectibles key v1").unwrap());
        assert_ne!(a, collectibles_seed(b"sig", b"Carrier collectibles key v2").unwrap());
        assert_ne!(a, collectibles_seed(b"sig2", b"Carrier collectibles key v1").unwrap());
    }
}
