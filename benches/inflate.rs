use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

const SIZES: &[(usize, &str)] = &[
    (4 * 1024, "4k"),
    (16 * 1024, "16k"),
    (64 * 1024, "64k"),
    (256 * 1024, "256k"),
    (1024 * 1024, "1m"),
    (16 * 1024 * 1024, "16m"),
];

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

fn make_textish(len: usize) -> Vec<u8> {
    // Deterministic ASCII-ish payload: a mixture of words, punctuation, and newlines.
    // This is meant to be "somewhat compressible" but not as extreme as make_repetitive.
    const CORPUS: &[u8] = b"\
+Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n\
+Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\
+Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.\n\
+Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.\n\
+Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.\n\
+";

    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let take = (len - out.len()).min(CORPUS.len());
        out.extend_from_slice(&CORPUS[..take]);
    }
    out
}

fn gzip_bytes(input: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(input).unwrap();
    enc.finish().unwrap()
}

fn bench_inflate_gzip(c: &mut Criterion) {
    let mut group = c.benchmark_group("inflate_gzip");
    group.warm_up_time(std::time::Duration::from_secs(3));
    group.measurement_time(std::time::Duration::from_secs(8));
    group.sample_size(50);

    // Keep the legacy microbench around as a sanity check.
    let cases: Vec<(String, Vec<u8>)> =
        std::iter::once(("small_text".to_string(), make_small_text()))
            .chain(SIZES.iter().flat_map(|(len, label)| {
                let mut v: Vec<(String, Vec<u8>)> = Vec::with_capacity(3);
                v.push((format!("repetitive_{label}"), make_repetitive(*len)));
                v.push((format!("random_{label}"), make_random(*len)));
                if *len >= 64 * 1024 {
                    v.push((format!("textish_{label}"), make_textish(*len)));
                }
                v
            }))
            .collect();

    for (name, plain) in cases {
        let gz = gzip_bytes(&plain);
        group.throughput(Throughput::Bytes(plain.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(&name), &gz, |b, gz| {
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
