fn main() {
    let start = std::time::Instant::now();
    let now = move || start.elapsed().as_secs_f64() * 1000.0;
    for n in [2usize, 4] {
        let t = provebench::run(n, &now);
        println!("native actions={} pk_ms={:.0} prove_ms={:.0}", t.actions, t.pk_ms, t.prove_ms);
    }
}
