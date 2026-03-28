use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Instant,
};

const MAXBITS: usize = 15;
const MAXLCODES: usize = 286;
const MAXDCODES: usize = 30;
const FIXLCODES: usize = 288;
const MAXDIST: usize = 32768;
const LENS: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEXT: [u16; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTS: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u16; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
const DEFAULT_CORPUS_LIMIT: usize = 64;
const DEFAULT_CORPUS_MAX_BYTES: u64 = 256 * 1024;
const DEFAULT_PERF_ITERATIONS: usize = 7;
const CORPUS_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "css", "go", "h", "hpp", "html", "java", "js", "json", "lock", "md", "py",
    "rs", "sh", "toml", "ts", "txt", "yaml", "yml",
];

struct State<'a> {
    bit_count: i32,
    bit_buffer: i32,
    input: &'a [u8],
    pos: usize,
    next: usize,
    window: [u8; MAXDIST],
}

struct Huffman<'a> {
    count: &'a [i16],
    symbol: &'a [i16],
}

impl<'a> State<'a> {
    fn nextbyte(&mut self) -> u8 {
        if self.pos < self.input.len() {
            self.pos += 1;
            return self.input[self.pos - 1];
        }
        panic!("unexpected end of input");
    }

    fn bits(&mut self, need: i32) -> i32 {
        let mut val = self.bit_buffer;
        while self.bit_count < need {
            val |= (self.nextbyte() as i32) << self.bit_count;
            self.bit_count += 8;
        }
        self.bit_buffer = val >> need;
        self.bit_count -= need;
        val & ((1i32 << need) - 1)
    }

    fn output(&mut self, out: &mut Vec<u8>, c: u8) {
        out.push(c);
        self.window[self.next] = c;
        self.next = (self.next + 1) & (MAXDIST - 1);
    }
}

