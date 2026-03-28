use std::{
    collections::hash_map::DefaultHasher,
    env,
    ffi::OsStr,
    fs,
    hash::{Hash, Hasher},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{self, Command},
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const MAXBITS: usize = 15;
const MAXLCODES: usize = 286;
const MAXDCODES: usize = 30;
const FIXLCODES: usize = 288;
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
const DEFAULT_PERF_REPEAT: usize = 5;
const PERF_SNAPSHOT_VERSION: u64 = 1;
const PERF_FILE_SNAPSHOT_VERSION: u64 = 1;
const CORPUS_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "css", "go", "h", "hpp", "html", "java", "js", "json", "lock", "md", "py",
    "rs", "sh", "toml", "ts", "txt", "yaml", "yml",
];
const FAST_BITS: usize = 9;
const FAST_TABLE_SIZE: usize = 1 << FAST_BITS;

#[derive(Clone, Copy)]
struct FastEntry {
    symbol: i16,
    len: u8,
}

const EMPTY_FAST_ENTRY: FastEntry = FastEntry { symbol: 0, len: 0 };

struct State<'a> {
    bit_count: i32,
    bit_buffer: i32,
    input: &'a [u8],
    pos: usize,
}

struct Huffman<'a> {
    count: &'a [i16],
    symbol: &'a [i16],
    fast: &'a [FastEntry; FAST_TABLE_SIZE],
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
}

