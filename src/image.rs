//! Dependency-free PNG decoding, integer image fitting and a bounded cache.
//!
//! Relative `Image.src` paths resolve against the renderer process's current
//! working directory, not the IR file's directory. Keys are lexically
//! normalised absolute paths; symlinks are not canonicalised. Decoding covers
//! non-interlaced 8-bit grayscale, RGB and RGBA PNGs with filter types 0..4,
//! CRC-32 and Adler-32 checks, and zlib stored/fixed/dynamic DEFLATE blocks.
//! Palette, gray+alpha, other bit depths and Adam7 are rejected with an error.
//! `fit_rect` and the raster sampler use integer arithmetic and
//! nearest-neighbour sampling; the canvas alpha-blends the resulting RGBA.
//! A missing, unreadable or invalid resource is cached as `None` and the raster
//! paints a deterministic placeholder, so bad input is never a render error.

use crate::Rect;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Maximum accepted PNG width or height.
pub const MAX_IMAGE_DIMENSION: u32 = 4096;
/// Maximum accepted decoded RGBA payload (4096x4096x4).
pub const MAX_DECODED_BYTES: usize =
    MAX_IMAGE_DIMENSION as usize * MAX_IMAGE_DIMENSION as usize * 4;
const MAX_RAW_BYTES: usize = MAX_DECODED_BYTES + MAX_IMAGE_DIMENSION as usize;
const MAX_COMPRESSED_BYTES: usize = MAX_RAW_BYTES + 1_048_576;
/// Maximum number of distinct resolved paths retained by the image cache.
pub const CACHE_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    /// Preserve the whole source aspect ratio inside the destination (default).
    Contain,
    /// Preserve the source aspect ratio while covering the destination.
    Cover,
    /// Stretch independently on both axes.
    Fill,
}

impl Default for ImageFit {
    fn default() -> Self {
        ImageFit::Contain
    }
}

/// Decoded image as opaque-canvas-ready RGBA8 pixels in row-major order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub evictions: u64,
}

