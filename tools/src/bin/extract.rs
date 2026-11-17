//! Writes every texture in the unpacked archive as a PNG named the way Vita3K names replacements
//! ({hash:016X}.png), decoded and unswizzled as Vita3K exports it. With a third argument (Vita3K's
//! export folder), compares each texture Vita3K already exported against ours, pixel by pixel.
//!
//!   extract <unpacked folder> <png folder> [Vita3K textures\export folder]
use std::collections::{HashMap, HashSet};
use xxhash_rust::xxh3::xxh3_64;

struct Tex {
    w: u32,
    h: u32,
    fmt: u32,
    data: Vec<u8>,
}

fn rgb565(c: u16) -> [u8; 3] {
    let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
    [((r << 3) | (r >> 2)) as u8, ((g << 2) | (g >> 4)) as u8, ((b << 3) | (b >> 2)) as u8]
}

/// A DXT1 colour block (8 bytes) into 16 RGBA pixels.
fn dxt1_colours(b: &[u8], alpha_mode: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u32, wb: u32, d: u32| ((a as u32 * wa + b as u32 * wb) / d) as u8;
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !alpha_mode {
        pal[2] = [mix(p0[0], p1[0], 2, 1, 3), mix(p0[1], p1[1], 2, 1, 3), mix(p0[2], p1[2], 2, 1, 3), 255];
        pal[3] = [mix(p0[0], p1[0], 1, 2, 3), mix(p0[1], p1[1], 1, 2, 3), mix(p0[2], p1[2], 1, 2, 3), 255];
    } else {
        pal[2] = [mix(p0[0], p1[0], 1, 1, 2), mix(p0[1], p1[1], 1, 1, 2), mix(p0[2], p1[2], 1, 1, 2), 255];
        pal[3] = [0, 0, 0, 0];
    }
    let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 4]; 16];
    for (i, px) in out.iter_mut().enumerate() {
        *px = pal[((idx >> (2 * i)) & 3) as usize];
    }
    out
}

/// A DXT5 block (16 bytes): interpolated alpha, then a DXT1 colour block (decoded as Vita3K does,
/// with the 3-colour mode when c0 <= c1).
fn dxt5(b: &[u8]) -> [[u8; 4]; 16] {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut al = [0u8; 8];
    al[0] = a0 as u8;
    al[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7u32 {
            al[i as usize + 1] = (((7 - i) * a0 + i * a1) / 7) as u8;
        }
    } else {
        for i in 1..5u32 {
            al[i as usize + 1] = (((5 - i) * a0 + i * a1) / 5) as u8;
        }
        al[6] = 0;
        al[7] = 255;
    }
    let bits = u64::from_le_bytes([b[2], b[3], b[4], b[5], b[6], b[7], 0, 0]);
    let mut out = dxt1_colours(&b[8..16], true);
    for (i, px) in out.iter_mut().enumerate() {
        px[3] = al[((bits >> (3 * i)) & 7) as usize];
    }
    out
}

/// Position of block n of a Vita swizzled (Morton) image of bw x bh blocks: Z-order (y bit first)
/// inside squares of the smaller side, the squares laid along the longer side.
fn unswizzle(n: u32, bw: u32, bh: u32) -> (u32, u32) {
    let side = bw.min(bh).max(1);
    let per = side * side;
    let (tile, m) = (n / per, n % per);
    let (mut x, mut y) = (0u32, 0u32);
    let mut bit = 0;
    while (1u32 << bit) < side {
        y |= ((m >> (2 * bit)) & 1) << bit;
        x |= ((m >> (2 * bit + 1)) & 1) << bit;
        bit += 1;
    }
    if bw > bh { (x + tile * side, y) } else { (x, y + tile * side) }
}

fn decode(t: &Tex) -> Option<Vec<u8>> {
    let (w, h) = (t.w as usize, t.h as usize);
    let mut rgba = vec![0u8; w * h * 4];
    match t.fmt & 0x9F00_0000 {
        0x8500_0000 | 0x8700_0000 => {
            let bs = if t.fmt & 0x9F00_0000 == 0x8500_0000 { 8 } else { 16 };
            let (bw, bh) = (t.w.div_ceil(4), t.h.div_ceil(4));
            for n in 0..bw * bh {
                let src = t.data.get(n as usize * bs..n as usize * bs + bs)?;
                let px = if bs == 8 { dxt1_colours(src, true) } else { dxt5(src) };
                let (bx, by) = unswizzle(n, bw, bh);
                for (i, p) in px.iter().enumerate() {
                    let (x, y) = (bx as usize * 4 + i % 4, by as usize * 4 + i / 4);
                    if x < w && y < h {
                        rgba[(y * w + x) * 4..][..4].copy_from_slice(p);
                    }
                }
            }
        }
        0x0C00_0000 => {
            // 32-bit textures are stored row by row (not swizzled). ARGB keeps bytes as B,G,R,A.
            let argb = (t.fmt >> 12) & 7 == 1;
            for n in 0..(t.w * t.h) as usize {
                let s = t.data.get(n * 4..n * 4 + 4)?;
                let px = if argb { [s[2], s[1], s[0], s[3]] } else { [s[0], s[1], s[2], s[3]] };
                rgba[n * 4..][..4].copy_from_slice(&px);
            }
        }
        _ => return None,
    }
    Some(rgba)
}