fn decode(s: &mut State, h: &Huffman) -> i32 {
    let (mut bitbuf, mut left) = (s.bit_buffer, s.bit_count);
    let (mut len, mut code, mut first, mut index) = (1, 0, 0, 0);
    let mut next_idx = 1usize;
    loop {
        while left > 0 {
            left -= 1;
            code |= bitbuf & 1;
            bitbuf >>= 1;
            let count = h.count[next_idx] as i32;
            next_idx += 1;
            if code < first + count {
                s.bit_buffer = bitbuf;
                s.bit_count = (s.bit_count - len) & 7;
                return h.symbol[(index + code - first) as usize] as i32;
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
            len += 1;
        }
        left = (MAXBITS as i32 + 1) - len;
        if left == 0 {
            break;
        }
        bitbuf = s.nextbyte() as i32;
        if left > 8 {
            left = 8;
        }
    }
    panic!("invalid huffman code");
}

fn construct(count: &mut [i16], symbol: &mut [i16], length: &[u16], n: usize) -> i32 {
    count[..=MAXBITS].fill(0);
    for i in 0..n {
        count[length[i] as usize] += 1;
    }
    if count[0] as usize == n {
        return 0;
    }
    let mut left = 1i32;
    for len in 1..=MAXBITS {
        left = (left << 1) - count[len] as i32;
        if left < 0 {
            return left;
        }
    }
    let mut offs = [0i16; MAXBITS + 1];
    for len in 1..MAXBITS {
        offs[len + 1] = offs[len] + count[len];
    }
    for sym in 0..n {
        if length[sym] != 0 {
            symbol[offs[length[sym] as usize] as usize] = sym as i16;
            offs[length[sym] as usize] += 1;
        }
    }
    left
}

fn codes(s: &mut State, out: &mut Vec<u8>, lc: &Huffman, dc: &Huffman) {
    loop {
        let sym = decode(s, lc);
        if sym < 256 {
            s.output(out, sym as u8);
        } else if sym > 256 {
            let idx = (sym - 257) as usize;
            let len = LENS[idx] as i32 + s.bits(LEXT[idx] as i32);
            let dsym = decode(s, dc) as usize;
            let dist = DISTS[dsym] as u32 + s.bits(DEXT[dsym] as i32) as u32;
            for _ in 0..len {
                let c = s.window[s.next.wrapping_sub(dist as usize) & (MAXDIST - 1)];
                s.output(out, c);
            }
        } else {
            return;
        }
    }
}

fn fixed(s: &mut State, out: &mut Vec<u8>) {
    static FIXED_TABLES: OnceLock<(
        [i16; MAXBITS + 1],
        [i16; FIXLCODES],
        [i16; MAXBITS + 1],
        [i16; MAXDCODES],
    )> = OnceLock::new();
    let (lc_cnt, lc_sym, dc_cnt, dc_sym) = FIXED_TABLES.get_or_init(|| {
        let (mut lc_cnt, mut lc_sym) = ([0i16; MAXBITS + 1], [0i16; FIXLCODES]);
        let (mut dc_cnt, mut dc_sym) = ([0i16; MAXBITS + 1], [0i16; MAXDCODES]);
        let mut lengths = [8u16; FIXLCODES];
        lengths[144..256].fill(9);
        lengths[256..280].fill(7);
        construct(&mut lc_cnt, &mut lc_sym, &lengths, FIXLCODES);
        lengths[..MAXDCODES].fill(5);
        construct(&mut dc_cnt, &mut dc_sym, &lengths, MAXDCODES);
        (lc_cnt, lc_sym, dc_cnt, dc_sym)
    });
    let lc = Huffman {
        count: lc_cnt,
        symbol: lc_sym,
    };
    let dc = Huffman {
        count: dc_cnt,
        symbol: dc_sym,
    };
    codes(s, out, &lc, &dc)
}

fn dynamic(s: &mut State, out: &mut Vec<u8>) {
    let (nlen, ndist, ncode) = (
        s.bits(5) as usize + 257,
        s.bits(5) as usize + 1,
        s.bits(4) as usize + 4,
    );
    let mut lengths = [0u16; MAXLCODES + MAXDCODES];
    for i in 0..ncode {
        lengths[ORDER[i]] = s.bits(3) as u16;
    }
    let (mut lc_cnt, mut lc_sym) = ([0i16; MAXBITS + 1], [0i16; MAXLCODES]);
    construct(&mut lc_cnt, &mut lc_sym, &lengths, 19);
    let mut lc = Huffman {
        count: &lc_cnt,
        symbol: &lc_sym,
    };
    let mut idx = 0usize;
    while idx < nlen + ndist {
        let sym = decode(s, &lc);
        if sym < 16 {
            lengths[idx] = sym as u16;
            idx += 1;
        } else {
            let (len, rep) = match sym {
                16 => (lengths[idx - 1], 3 + s.bits(2) as usize),
                17 => (0, 3 + s.bits(3) as usize),
                _ => (0, 11 + s.bits(7) as usize),
            };
            for _ in 0..rep {
                lengths[idx] = len;
                idx += 1;
            }
        }
    }
    construct(&mut lc_cnt, &mut lc_sym, &lengths, nlen);
    lc = Huffman {
        count: &lc_cnt,
        symbol: &lc_sym,
    };
    let (mut dc_cnt, mut dc_sym) = ([0i16; MAXBITS + 1], [0i16; MAXDCODES]);
    construct(&mut dc_cnt, &mut dc_sym, &lengths[nlen..], ndist);
    let dc = Huffman {
        count: &dc_cnt,
        symbol: &dc_sym,
    };
    codes(s, out, &lc, &dc)
}

fn stored(s: &mut State, out: &mut Vec<u8>) {
    s.bits(s.bit_count);
    let len = s.bits(16) as u32;
    s.bits(16);
    for _ in 0..len {
        let c = s.nextbyte();
        s.output(out, c);
    }
}

pub fn inflate(input: &[u8], output_size: usize) -> Vec<u8> {
    let mut s = State {
        bit_count: 0,
        bit_buffer: 0,
        input,
        pos: 0,
        next: 0,
        window: [0; MAXDIST],
    };
    let mut out = Vec::with_capacity(output_size);
    loop {
        let last = s.bits(1);
        match s.bits(2) {
            0 => stored(&mut s, &mut out),
            1 => fixed(&mut s, &mut out),
            2 => dynamic(&mut s, &mut out),
            _ => panic!("invalid block type"),
        }
        if last != 0 {
            break;
        }
    }
    out
}

fn read_c_string(input: &[u8], offset: &mut usize) {
    let rest = input
        .get(*offset..)
        .unwrap_or_else(|| panic!("truncated gzip header"));
    let len = rest
        .iter()
        .position(|&b| b == 0)
        .unwrap_or_else(|| panic!("unterminated gzip header string"));
    *offset += len + 1;
}

fn gzip_payload(input: &[u8]) -> &[u8] {
    assert!(input.len() >= 18, "truncated gzip stream");
    assert!(input[0] == 0x1F && input[1] == 0x8B, "not a gzip file");
    assert!(input[2] == 8, "unsupported compression method");

    let flags = input[3];
    assert!(flags & 0xE0 == 0, "unsupported gzip format");

    let mut offset = 10usize;
    if flags & 0x04 != 0 {
        let xlen = u16::from_le_bytes(
            input[offset..offset + 2]
                .try_into()
                .unwrap_or_else(|_| panic!("truncated gzip extra field")),
        ) as usize;
        offset += 2 + xlen;
    }
    if flags & 0x08 != 0 {
        read_c_string(input, &mut offset);
    }
    if flags & 0x10 != 0 {
        read_c_string(input, &mut offset);
    }
    if flags & 0x02 != 0 {
        offset += 2;
    }

    assert!(
        offset <= input.len().saturating_sub(8),
        "truncated gzip stream"
    );
    &input[offset..input.len() - 8]
}

fn gzip_isize(input: &[u8]) -> usize {
    u32::from_le_bytes(
        input[input.len() - 4..]
            .try_into()
            .unwrap_or_else(|_| panic!("truncated gzip trailer")),
    ) as usize
}

pub fn gunzip(input: &[u8]) -> Vec<u8> {
    let payload = gzip_payload(input);
    inflate(payload, gzip_isize(input))
}

#[derive(Clone)]
struct CorpusEntry {
    path: PathBuf,
    input: Vec<u8>,
    compressed: Vec<u8>,
}

impl CorpusEntry {
    fn bytes(&self) -> usize {
        self.input.len()
    }
}

struct PerfRun {
    elapsed_ms: f64,
    bytes: usize,
}

struct PerfSummary {
    iterations: usize,
    total_ms: f64,
    avg_ms: f64,
    min_ms: f64,
    median_ms: f64,
    max_ms: f64,
    bytes: usize,
}

fn print_usage(program: &str) {
    eprintln!(
        "usage:
  {program} [FILE]
  {program} perf --root PATH [--iterations N] [--count N] [--max-bytes N]"
    );
}

fn file_extension_allowed(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| CORPUS_EXTENSIONS.contains(&ext))
}