#[derive(Default)]
struct CacheState {
    images: HashMap<PathBuf, Option<Arc<Image>>>,
    order: VecDeque<PathBuf>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

static CACHE: OnceLock<Mutex<CacheState>> = OnceLock::new();

fn cache_state() -> MutexGuard<'static, CacheState> {
    CACHE
        .get_or_init(|| Mutex::new(CacheState::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Resolve a src path to a stable absolute path using the process CWD.
pub fn resolve(path: &str) -> PathBuf {
    let raw = Path::new(path);
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(raw)
    };
    let mut out = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Reset the cache and its counters. Useful for deterministic tests.
pub fn clear_cache() {
    *cache_state() = CacheState::default();
}

pub fn cache_stats() -> CacheStats {
    let state = cache_state();
    CacheStats {
        hits: state.hits,
        misses: state.misses,
        entries: state.images.len(),
        evictions: state.evictions,
    }
}

/// Load and decode a PNG on first use; cache both successes and failures.
pub fn cached(path: &str) -> Option<Arc<Image>> {
    let key = resolve(path);
    let mut state = cache_state();
    if let Some(entry) = state.images.get(&key) {
        let loaded = entry.clone();
        state.hits += 1;
        return loaded;
    }
    state.misses += 1;
    let loaded = fs::read(&key)
        .ok()
        .and_then(|bytes| decode(&bytes).ok())
        .map(Arc::new);
    while state.images.len() >= CACHE_CAPACITY {
        let Some(oldest) = state.order.pop_front() else {
            break;
        };
        if state.images.remove(&oldest).is_some() {
            state.evictions += 1;
        }
    }
    state.images.insert(key.clone(), loaded.clone());
    state.order.push_back(key);
    loaded
}

/// Integer-only fit rect relative to a destination rect. `cover` offsets can
/// be negative because the source is cropped; `contain` leaves letterboxing.
pub fn fit_rect(src_w: usize, src_h: usize, dst: Rect, fit: ImageFit) -> Rect {
    if src_w == 0 || src_h == 0 || dst.w <= 0 || dst.h <= 0 {
        return Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
    }
    if fit == ImageFit::Fill {
        return Rect {
            x: 0,
            y: 0,
            w: dst.w,
            h: dst.h,
        };
    }
    let sw = src_w as i128;
    let sh = src_h as i128;
    let dw = dst.w as i128;
    let dh = dst.h as i128;
    let source_wider = sw * dh > dw * sh;
    let (w, h) = match (fit, source_wider) {
        (ImageFit::Contain, true) => (dw, rounded_ratio(sh * dw, sw).min(dh)),
        (ImageFit::Contain, false) => (rounded_ratio(sw * dh, sh).min(dw), dh),
        (ImageFit::Cover, true) => (rounded_ratio(sw * dh, sh).max(dw), dh),
        (ImageFit::Cover, false) => (dw, rounded_ratio(sh * dw, sw).max(dh)),
        (ImageFit::Fill, _) => (dw, dh),
    };
    let w = w.max(1);
    let h = h.max(1);
    let w = if fit == ImageFit::Contain {
        w.min(dw)
    } else {
        w
    };
    let h = if fit == ImageFit::Contain {
        h.min(dh)
    } else {
        h
    };
    let w = w.min(i64::MAX as i128) as i64;
    let h = h.min(i64::MAX as i128) as i64;
    Rect {
        x: (dst.w - w) / 2,
        y: (dst.h - h) / 2,
        w,
        h,
    }
}

fn rounded_ratio(num: i128, den: i128) -> i128 {
    if den <= 0 {
        0
    } else {
        (num + den / 2) / den
    }
}

struct Ihdr {
    width: usize,
    height: usize,
    channels: usize,
    row_bytes: usize,
    raw_len: usize,
    rgba_len: usize,
}

/// Decode one PNG byte slice. Every malformed input returns a readable error.
pub fn decode(input: &[u8]) -> Result<Image, String> {
    const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if input.len() < SIGNATURE.len() || input.get(..8) != Some(SIGNATURE.as_slice()) {
        return Err("PNG signature is missing or invalid".into());
    }
    let mut pos = 8usize;
    let mut first = true;
    let mut ihdr: Option<Ihdr> = None;
    let mut idat = Vec::new();
    let mut ended = false;
    while pos < input.len() {
        if input.len() - pos < 12 {
            return Err("truncated PNG chunk header".into());
        }
        let len = usize::try_from(be32(&input[pos..pos + 4])?)
            .map_err(|_| "PNG chunk length is too large".to_string())?;
        if len > input.len() - pos - 12 {
            return Err("truncated PNG chunk data".into());
        }
        let kind = &input[pos + 4..pos + 8];
        if !kind.iter().all(|b| b.is_ascii_alphabetic()) {
            return Err("PNG chunk type is not four ASCII letters".into());
        }
        let data = &input[pos + 8..pos + 8 + len];
        let expected_crc = be32(&input[pos + 8 + len..pos + 12 + len])?;
        if crc32(&[kind, data]) != expected_crc {
            return Err(format!(
                "PNG chunk {} has an invalid CRC-32",
                String::from_utf8_lossy(kind)
            ));
        }
        match kind {
            b"IHDR" => {
                if !first {
                    return Err("IHDR must be the first PNG chunk".into());
                }
                ihdr = Some(parse_ihdr(data)?);
            }
            b"IDAT" => {
                if ihdr.is_none() {
                    return Err("IDAT appears before IHDR".into());
                }
                if idat.len().saturating_add(data.len()) > MAX_COMPRESSED_BYTES {
                    return Err("compressed PNG data exceeds the size cap".into());
                }
                idat.extend_from_slice(data);
            }
            b"IEND" => {
                if len != 0 {
                    return Err("IEND must be empty".into());
                }
                ended = true;
                break;
            }
            _ if kind[0].is_ascii_uppercase() => {
                return Err(format!(
                    "unsupported critical PNG chunk {}",
                    String::from_utf8_lossy(kind)
                ));
            }
            _ => {}
        }
        first = false;
        pos = pos.saturating_add(len).saturating_add(12);
    }
    let Some(ihdr) = ihdr else {
        return Err("PNG is missing its IHDR chunk".into());
    };
    if !ended {
        return Err("PNG is missing its IEND chunk".into());
    }
    if idat.is_empty() {
        return Err("PNG has no IDAT data".into());
    }
    let raw = inflate_zlib(&idat, ihdr.raw_len)?;
    let pixels = unfilter(&raw, &ihdr)?;
    Ok(Image {
        width: ihdr.width,
        height: ihdr.height,
        pixels,
    })
}

fn parse_ihdr(data: &[u8]) -> Result<Ihdr, String> {
    if data.len() != 13 {
        return Err("IHDR must be exactly 13 bytes".into());
    }
    let width =
        usize::try_from(be32(&data[0..4])?).map_err(|_| "PNG width is too large".to_string())?;
    let height =
        usize::try_from(be32(&data[4..8])?).map_err(|_| "PNG height is too large".to_string())?;
    let bit_depth = data[8];
    let color_type = data[9];
    if width == 0 || height == 0 {
        return Err("PNG dimensions must be positive".into());
    }
    if width > MAX_IMAGE_DIMENSION as usize || height > MAX_IMAGE_DIMENSION as usize {
        return Err(format!(
            "PNG dimensions must be at most {MAX_IMAGE_DIMENSION}x{MAX_IMAGE_DIMENSION}"
        ));
    }
    if bit_depth != 8 {
        return Err("unsupported PNG bit depth: only 8-bit samples are accepted".into());
    }
    let channels = match color_type {
        0 => 1,
        2 => 3,
        6 => 4,
        other => {
            return Err(format!(
                "unsupported PNG color type {other}: only grayscale, RGB and RGBA are accepted"
            ))
        }
    };
    if data[10] != 0 || data[11] != 0 || data[12] != 0 {
        return Err("unsupported PNG compression, filter or interlace method".into());
    }
    let row_bytes = width
        .checked_mul(channels)
        .ok_or_else(|| "PNG row is too large".to_string())?;
    let raw_len = row_bytes
        .checked_add(1)
        .and_then(|row| row.checked_mul(height))
        .ok_or_else(|| "PNG scanline data is too large".to_string())?;
    let rgba_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "PNG decoded data is too large".to_string())?;
    if raw_len > MAX_RAW_BYTES || rgba_len > MAX_DECODED_BYTES {
        return Err("PNG decoded data exceeds the size cap".into());
    }
    Ok(Ihdr {
        width,
        height,
        channels,
        row_bytes,
        raw_len,
        rgba_len,
    })
}

fn inflate_zlib(stream: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    if stream.len() < 6 {
        return Err("truncated zlib stream".into());
    }
    let cmf = stream[0];
    let flg = stream[1];
    if cmf & 0x0f != 8 || cmf >> 4 > 7 {
        return Err("unsupported zlib compression method".into());
    }
    if flg & 0x20 != 0 {
        return Err("zlib preset dictionaries are unsupported".into());
    }
    if ((u16::from(cmf) << 8) | u16::from(flg)) % 31 != 0 {
        return Err("zlib header has an invalid checksum".into());
    }
    let mut bits = BitReader {
        data: stream,
        pos: 2,
        bit: 0,
    };
    let raw = inflate(&mut bits, expected)?;
    bits.align_to_byte();
    if bits.pos.saturating_add(4) != stream.len() {
        return Err("zlib stream has missing or trailing bytes".into());
    }
    let stored = be32(&stream[bits.pos..bits.pos + 4])?;
    let actual = adler32(&raw);
    if stored != actual {
        return Err("zlib Adler-32 checksum mismatch".into());
    }
    Ok(raw)
}

fn inflate(bits: &mut BitReader<'_>, expected: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected.min(MAX_RAW_BYTES));
    loop {
        let final_block = bits.read_bits(1)? != 0;
        let block_type = bits.read_bits(2)?;
        match block_type {
            0 => stored_block(bits, &mut out, expected)?,
            1 => {
                let (lit, dist) = fixed_trees()?;
                huffman_block(bits, &lit, &dist, &mut out, expected)?;
            }
            2 => {
                let (lit, dist) = dynamic_trees(bits)?;
                huffman_block(bits, &lit, &dist, &mut out, expected)?;
            }
            _ => return Err("invalid DEFLATE block type 3".into()),
        }
        if final_block {
            break;
        }
    }
    if out.len() != expected {
        return Err(format!(
            "PNG scanline data has {} bytes; expected {expected}",
            out.len()
        ));
    }
    Ok(out)
}

fn stored_block(
    bits: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    expected: usize,
) -> Result<(), String> {
    bits.align_to_byte();
    let len = bits.read_u16_le()? as usize;
    let nlen = bits.read_u16_le()? as usize;
    if len ^ 0xffff != nlen {
        return Err("DEFLATE stored block length complement is invalid".into());
    }
    if out.len().saturating_add(len) > expected {
        return Err("DEFLATE output exceeds the PNG scanline size".into());
    }
    let data = bits.take(len)?;
    out.extend_from_slice(data);
    Ok(())
}

fn fixed_trees() -> Result<(Huffman, Huffman), String> {
    let mut lit = [0u8; 288];
    for (symbol, len) in lit.iter_mut().enumerate() {
        *len = match symbol {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let dist = [5u8; 32];
    Ok((Huffman::build(&lit)?, Huffman::build(&dist)?))
}

const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn dynamic_trees(bits: &mut BitReader<'_>) -> Result<(Huffman, Huffman), String> {
    let hlit = bits.read_bits(5)? as usize + 257;
    let hdist = bits.read_bits(5)? as usize + 1;
    let hclen = bits.read_bits(4)? as usize + 4;
    if hlit > 286 || hdist > 32 || hclen > 19 {
        return Err("dynamic DEFLATE header has out-of-range code counts".into());
    }
    let mut code_lengths = [0u8; 19];
    for &index in CODE_LENGTH_ORDER.iter().take(hclen) {
        code_lengths[index] = bits.read_bits(3)? as u8;
    }
    let code_tree = Huffman::build(&code_lengths)?;
    let total = hlit.saturating_add(hdist);
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let symbol = code_tree.decode(bits)? as usize;
        let (value, repeat) = match symbol {
            0..=15 => (symbol as u8, 1),
            16 => {
                let previous = *lengths
                    .last()
                    .ok_or_else(|| "DEFLATE repeat has no previous code length".to_string())?;
                (previous, 3 + bits.read_bits(2)? as usize)
            }
            17 => (0, 3 + bits.read_bits(3)? as usize),
            18 => (0, 11 + bits.read_bits(7)? as usize),
            _ => return Err("invalid DEFLATE code-length symbol".into()),
        };
        if lengths.len().saturating_add(repeat) > total {
            return Err("DEFLATE code-length repeat exceeds the header".into());
        }
        lengths.resize(lengths.len() + repeat, value);
    }
    if lengths.get(256).copied().unwrap_or(0) == 0 {
        return Err("dynamic DEFLATE block has no end-of-block code".into());
    }
    Ok((
        Huffman::build(&lengths[..hlit])?,
        Huffman::build(&lengths[hlit..])?,
    ))
}

fn huffman_block(
    bits: &mut BitReader<'_>,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    expected: usize,
) -> Result<(), String> {
    loop {
        let symbol = lit.decode(bits)? as usize;
        match symbol {
            0..=255 => push_byte(out, symbol as u8, expected)?,
            256 => return Ok(()),
            257..=285 => {
                let index = symbol - 257;
                let length = LENGTH_BASE[index] + bits.read_bits(LENGTH_EXTRA[index])? as usize;
                let distance_symbol = dist.decode(bits)? as usize;
                if distance_symbol >= DIST_BASE.len() {
                    return Err("invalid DEFLATE distance symbol".into());
                }
                let distance = DIST_BASE[distance_symbol]
                    + bits.read_bits(DIST_EXTRA[distance_symbol])? as usize;
                if distance == 0 || distance > out.len() {
                    return Err("DEFLATE distance exceeds available output".into());
                }
                if out.len().saturating_add(length) > expected {
                    return Err("DEFLATE output exceeds the PNG scanline size".into());
                }
                for _ in 0..length {
                    let byte = *out
                        .get(out.len() - distance)
                        .ok_or_else(|| "DEFLATE back-reference is out of bounds".to_string())?;
                    out.push(byte);
                }
            }
            _ => return Err("invalid DEFLATE literal/length symbol".into()),
        }
    }
}

fn push_byte(out: &mut Vec<u8>, byte: u8, expected: usize) -> Result<(), String> {
    if out.len() >= expected || out.len() >= MAX_RAW_BYTES {
        return Err("DEFLATE output exceeds the PNG scanline size".into());
    }
    out.push(byte);
    Ok(())
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn build(lengths: &[u8]) -> Result<Huffman, String> {
        let mut counts = [0u16; 16];
        for &length in lengths {
            if length > 15 {
                return Err("DEFLATE Huffman code length exceeds 15 bits".into());
            }
            if length > 0 {
                counts[length as usize] += 1;
            }
        }
        let mut left = 1i32;
        for bits in 1..=15 {
            left = (left << 1) - i32::from(counts[bits]);
            if left < 0 {
                return Err("DEFLATE Huffman code is oversubscribed".into());
            }
        }
        let total: usize = counts.iter().skip(1).map(|&n| usize::from(n)).sum();
        let mut offsets = [0usize; 16];
        let mut cursor = 0usize;
        for bits in 1..=15 {
            offsets[bits] = cursor;
            cursor = cursor.saturating_add(usize::from(counts[bits]));
        }
        let mut symbols = vec![0u16; total];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length > 0 {
                let at = &mut offsets[length as usize];
                if let Some(slot) = symbols.get_mut(*at) {
                    *slot = symbol as u16;
                } else {
                    return Err("DEFLATE Huffman symbol table is inconsistent".into());
                }
                *at += 1;
            }
        }
        Ok(Huffman { counts, symbols })
    }

    fn decode(&self, bits: &mut BitReader<'_>) -> Result<u16, String> {
        if self.symbols.is_empty() {
            return Err("DEFLATE Huffman tree is empty".into());
        }
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0usize;
        for len in 1..=15 {
            code |= i32::from(bits.read_bit()?);
            let count = i32::from(self.counts[len]);
            if code - first < count {
                let delta = code - first;
                if delta < 0 {
                    return Err("invalid DEFLATE Huffman code".into());
                }
                return self
                    .symbols
                    .get(index + delta as usize)
                    .copied()
                    .ok_or_else(|| "invalid DEFLATE Huffman symbol index".into());
            }
            index = index.saturating_add(count as usize);
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("invalid DEFLATE Huffman code".into())
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u8,
}

impl<'a> BitReader<'a> {
    fn read_bit(&mut self) -> Result<u8, String> {
        let byte = *self
            .data
            .get(self.pos)
            .ok_or_else(|| "truncated DEFLATE bit stream".to_string())?;
        let bit = (byte >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.pos = self.pos.saturating_add(1);
        }
        Ok(bit)
    }

    fn read_bits(&mut self, count: u8) -> Result<u32, String> {
        let mut value = 0u32;
        for shift in 0..count {
            value |= u32::from(self.read_bit()?) << shift;
        }
        Ok(value)
    }

    fn align_to_byte(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos = self.pos.saturating_add(1);
        }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        if self.bit != 0 {
            return Err("DEFLATE stored block is not byte-aligned".into());
        }
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| "DEFLATE stored block length is too large".to_string())?;
        let data = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| "truncated DEFLATE stored block".to_string())?;
        self.pos = end;
        Ok(data)
    }

    fn read_u16_le(&mut self) -> Result<u16, String> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
}

fn unfilter(raw: &[u8], ihdr: &Ihdr) -> Result<Vec<u8>, String> {
    let stride = ihdr
        .row_bytes
        .checked_add(1)
        .ok_or_else(|| "PNG scanline stride is too large".to_string())?;
    if raw.len() != ihdr.raw_len {
        return Err("PNG scanline byte count does not match IHDR".into());
    }
    let mut pixels = Vec::with_capacity(ihdr.rgba_len);
    let mut previous = vec![0u8; ihdr.row_bytes];
    for y in 0..ihdr.height {
        let start = y * stride;
        let filter = *raw
            .get(start)
            .ok_or_else(|| "truncated PNG scanline filter".to_string())?;
        if filter > 4 {
            return Err(format!("unsupported PNG filter type {filter}"));
        }
        let row = raw
            .get(start + 1..start + stride)
            .ok_or_else(|| "truncated PNG scanline data".to_string())?;
        let mut current = row.to_vec();
        for i in 0..ihdr.row_bytes {
            let left = if i >= ihdr.channels {
                current[i - ihdr.channels]
            } else {
                0
            };
            let up = previous[i];
            let up_left = if i >= ihdr.channels {
                previous[i - ihdr.channels]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth(left, up, up_left),
                _ => unreachable!("filter range checked above"),
            };
            current[i] = current[i].wrapping_add(predictor);
        }
        for x in 0..ihdr.width {
            let source = x * ihdr.channels;
            let px = current
                .get(source..source + ihdr.channels)
                .ok_or_else(|| "truncated PNG pixel data".to_string())?;
            match ihdr.channels {
                1 => pixels.extend_from_slice(&[px[0], px[0], px[0], 255]),
                3 => pixels.extend_from_slice(&[px[0], px[1], px[2], 255]),
                4 => pixels.extend_from_slice(px),
                _ => return Err("unsupported PNG channel count".into()),
            }
        }
        previous = current;
    }
    if pixels.len() != ihdr.rgba_len {
        return Err("PNG decoded RGBA byte count is inconsistent".into());
    }
    Ok(pixels)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (i32::from(a), i32::from(b), i32::from(c));
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

fn be32(bytes: &[u8]) -> Result<u32, String> {
    let bytes = bytes
        .get(..4)
        .ok_or_else(|| "truncated 32-bit PNG field".to_string())?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut table = [0u32; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut value = i as u32;
        for _ in 0..8 {
            value = if value & 1 != 0 {
                0xedb8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
        }
        *entry = value;
    }
    let mut crc = 0xffff_ffffu32;
    for part in parts {
        for &byte in *part {
            let index = ((crc ^ u32::from(byte)) & 0xff) as usize;
            crc = table[index] ^ (crc >> 8);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u64;
    let mut b = 0u64;
    for &byte in data {
        a = (a + u64::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    ((b << 16) | a) as u32
}
