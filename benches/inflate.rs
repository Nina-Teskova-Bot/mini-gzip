use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn make_small_text() -> Vec<u8> {
    b"hello world\nthis is a small text fixture\n".to_vec()
}

fn make_random(len: usize) -> Vec<u8> {
    // Deterministic pseudo-random to keep benchmarks reproducible.
    let mut x: u64 = 0x1234_5678_9abc_def0;
    let mut out = vec![0u8; len];
    for b in &mut out {
        // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        x = x.wrapping_mul(0x2545_f491_4f6c_dd1d);
        *b = (x >> 56) as u8;
    }
    out
}

fn make_repetitive(len: usize) -> Vec<u8> {
    // Highly compressible.
    let pat = b"abcabcabcabcabcabcabcabcabcabc";
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let take = (len - out.len()).min(pat.len());
        out.extend_from_slice(&pat[..take]);
    }
    out
}

fn gzip_bytes(input: &[u8]) -> Vec<u8> {
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;

    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(input).unwrap();
    enc.finish().unwrap()
}

fn bench_inflate_gzip(c: &mut Criterion) {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("small_text", make_small_text()),
        ("random_64k", make_random(64 * 1024)),
        ("repetitive_256k", make_repetitive(256 * 1024)),
        ("random_1m", make_random(1024 * 1024)),
    ];

    let mut group = c.benchmark_group("inflate_gzip");
    group.warm_up_time(std::time::Duration::from_secs(3));
    group.measurement_time(std::time::Duration::from_secs(8));
    group.sample_size(50);

    for (name, plain) in cases {
        let gz = gzip_bytes(&plain);
        group.throughput(Throughput::Bytes(plain.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &gz, |b, gz| {
            b.iter(|| {
                let out = mini_gzip::inflate_gzip(gz).unwrap();
                criterion::black_box(out)
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_inflate_gzip);
criterion_main!(benches);
