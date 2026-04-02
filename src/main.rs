use std::{
    env,
    io::{self, Read, Write},
};

fn main() {
    let args: Vec<String> = env::args().collect();
    let buf: Vec<u8> = if args.len() > 1 {
        std::fs::read(&args[1]).expect("read file")
    } else {
        let mut b = Vec::new();
        io::stdin().read_to_end(&mut b).expect("read stdin");
        b
    };

    let out = mini_gzip::inflate_gzip(&buf).expect("inflate gzip");
    io::stdout().write_all(&out).unwrap();
}