fn collect_corpus_files(root: &Path, files: &mut Vec<PathBuf>, limit: usize, max_bytes: u64) {
    if files.len() >= limit {
        return;
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", root.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if files.len() >= limit {
            break;
        }
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        if path.is_dir() {
            if matches!(name, ".git" | "target" | "node_modules") {
                continue;
            }
            collect_corpus_files(&path, files, limit, max_bytes);
        } else if path.is_file()
            && file_extension_allowed(&path)
            && fs::metadata(&path)
                .map(|meta| meta.len() <= max_bytes)
                .unwrap_or(false)
        {
            files.push(path);
        }
    }
}

fn corpus_candidates(root: &Path, limit: usize, max_bytes: u64) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_corpus_files(root, &mut files, limit, max_bytes);
    files
}

fn gzip_via_file(path: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("gzip")
        .args(args)
        .arg(path)
        .output()
        .expect("run gzip");
    assert!(output.status.success(), "gzip failed: {:?}", output.status);
    output.stdout
}

fn prepare_corpus(root: &Path, limit: usize, max_bytes: u64) -> Vec<CorpusEntry> {
    let paths = corpus_candidates(root, limit, max_bytes);
    assert!(
        !paths.is_empty(),
        "no matching files found under {}",
        root.display()
    );
    paths
        .into_iter()
        .map(|path| {
            let input = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let compressed = gzip_via_file(&path, &["-n", "-c"]);
            let output = gunzip(&compressed);
            assert_eq!(output, input, "failed for {}", path.display());
            CorpusEntry {
                path,
                input,
                compressed,
            }
        })
        .collect()
}

