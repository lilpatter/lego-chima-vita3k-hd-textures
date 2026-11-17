//! Unpacks a TT Fusion FUSE1.00 archive: header, index of (name hash, offset, size) at the offset in
//! the header. A packed file is a run of chunks, each a u32 length + TT's RefPack variant; the rest
//! are stored as is. The index's size field >> 5 is the unpacked size, which every packed file matches.
//!
//!   unpack <lego_chima_psp2.fib> <output folder>
use std::io::Write;

/// Decompresses one chunk onto the end of `out` (back-references may reach into earlier chunks).
fn unpack_into(src: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let mut i = 0usize;
    let n = src.len();
    let get = |k: usize| src.get(k).copied().ok_or_else(|| "truncated".to_string());
    while i < n {
        let b0 = src[i];
        let (lit, len, off);
        if b0 < 0x80 {
            let b1 = get(i + 1)?; i += 2;
            lit = ((b0 >> 2) & 3) as usize; len = ((b0 >> 4) & 7) as usize + 3; off = ((((b0 & 3) as usize) << 8) | b1 as usize) + 1;
        } else if b0 < 0xC0 {
            let (b1, b2) = (get(i + 1)?, get(i + 2)?); i += 3;
            lit = (b1 >> 6) as usize; len = (b0 & 0x3F) as usize + 4; off = ((((b1 & 0x3F) as usize) << 8) | b2 as usize) + 1;
        } else if b0 < 0xE0 {
            let (b1, b2, b3) = (get(i + 1)?, get(i + 2)?, get(i + 3)?); i += 4;
            // 110L LHHO: L literals, H the length's top bits, O the offset's 17th bit.
            lit = ((b0 >> 3) & 3) as usize; len = ((((b0 & 6) as usize) << 7) | b3 as usize) + 5; off = ((((b0 & 1) as usize) << 16) | ((b1 as usize) << 8) | b2 as usize) + 1;
        } else if b0 < 0xFC {
            i += 1;
            let l = ((b0 & 0x1F) as usize + 1) * 4;
            out.extend_from_slice(src.get(i..i + l).ok_or("truncated literal")?); i += l;
            continue;
        } else {
            i += 1;
            let l = (b0 & 3) as usize;
            out.extend_from_slice(src.get(i..i + l).ok_or("truncated end")?); i += l;
            if i != n { return Err(format!("end token at {i} of {n}")); }
            return Ok(());
        }
        out.extend_from_slice(src.get(i..i + lit).ok_or("truncated literal")?); i += lit;
        if off > out.len() { return Err(format!("offset {off} > {}", out.len())); }
        let start = out.len() - off;
        for k in 0..len { let c = out[start + k]; out.push(c); }
    }
    Ok(())
}

/// A packed entry: chunks of (u32 compressed length, data) filling the entry's space exactly.
fn unpack(src: &[u8], want: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(want);
    let mut p = 0usize;
    while p < src.len() {
        let z = u32::from_le_bytes(src.get(p..p + 4).ok_or("short chunk header")?.try_into().unwrap()) as usize;
        let chunk = src.get(p + 4..p + 4 + z).ok_or("chunk past the end")?;
        unpack_into(chunk, &mut out)?;
        p += 4 + z;
    }
    Ok(out)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (fib, outdir) = (&args[1], std::path::Path::new(&args[2]));
    let b = std::fs::read(fib).unwrap();
    assert_eq!(&b[..8], b"FUSE1.00");
    let u32at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let (files, info) = (u32at(8) as usize, u32at(16) as usize);
    let mut ents: Vec<(u32, usize, u32)> = (0..files).map(|k| (u32at(info + k * 12), u32at(info + k * 12 + 4) as usize, u32at(info + k * 12 + 8))).collect();
    ents.sort_by_key(|e| e.1);
    std::fs::create_dir_all(outdir).unwrap();
    let mut index = std::fs::File::create(outdir.join("index.tsv")).unwrap();
    let (mut packed, mut stored, mut failed, mut mismatched) = (0, 0, 0, 0);
    for k in 0..ents.len() {
        let (hash, off, size) = ents[k];
        let end = if k + 1 < ents.len() { ents[k + 1].1 } else { info };
        let want = (size >> 5) as usize;
        let (data, how) = match unpack(&b[off..end], want) {
            Ok(d) => (d, "packed"),
            Err(_) => (b[off..end].to_vec(), "stored"),
        };
        if how == "packed" && data.len() != want { mismatched += 1; }
        match how { "packed" => packed += 1, _ => stored += 1 }
        if data.is_empty() { failed += 1; }
        std::fs::write(outdir.join(format!("{hash:08x}.bin")), &data).unwrap();
        writeln!(index, "{hash:08x}\t{off}\t{size:#x}\t{how}\t{}", data.len()).unwrap();
    }
    println!("files {files}: packed {packed}, stored {stored}, empty {failed}, size differs from index {mismatched}");
}
