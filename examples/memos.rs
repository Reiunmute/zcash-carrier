// Read-only: parse a raw tx file (as `sale-accept` writes with DRY=1), decrypt it with FVK (env)
// and print each carrier memo and the records it carries (marketplace SPEC 4.2, 4.3).
// Usage: FVK=<hex> cargo run --release --example memos -- <raw_tx_file>
use carrier::*;
fn main() -> anyhow::Result<()> {
    let fvk = fvk_from_hex(&std::env::var("FVK")?)?;
    let raw = std::fs::read(std::env::args().nth(1).unwrap())?;
    let tx = zcash_primitives::transaction::Transaction::read(&raw[..], zcash_protocol::consensus::BranchId::Nu6_3)?;
    for f in decrypt_ivk(&tx, &fvk) {
        match decode_memo(&f.memo) {
            Some(m) => println!("action {} value {} kind {} item {} hop {} payload {}B account_record {:?} vault_record_ok {}", f.action, f.note.value().inner(), m.kind, m.num, m.hop, m.art.len(), decode_account_record(&m.art), VaultRecord::decode(&m.art).is_ok()),
            None => println!("action {} value {} non-carrier memo", f.action, f.note.value().inner()),
        }
    }
    Ok(())
}