fn summarize_runs(runs: &[PerfRun], iterations: usize) -> PerfSummary {
    let mut elapsed: Vec<f64> = runs.iter().map(|run| run.elapsed_ms).collect();
    elapsed.sort_by(|a, b| a.total_cmp(b));
    let total_ms = elapsed.iter().sum::<f64>();
    let median_ms = if elapsed.len() % 2 == 0 {
        let hi = elapsed.len() / 2;
        (elapsed[hi - 1] + elapsed[hi]) / 2.0
    } else {
        elapsed[elapsed.len() / 2]
    };
    PerfSummary {
        iterations,
        total_ms,
        avg_ms: total_ms / elapsed.len() as f64,
        min_ms: elapsed[0],
        median_ms,
        max_ms: elapsed[elapsed.len() - 1],
        bytes: runs[0].bytes,
    }
}

fn run_perf_harness(root: &Path, iterations: usize, limit: usize, max_bytes: u64) {
    assert!(iterations > 0, "iterations must be greater than zero");
    let corpus = prepare_corpus(root, limit, max_bytes);
    let bytes: usize = corpus.iter().map(CorpusEntry::bytes).sum();
    let compressed_bytes: usize = corpus.iter().map(|entry| entry.compressed.len()).sum();

    eprintln!(
        "perf corpus root={} files={} bytes={} compressed_bytes={} iterations={} max_bytes={} extensions={}",
        root.display(),
        corpus.len(),
        bytes,
        compressed_bytes,
        iterations,
        max_bytes,
        CORPUS_EXTENSIONS.join(",")
    );
    for entry in &corpus {
        eprintln!("selected {}", entry.path.display());
    }

    let mut runs = Vec::with_capacity(iterations);
    for iteration in 0..iterations {
        let start = Instant::now();
        let mut iteration_bytes = 0usize;
        for entry in &corpus {
            let output = gunzip(&entry.compressed);
            assert_eq!(output, entry.input, "failed for {}", entry.path.display());
            iteration_bytes += output.len();
        }
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "iteration={} elapsed_ms={:.3} bytes={}",
            iteration + 1,
            elapsed_ms,
            iteration_bytes
        );
        runs.push(PerfRun {
            elapsed_ms,
            bytes: iteration_bytes,
        });
    }

    let summary = summarize_runs(&runs, iterations);
    println!(
        "summary files={} bytes={} compressed_bytes={} iterations={} total_ms={:.3} avg_ms={:.3} min_ms={:.3} median_ms={:.3} max_ms={:.3}",
        corpus.len(),
        summary.bytes,
        compressed_bytes,
        summary.iterations,
        summary.total_ms,
        summary.avg_ms,
        summary.min_ms,
        summary.median_ms,
        summary.max_ms
    );
}

