#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Write;

use flate2::{write::GzEncoder, Compression};

fuzz_target!(|data: &[u8]| {
    // Generate gzip using a known-good implementation, then ensure we can roundtrip.
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(data).unwrap();
    let gz = enc.finish().unwrap();

    let out = mini_gzip::inflate_gzip(&gz).unwrap();
    assert_eq!(out, data);
});