fn hash(t: &Tex) -> Option<u64> {
    let (bits, bw, bh) = match t.fmt & 0x9F00_0000 {
        0x8500_0000 => (4, 4, 4),
        0x8700_0000 => (8, 4, 4),
        0x0C00_0000 => (32, 1, 1),
        _ => return None,
    };
    let (aw, ah) = (t.w.div_ceil(bw) * bw, t.h.div_ceil(bh) * bh);
    let first = (aw as usize * ah as usize * bits) / 8;
    let swz = ((t.fmt >> 12) & 7) as u64;
    Some(xxh3_64(t.data.get(..first)?) ^ ((aw as u64) << 16 | ah as u64 | swz << 33))
}

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut e = png::Encoder::new(f, w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn read_png(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
    let d = png::Decoder::new(std::fs::File::open(path).ok()?);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        _ => return None,
    };
    Some((info.width, info.height, rgba))
}

fn walk(p: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
        let q = e.path();
        if q.is_dir() { v.extend(walk(&q)); } else { v.push(q); }
    }
    v
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, out) = (std::path::Path::new(&a[1]), std::path::Path::new(&a[2]));
    let export = a.get(3).map(std::path::PathBuf::from);
    std::fs::create_dir_all(out).unwrap();
    let mut seen: HashSet<u64> = HashSet::new();
    let (mut written, mut skipped) = (0, 0);
    let mut exported: HashMap<u64, std::path::PathBuf> = HashMap::new();
    if let Some(e) = &export {
        for p in walk(e) {
            if let Some(h) = p.file_stem().and_then(|s| s.to_str()).and_then(|s| u64::from_str_radix(s, 16).ok()) {
                exported.entry(h).or_insert(p);
            }
        }
    }
    let (mut compared, mut same) = (0, 0);
    let mut worst: Vec<(u32, String)> = Vec::new();
    let mut fmts: HashMap<u32, u32> = HashMap::new();
    let mut map: Vec<String> = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let d = std::fs::read(e.path()).unwrap();
        if d.len() < 0x34 || d[..4] != [0, 4, 0, 0] {
            continue;
        }
        let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        if u(0x0C) != 0x1C {
            continue;
        }
        let t = Tex { w: u(0x18), h: u(0x1C), fmt: u(0x20), data: d[0x34..(0x34 + u(0x14) as usize).min(d.len())].to_vec() };
        let Some(hv) = hash(&t) else { skipped += 1; *fmts.entry(t.fmt).or_insert(0) += 1; continue };
        if !seen.insert(hv) {
            continue;
        }
        let Some(rgba) = decode(&t) else { skipped += 1; continue };
        write_png(&out.join(format!("{hv:016X}.png")), t.w, t.h, &rgba);
        map.push(format!("{hv:016X}	{}	{:08x}	{}x{}", e.file_name().to_string_lossy(), t.fmt, t.w, t.h));
        written += 1;
        if let Some(p) = exported.get(&hv) {
            if let Some((ew, eh, theirs)) = read_png(p) {
                compared += 1;
                if (ew, eh) == (t.w, t.h) {
                    let diff = rgba.iter().zip(&theirs).map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs()).max().unwrap_or(0);
                    if diff <= 8 { same += 1 } else { worst.push((diff, format!("{hv:016X} fmt {:08x} {}x{}", t.fmt, t.w, t.h))); }
                } else {
                    worst.push((999, format!("{hv:016X} size {ew}x{eh} vs {}x{}", t.w, t.h)));
                }
            }
        }
    }
    std::fs::write(out.with_extension("tsv"), map.join("
")).unwrap();
    println!("textures written: {written}, unsupported: {skipped}");
    for (f, n) in &fmts { println!("  unsupported format {f:08x}: {n}"); }
    if export.is_some() {
        println!("compared with Vita3K's own exports: {compared}, identical (within 8 levels): {same}");
        worst.sort();
        worst.reverse();
        for (d, s) in worst.iter().take(10) {
            println!("  differs by {d}: {s}");
        }
    }
}