fn main() {
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "mini-gzip".to_string());
    let Some(cmd) = args.next() else {
        let mut b = Vec::new();
        io::stdin().read_to_end(&mut b).expect("read stdin");
        io::stdout().write_all(&gunzip(&b)).unwrap();
        return;
    };

    if matches!(cmd.as_str(), "-h" | "--help") {
        print_usage(&program);
        return;
    }

    if cmd == "perf" {
        let mut root = None::<PathBuf>;
        let mut iterations = DEFAULT_PERF_ITERATIONS;
        let mut count = DEFAULT_CORPUS_LIMIT;
        let mut max_bytes = DEFAULT_CORPUS_MAX_BYTES;
        let rest: Vec<String> = args.collect();
        let mut idx = 0usize;
        while idx < rest.len() {
            let flag = &rest[idx];
            let next = rest
                .get(idx + 1)
                .unwrap_or_else(|| panic!("missing value for {flag}"));
            match flag.as_str() {
                "--root" => root = Some(PathBuf::from(next)),
                "--iterations" => iterations = next.parse().expect("iterations must be an integer"),
                "--count" => count = next.parse().expect("count must be an integer"),
                "--max-bytes" => max_bytes = next.parse().expect("max-bytes must be an integer"),
                _ => panic!("unknown perf arg: {flag}"),
            }
            idx += 2;
        }
        let root = root.expect("perf mode requires --root PATH");
        run_perf_harness(&root, iterations, count, max_bytes);
        return;
    }

    let buf = fs::read(&cmd).expect("read file");
    io::stdout().write_all(&gunzip(&buf)).unwrap();
}

#[cfg(test)]
mod tests {
    use super::{corpus_candidates, gunzip, gzip_via_file};
    use std::{
        collections::BTreeMap,
        env,
        ffi::OsStr,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static UNIQUE_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_path(name: &str) -> PathBuf {
        let suffix = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        env::temp_dir().join(format!("mini-gzip-{name}-{nanos}-{suffix}"))
    }

    fn gzip_from_bytes(name: &str, input: &[u8], args: &[&str]) -> Vec<u8> {
        let path = unique_temp_path(name);
        fs::write(&path, input).expect("write temp input");
        let compressed = gzip_via_file(&path, args);
        fs::remove_file(&path).expect("remove temp input");
        compressed
    }

    #[test]
    fn decompresses_gzip_from_stdin_at_multiple_levels() {
        let mut input = Vec::new();
        for i in 0..200_000usize {
            input.extend_from_slice(format!("{i:06}:mini-gzip\n").as_bytes());
            input.push((i % 251) as u8);
        }

        for args in [["-n", "-1", "-c"], ["-n", "-9", "-c"]] {
            let compressed = gzip_from_bytes("levels", &input, &args);
            assert_eq!(gunzip(&compressed), input);
        }
    }

    #[test]
    fn decompresses_binary_payload_with_full_byte_range() {
        let input: Vec<u8> = (0..=255).cycle().take(65_536).collect();
        let compressed = gzip_from_bytes("binary", &input, &["-n", "-c"]);
        assert_eq!(gunzip(&compressed), input);
    }

    #[test]
    fn decompresses_gzip_with_filename_header() {
        let dir = unique_temp_path("fname");
        fs::create_dir(&dir).expect("create temp dir");
        let path = dir.join("sample.txt");
        let input = b"header parsing should honor original filenames\n".repeat(1024);
        fs::write(&path, &input).expect("write temp file");

        let compressed = gzip_via_file(&path, &["-c"]);
        assert_eq!(gunzip(&compressed), input);

        fs::remove_file(&path).expect("remove temp file");
        fs::remove_dir(&dir).expect("remove temp dir");
    }

    #[test]
    #[ignore = "requires MINI_GZIP_CORPUS_ROOT to be set to a workspace root"]
    fn corpus_round_trip_from_workspace_root() {
        let root = PathBuf::from(
            env::var("MINI_GZIP_CORPUS_ROOT")
                .expect("set MINI_GZIP_CORPUS_ROOT to a workspace root"),
        );
        let files = corpus_candidates(&root, 200, 1_000_000);
        assert!(!files.is_empty(), "no files found under {}", root.display());

        let mut ext_counts = BTreeMap::<String, usize>::new();
        for path in &files {
            let input = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let compressed = gzip_via_file(path, &["-n", "-c"]);
            assert_eq!(gunzip(&compressed), input, "failed for {}", path.display());

            let ext = path
                .extension()
                .and_then(OsStr::to_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("<no-ext>")
                .to_string();
            *ext_counts.entry(ext).or_default() += 1;
        }

        eprintln!("validated {} files under {}", files.len(), root.display());
        eprintln!("extension mix: {:?}", ext_counts);
    }
}
