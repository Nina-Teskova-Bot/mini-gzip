use std::io::Write;

use flate2::{Compression, write::GzEncoder};
use proptest::prelude::*;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

#[test]
fn deterministic_roundtrip_empty() {
    let input = b"";
    let gz = gzip(input);
    let out = mini_gzip::inflate_gzip(&gz).unwrap();
    assert_eq!(out, input);
}

#[test]
fn deterministic_roundtrip_hello() {
    let input = b"hello world\n";
    let gz = gzip(input);
    let out = mini_gzip::inflate_gzip(&gz).unwrap();
    assert_eq!(out, input);
}

#[test]
fn deterministic_roundtrip_repeated() {
    let input = vec![b'a'; 256 * 1024];
    let gz = gzip(&input);
    let out = mini_gzip::inflate_gzip(&gz).unwrap();
    assert_eq!(out, input);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, max_shrink_iters: 0, .. ProptestConfig::default() })]

    #[test]
    fn proptest_roundtrip_random_bytes(data in prop::collection::vec(any::<u8>(), 0..(64*1024))) {
        let gz = gzip(&data);
        let out = mini_gzip::inflate_gzip(&gz).unwrap();
        prop_assert_eq!(out, data);
    }
}

#[test]
fn optional_cross_validate_with_system_gzip() -> Result<()> {
    // Only run when requested and when `gzip` is available.
    if std::env::var("MINI_GZIP_CROSSCHECK").ok().as_deref() != Some("1") {
        return Ok(());
    }

    if which::which("gzip").is_err() {
        eprintln!("skipping: gzip not found in PATH");
        return Ok(());
    }

    let input = b"Cross-check data: \0\x01\x02\n";
    let gz = gzip(input);

    let tmp = tempfile::tempdir()?;
    let gz_path = tmp.path().join("input.gz");
    std::fs::write(&gz_path, &gz)?;

    let sys_out = std::process::Command::new("gzip")
        .arg("-dc")
        .arg(&gz_path)
        .output()?;
    assert!(sys_out.status.success());

    let ours = mini_gzip::inflate_gzip(&gz)?;
    assert_eq!(ours, sys_out.stdout);
    Ok(())
}