fn decode(s: &mut State, h: &Huffman) -> i32 {
    if s.bit_count < FAST_BITS as i32 {
        while s.bit_count < FAST_BITS as i32 && s.pos < s.input.len() {
            s.bit_buffer |= (s.nextbyte() as i32) << s.bit_count;
            s.bit_count += 8;
        }
    }
    if s.bit_count > 0 {
        let entry = h.fast[(s.bit_buffer as usize) & (FAST_TABLE_SIZE - 1)];
        if entry.len != 0 && i32::from(entry.len) <= s.bit_count {
            s.bit_buffer >>= entry.len;
            s.bit_count -= i32::from(entry.len);
            return entry.symbol as i32;
        }
    }

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

fn build_fast_table(lengths: &[u16], n: usize, fast: &mut [FastEntry; FAST_TABLE_SIZE]) {
    fast.fill(EMPTY_FAST_ENTRY);

    let mut count = [0u32; MAXBITS + 1];
    for &len in lengths.iter().take(n) {
        count[len as usize] += 1;
    }

    let mut next_code = [0u32; MAXBITS + 1];
    let mut code = 0u32;
    for bits in 1..=MAXBITS {
        code = (code + count[bits - 1]) << 1;
        next_code[bits] = code;
    }

    for (symbol, &len_u16) in lengths.iter().take(n).enumerate() {
        let len = len_u16 as usize;
        if len == 0 {
            continue;
        }
        let code = next_code[len];
        next_code[len] += 1;
        if len <= FAST_BITS {
            let reversed = (code.reverse_bits() >> (u32::BITS as usize - len)) as usize;
            let entry = FastEntry {
                symbol: symbol as i16,
                len: len as u8,
            };
            let step = 1usize << len;
            let mut idx = reversed;
            while idx < FAST_TABLE_SIZE {
                fast[idx] = entry;
                idx += step;
            }
        }
    }
}

fn codes(s: &mut State, out: &mut Vec<u8>, lc: &Huffman, dc: &Huffman) {
    loop {
        let sym = decode(s, lc);
        if sym < 256 {
            out.push(sym as u8);
        } else if sym > 256 {
            let idx = (sym - 257) as usize;
            let len = LENS[idx] as usize + s.bits(LEXT[idx] as i32) as usize;
            let dsym = decode(s, dc) as usize;
            let dist = DISTS[dsym] as usize + s.bits(DEXT[dsym] as i32) as usize;
            assert!(dist <= out.len(), "invalid backreference distance");
            let start = out.len() - dist;
            if dist >= len {
                out.extend_from_within(start..start + len);
            } else {
                let target_len = out.len() + len;
                out.reserve(len);
                while out.len() < target_len {
                    let chunk = (out.len() - start).min(target_len - out.len());
                    out.extend_from_within(start..start + chunk);
                }
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
        [FastEntry; FAST_TABLE_SIZE],
        [i16; MAXBITS + 1],
        [i16; MAXDCODES],
        [FastEntry; FAST_TABLE_SIZE],
    )> = OnceLock::new();
    let (lc_cnt, lc_sym, lc_fast, dc_cnt, dc_sym, dc_fast) = FIXED_TABLES.get_or_init(|| {
        let (mut lc_cnt, mut lc_sym) = ([0i16; MAXBITS + 1], [0i16; FIXLCODES]);
        let (mut dc_cnt, mut dc_sym) = ([0i16; MAXBITS + 1], [0i16; MAXDCODES]);
        let (mut lc_fast, mut dc_fast) = (
            [EMPTY_FAST_ENTRY; FAST_TABLE_SIZE],
            [EMPTY_FAST_ENTRY; FAST_TABLE_SIZE],
        );
        let mut lit_lengths = [8u16; FIXLCODES];
        lit_lengths[144..256].fill(9);
        lit_lengths[256..280].fill(7);
        construct(&mut lc_cnt, &mut lc_sym, &lit_lengths, FIXLCODES);
        build_fast_table(&lit_lengths, FIXLCODES, &mut lc_fast);

        let dist_lengths = [5u16; MAXDCODES];
        construct(&mut dc_cnt, &mut dc_sym, &dist_lengths, MAXDCODES);
        build_fast_table(&dist_lengths, MAXDCODES, &mut dc_fast);
        (lc_cnt, lc_sym, lc_fast, dc_cnt, dc_sym, dc_fast)
    });
    let lc = Huffman {
        count: lc_cnt,
        symbol: lc_sym,
        fast: lc_fast,
    };
    let dc = Huffman {
        count: dc_cnt,
        symbol: dc_sym,
        fast: dc_fast,
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
    let mut lc_fast = [EMPTY_FAST_ENTRY; FAST_TABLE_SIZE];
    construct(&mut lc_cnt, &mut lc_sym, &lengths, 19);
    build_fast_table(&lengths, 19, &mut lc_fast);
    let mut lc = Huffman {
        count: &lc_cnt,
        symbol: &lc_sym,
        fast: &lc_fast,
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
            let end = idx + rep;
            lengths[idx..end].fill(len);
            idx = end;
        }
    }
    construct(&mut lc_cnt, &mut lc_sym, &lengths, nlen);
    build_fast_table(&lengths, nlen, &mut lc_fast);
    lc = Huffman {
        count: &lc_cnt,
        symbol: &lc_sym,
        fast: &lc_fast,
    };
    let (mut dc_cnt, mut dc_sym) = ([0i16; MAXBITS + 1], [0i16; MAXDCODES]);
    let mut dc_fast = [EMPTY_FAST_ENTRY; FAST_TABLE_SIZE];
    construct(&mut dc_cnt, &mut dc_sym, &lengths[nlen..], ndist);
    build_fast_table(&lengths[nlen..], ndist, &mut dc_fast);
    let dc = Huffman {
        count: &dc_cnt,
        symbol: &dc_sym,
        fast: &dc_fast,
    };
    codes(s, out, &lc, &dc)
}

fn stored(s: &mut State, out: &mut Vec<u8>) {
    s.bits(s.bit_count);
    let len = s.bits(16) as u32;
    s.bits(16);
    for _ in 0..len {
        out.push(s.nextbyte());
    }
}

pub fn inflate(input: &[u8], output_size: usize) -> Vec<u8> {
    let mut s = State {
        bit_count: 0,
        bit_buffer: 0,
        input,
        pos: 0,
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

struct SingleFileFixture {
    source: PathBuf,
    snapshot_root: PathBuf,
    input: Vec<u8>,
    compressed: Vec<u8>,
}

fn print_usage(program: &str) {
    eprintln!(
        "usage:
  {program} [FILE]
  {program} perf --root PATH [--iterations N] [--count N] [--max-bytes N]
  {program} perf-file --path FILE [--iterations N] [--repeat N]"
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

fn perf_snapshot_dir(root: &Path, limit: usize, max_bytes: u64) -> PathBuf {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut hasher = DefaultHasher::new();
    PERF_SNAPSHOT_VERSION.hash(&mut hasher);
    canonical_root.hash(&mut hasher);
    limit.hash(&mut hasher);
    max_bytes.hash(&mut hasher);
    CORPUS_EXTENSIONS.hash(&mut hasher);
    PathBuf::from("target")
        .join("perf-corpus")
        .join(format!("{:016x}", hasher.finish()))
}

fn perf_snapshot_complete(snapshot_root: &Path) -> bool {
    snapshot_root.join(".complete").is_file()
}

fn remove_snapshot_root(snapshot_root: &Path) {
    if snapshot_root.is_dir() {
        fs::remove_dir_all(snapshot_root)
            .unwrap_or_else(|e| panic!("remove {}: {e}", snapshot_root.display()));
    } else if snapshot_root.exists() {
        fs::remove_file(snapshot_root)
            .unwrap_or_else(|e| panic!("remove {}: {e}", snapshot_root.display()));
    }
}

fn create_perf_snapshot(root: &Path, snapshot_root: &Path, limit: usize, max_bytes: u64) {
    let paths = corpus_candidates(root, limit, max_bytes);
    assert!(
        !paths.is_empty(),
        "no matching files found under {}",
        root.display()
    );

    let snapshot_parent = snapshot_root.parent().expect("snapshot dir parent");
    fs::create_dir_all(snapshot_parent)
        .unwrap_or_else(|e| panic!("create {}: {e}", snapshot_parent.display()));

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let snapshot_name = snapshot_root
        .file_name()
        .and_then(OsStr::to_str)
        .expect("snapshot dir name");
    let temp_root =
        snapshot_parent.join(format!(".{snapshot_name}.tmp-{}-{unique}", process::id()));
    if temp_root.exists() {
        remove_snapshot_root(&temp_root);
    }
    fs::create_dir_all(&temp_root)
        .unwrap_or_else(|e| panic!("create {}: {e}", temp_root.display()));

    let mut manifest = format!(
        "source_root={}\nlimit={limit}\nmax_bytes={max_bytes}\nextensions={}\n",
        root.display(),
        CORPUS_EXTENSIONS.join(",")
    );
    for path in &paths {
        let relative = path
            .strip_prefix(root)
            .unwrap_or_else(|_| panic!("{} not under {}", path.display(), root.display()));
        let snapshot_path = temp_root.join(relative);
        if let Some(parent) = snapshot_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("create {}: {e}", parent.display()));
        }
        fs::copy(path, &snapshot_path).unwrap_or_else(|e| {
            panic!(
                "copy {} -> {}: {e}",
                path.display(),
                snapshot_path.display()
            )
        });
        manifest.push_str(&format!("{}\n", relative.display()));
    }

    let manifest_path = temp_root.join(".manifest");
    fs::write(&manifest_path, manifest)
        .unwrap_or_else(|e| panic!("write {}: {e}", manifest_path.display()));
    let complete_path = temp_root.join(".complete");
    fs::write(&complete_path, b"")
        .unwrap_or_else(|e| panic!("write {}: {e}", complete_path.display()));

    match fs::rename(&temp_root, snapshot_root) {
        Ok(()) => {}
        Err(_) if perf_snapshot_complete(snapshot_root) => {
            let _ = fs::remove_dir_all(&temp_root);
        }
        Err(err) => panic!(
            "rename {} -> {}: {err}",
            temp_root.display(),
            snapshot_root.display()
        ),
    }
}

fn ensure_perf_snapshot(root: &Path, limit: usize, max_bytes: u64) -> PathBuf {
    let snapshot_root = perf_snapshot_dir(root, limit, max_bytes);
    if perf_snapshot_complete(&snapshot_root) {
        return snapshot_root;
    }
    if snapshot_root.exists() {
        remove_snapshot_root(&snapshot_root);
    }
    create_perf_snapshot(root, &snapshot_root, limit, max_bytes);
    snapshot_root
}

fn normalized_snapshot_source(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|e| panic!("current_dir: {e}"))
            .join(path)
    };
    absolute
        .canonicalize()
        .unwrap_or_else(|_| absolute.to_path_buf())
}

fn perf_file_snapshot_dir(source: &Path) -> PathBuf {
    let normalized = normalized_snapshot_source(source);
    let mut hasher = DefaultHasher::new();
    PERF_FILE_SNAPSHOT_VERSION.hash(&mut hasher);
    normalized.hash(&mut hasher);
    PathBuf::from("target")
        .join("perf-file")
        .join(format!("{:016x}", hasher.finish()))
}

fn create_perf_file_snapshot(source: &Path, snapshot_root: &Path) {
    assert!(source.is_file(), "{} is not a file", source.display());

    let snapshot_parent = snapshot_root.parent().expect("snapshot dir parent");
    fs::create_dir_all(snapshot_parent)
        .unwrap_or_else(|e| panic!("create {}: {e}", snapshot_parent.display()));

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let snapshot_name = snapshot_root
        .file_name()
        .and_then(OsStr::to_str)
        .expect("snapshot dir name");
    let temp_root =
        snapshot_parent.join(format!(".{snapshot_name}.tmp-{}-{unique}", process::id()));
    if temp_root.exists() {
        remove_snapshot_root(&temp_root);
    }
    fs::create_dir_all(&temp_root)
        .unwrap_or_else(|e| panic!("create {}: {e}", temp_root.display()));

    let snapshot_input = temp_root.join("input");
    fs::copy(source, &snapshot_input).unwrap_or_else(|e| {
        panic!(
            "copy {} -> {}: {e}",
            source.display(),
            snapshot_input.display()
        )
    });

    let snapshot_gzip = temp_root.join("input.gz");
    let compressed = gzip_via_file(&snapshot_input, &["-n", "-c"]);
    fs::write(&snapshot_gzip, compressed)
        .unwrap_or_else(|e| panic!("write {}: {e}", snapshot_gzip.display()));

    let manifest = format!(
        "source={}\nsource_size={}\nsource_name={}\n",
        normalized_snapshot_source(source).display(),
        fs::metadata(source)
            .unwrap_or_else(|e| panic!("metadata {}: {e}", source.display()))
            .len(),
        source
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("<unknown>"),
    );
    let manifest_path = temp_root.join(".manifest");
    fs::write(&manifest_path, manifest)
        .unwrap_or_else(|e| panic!("write {}: {e}", manifest_path.display()));
    let complete_path = temp_root.join(".complete");
    fs::write(&complete_path, b"")
        .unwrap_or_else(|e| panic!("write {}: {e}", complete_path.display()));

    match fs::rename(&temp_root, snapshot_root) {
        Ok(()) => {}
        Err(_) if perf_snapshot_complete(snapshot_root) => {
            let _ = fs::remove_dir_all(&temp_root);
        }
        Err(err) => panic!(
            "rename {} -> {}: {err}",
            temp_root.display(),
            snapshot_root.display()
        ),
    }
}

fn ensure_perf_file_snapshot(source: &Path) -> PathBuf {
    let snapshot_root = perf_file_snapshot_dir(source);
    if perf_snapshot_complete(&snapshot_root) {
        return snapshot_root;
    }
    if snapshot_root.exists() {
        remove_snapshot_root(&snapshot_root);
    }
    create_perf_file_snapshot(source, &snapshot_root);
    snapshot_root
}

fn prepare_single_file_fixture(source: &Path) -> SingleFileFixture {
    let source = normalized_snapshot_source(source);
    let snapshot_root = ensure_perf_file_snapshot(&source);
    let snapshot_input = snapshot_root.join("input");
    let snapshot_gzip = snapshot_root.join("input.gz");
    let input = fs::read(&snapshot_input)
        .unwrap_or_else(|e| panic!("read {}: {e}", snapshot_input.display()));
    let compressed = fs::read(&snapshot_gzip)
        .unwrap_or_else(|e| panic!("read {}: {e}", snapshot_gzip.display()));
    let output = gunzip(&compressed);
    assert_eq!(output, input, "failed for {}", source.display());
    SingleFileFixture {
        source,
        snapshot_root,
        input,
        compressed,
    }
}

fn prepare_corpus(root: &Path, limit: usize, max_bytes: u64) -> (Vec<CorpusEntry>, PathBuf) {
    let snapshot_root = ensure_perf_snapshot(root, limit, max_bytes);
    let paths = corpus_candidates(&snapshot_root, limit, max_bytes);
    assert!(
        !paths.is_empty(),
        "no matching files found under {}",
        root.display()
    );
    let corpus = paths
        .into_iter()
        .map(|snapshot_path| {
            let relative = snapshot_path
                .strip_prefix(&snapshot_root)
                .unwrap_or_else(|_| {
                    panic!(
                        "{} not under {}",
                        snapshot_path.display(),
                        snapshot_root.display()
                    )
                });
            let source_path = root.join(relative);
            let input = fs::read(&snapshot_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", snapshot_path.display()));
            let compressed = gzip_via_file(&snapshot_path, &["-n", "-c"]);
            let output = gunzip(&compressed);
            assert_eq!(output, input, "failed for {}", source_path.display());
            CorpusEntry {
                path: source_path,
                input,
                compressed,
            }
        })
        .collect();
    (corpus, snapshot_root)
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
    let (corpus, snapshot_root) = prepare_corpus(root, limit, max_bytes);
    let bytes: usize = corpus.iter().map(CorpusEntry::bytes).sum();
    let compressed_bytes: usize = corpus.iter().map(|entry| entry.compressed.len()).sum();

    eprintln!(
        "perf corpus root={} snapshot={} files={} bytes={} compressed_bytes={} iterations={} max_bytes={} extensions={}",
        root.display(),
        snapshot_root.display(),
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

fn run_single_file_perf(source: &Path, iterations: usize, repeat: usize) {
    assert!(iterations > 0, "iterations must be greater than zero");
    assert!(repeat > 0, "repeat must be greater than zero");

    let fixture = prepare_single_file_fixture(source);
    let source_bytes = fixture.input.len();
    let compressed_bytes = fixture.compressed.len();

    eprintln!(
        "perf file source={} snapshot={} source_bytes={} compressed_bytes={} iterations={} repeat={}",
        fixture.source.display(),
        fixture.snapshot_root.display(),
        source_bytes,
        compressed_bytes,
        iterations,
        repeat
    );

    let mut runs = Vec::with_capacity(iterations);
    for iteration in 0..iterations {
        let start = Instant::now();
        let mut sink = io::sink();
        let mut iteration_bytes = 0usize;
        for _ in 0..repeat {
            let output = gunzip(&fixture.compressed);
            assert_eq!(
                output.as_slice(),
                fixture.input.as_slice(),
                "failed for {}",
                fixture.source.display()
            );
            sink.write_all(&output).expect("write sink");
            iteration_bytes += output.len();
        }
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "iteration={} elapsed_ms={:.3} bytes={} decodes={}",
            iteration + 1,
            elapsed_ms,
            iteration_bytes,
            repeat
        );
        runs.push(PerfRun {
            elapsed_ms,
            bytes: iteration_bytes,
        });
    }

    let summary = summarize_runs(&runs, iterations);
    println!(
        "summary source={} snapshot={} source_bytes={} bytes={} compressed_bytes={} iterations={} repeat={} total_decodes={} total_ms={:.3} avg_ms={:.3} min_ms={:.3} median_ms={:.3} max_ms={:.3}",
        fixture.source.display(),
        fixture.snapshot_root.display(),
        source_bytes,
        summary.bytes,
        compressed_bytes,
        summary.iterations,
        repeat,
        summary.iterations * repeat,
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

    if cmd == "perf-file" {
        let mut path = None::<PathBuf>;
        let mut iterations = DEFAULT_PERF_ITERATIONS;
        let mut repeat = DEFAULT_PERF_REPEAT;
        let rest: Vec<String> = args.collect();
        let mut idx = 0usize;
        while idx < rest.len() {
            let flag = &rest[idx];
            let next = rest
                .get(idx + 1)
                .unwrap_or_else(|| panic!("missing value for {flag}"));
            match flag.as_str() {
                "--path" => path = Some(PathBuf::from(next)),
                "--iterations" => iterations = next.parse().expect("iterations must be an integer"),
                "--repeat" => repeat = next.parse().expect("repeat must be an integer"),
                _ => panic!("unknown perf-file arg: {flag}"),
            }
            idx += 2;
        }
        let path = path.expect("perf-file mode requires --path FILE");
        run_single_file_perf(&path, iterations, repeat);
        return;
    }

    let buf = fs::read(&cmd).expect("read file");
    io::stdout().write_all(&gunzip(&buf)).unwrap();
}

#[cfg(test)]
mod tests {
    use super::{
        corpus_candidates, gunzip, gzip_via_file, prepare_corpus, prepare_single_file_fixture,
    };
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
    fn perf_corpus_snapshot_reuses_copied_inputs_across_source_mutations() {
        let root = unique_temp_path("snapshot-root");
        fs::create_dir_all(root.join("nested")).expect("create temp root");
        let alpha = root.join("nested/alpha.md");
        let beta = root.join("beta.json");
        fs::write(&alpha, b"alpha snapshot\n").expect("write alpha");
        fs::write(&beta, b"{\"value\":1}\n").expect("write beta");

        let (first_corpus, snapshot_root) = prepare_corpus(&root, 10, 1024);
        let first_inputs: Vec<_> = first_corpus
            .iter()
            .map(|entry| (entry.path.clone(), entry.input.clone()))
            .collect();

        fs::write(&alpha, b"mutated live source\n").expect("rewrite alpha");
        fs::remove_file(&beta).expect("remove beta");

        let (second_corpus, second_snapshot_root) = prepare_corpus(&root, 10, 1024);
        let second_inputs: Vec<_> = second_corpus
            .iter()
            .map(|entry| (entry.path.clone(), entry.input.clone()))
            .collect();

        assert_eq!(snapshot_root, second_snapshot_root);
        assert_eq!(first_inputs, second_inputs);

        fs::remove_dir_all(&root).expect("remove temp root");
        fs::remove_dir_all(&snapshot_root).expect("remove snapshot root");
    }

    #[test]
    fn perf_file_snapshot_reuses_copied_input_across_source_mutations() {
        let root = unique_temp_path("single-file-root");
        fs::create_dir_all(&root).expect("create temp root");
        let source = root.join("large.txt");
        let input = b"single file snapshot\n".repeat(4096);
        fs::write(&source, &input).expect("write source");

        let first = prepare_single_file_fixture(&source);
        let first_input = first.input.clone();
        let first_compressed = first.compressed.clone();
        let snapshot_root = first.snapshot_root.clone();

        fs::write(&source, b"mutated live source\n").expect("rewrite source");

        let second = prepare_single_file_fixture(&source);
        assert_eq!(snapshot_root, second.snapshot_root);
        assert_eq!(first_input, second.input);
        assert_eq!(first_compressed, second.compressed);

        fs::remove_dir_all(&root).expect("remove temp root");
        fs::remove_dir_all(&snapshot_root).expect("remove snapshot root");
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
