//! Real PNG fixtures, decoder validation, image rendering, cache accounting,
//! opt-in text typography and the three pinned golden digest proofs.

use opurom_render::image::{self, ImageFit};
use opurom_render::{
    digest_hex, font, layout, parse_ir, Align, Ir, Kind, LaidOut, Rect, Renderer, StateValue,
    StateView, CACHE_CAPACITY,
};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn cache_guard() -> MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the Unix epoch")
        .as_nanos();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("image-media-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create test temp dir");
    dir
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn crc32(data: &[u8]) -> u32 {
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
    for &byte in data {
        let index = ((crc ^ u32::from(byte)) & 0xff) as usize;
        crc = table[index] ^ (crc >> 8);
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
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

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit: u8,
}

impl BitWriter {
    fn write_bit(&mut self, bit: u8) {
        if self.bit == 0 {
            self.bytes.push(0);
        }
        let last = self.bytes.len() - 1;
        if bit != 0 {
            self.bytes[last] |= 1 << self.bit;
        }
        self.bit = (self.bit + 1) % 8;
    }

    fn write_lsb(&mut self, value: u32, count: u8) {
        for shift in 0..count {
            self.write_bit(((value >> shift) & 1) as u8);
        }
    }

    fn write_msb(&mut self, value: u32, count: u8) {
        for shift in (0..count).rev() {
            self.write_bit(((value >> shift) & 1) as u8);
        }
    }

    fn align(&mut self) {
        self.bit = 0;
    }

    fn finish(mut self) -> Vec<u8> {
        self.align();
        self.bytes
    }
}

#[derive(Clone, Copy)]
enum BlockMode {
    Stored,
    Fixed,
}

fn stored_deflate(raw: &[u8]) -> Vec<u8> {
    let mut bits = BitWriter::default();
    let chunks: Vec<&[u8]> = if raw.is_empty() {
        vec![&[]]
    } else {
        raw.chunks(65_535).collect()
    };
    for (i, chunk) in chunks.iter().enumerate() {
        bits.write_lsb(u32::from(i + 1 == chunks.len()), 1);
        bits.write_lsb(0, 2);
        bits.align();
        let len = chunk.len() as u16;
        bits.bytes.extend_from_slice(&len.to_le_bytes());
        bits.bytes.extend_from_slice(&(!len).to_le_bytes());
        bits.bytes.extend_from_slice(chunk);
    }
    bits.finish()
}

fn fixed_literal(symbol: usize) -> (u32, u8) {
    match symbol {
        0..=143 => ((0x30 + symbol) as u32, 8),
        144..=255 => ((0x190 + symbol - 144) as u32, 9),
        256..=279 => ((symbol - 256) as u32, 7),
        _ => ((0xc0 + symbol - 280) as u32, 8),
    }
}

fn fixed_deflate(raw: &[u8]) -> Vec<u8> {
    let mut bits = BitWriter::default();
    bits.write_lsb(1, 1);
    bits.write_lsb(1, 2);
    for &byte in raw {
        let (code, len) = fixed_literal(byte as usize);
        bits.write_msb(code, len);
    }
    bits.write_msb(0, 7); // end-of-block symbol 256
    bits.finish()
}

fn zlib_stream(raw: &[u8], mode: BlockMode) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let deflate = match mode {
        BlockMode::Stored => stored_deflate(raw),
        BlockMode::Fixed => fixed_deflate(raw),
    };
    out.extend_from_slice(&deflate);
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn ihdr(width: u32, height: u32, bit_depth: u8, color_type: u8, interlace: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(13);
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&[bit_depth, color_type, 0, 0, interlace]);
    out
}

fn make_png(width: u32, height: u32, color_type: u8, raw: &[u8], mode: BlockMode) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend_from_slice(&png_chunk(b"IHDR", &ihdr(width, height, 8, color_type, 0)));
    out.extend_from_slice(&png_chunk(b"IDAT", &zlib_stream(raw, mode)));
    out.extend_from_slice(&png_chunk(b"IEND", &[]));
    out
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

fn filter_row(kind: u8, row: &[u8], previous: &[u8], bpp: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(row.len() + 1);
    out.push(kind);
    for (i, &value) in row.iter().enumerate() {
        let left = if i >= bpp { row[i - bpp] } else { 0 };
        let up = previous[i];
        let up_left = if i >= bpp { previous[i - bpp] } else { 0 };
        let predictor = match kind {
            0 => 0,
            1 => left,
            2 => up,
            3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
            4 => paeth(left, up, up_left),
            _ => unreachable!("filter kind is 0..4"),
        };
        out.push(value.wrapping_sub(predictor));
    }
    out
}

fn fixture_pixel(x: usize, y: usize) -> [u8; 4] {
    [
        (x * 17 + y * 29) as u8,
        (x * 7 + y * 31) as u8,
        (x * 53 + y * 11) as u8,
        (x * 3 + y * 5) as u8,
    ]
}

fn source_row(width: usize, y: usize, bpp: usize) -> Vec<u8> {
    let mut row = Vec::with_capacity(width * bpp);
    for x in 0..width {
        let p = fixture_pixel(x, y);
        match bpp {
            1 => row.push(p[0]),
            3 => row.extend_from_slice(&[p[0], p[1], p[2]]),
            4 => row.extend_from_slice(&p),
            _ => unreachable!("channel count is 1, 3 or 4"),
        }
    }
    row
}

fn expected_rgba(rows: &[Vec<u8>], bpp: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        for px in row.chunks(bpp) {
            match bpp {
                1 => out.extend_from_slice(&[px[0], px[0], px[0], 255]),
                3 => out.extend_from_slice(&[px[0], px[1], px[2], 255]),
                4 => out.extend_from_slice(px),
                _ => unreachable!("channel count is 1, 3 or 4"),
            }
        }
    }
    out
}

fn filtered_rows(rows: &[Vec<u8>], bpp: usize, filter: impl Fn(usize) -> u8) -> Vec<u8> {
    let mut raw = Vec::new();
    let mut previous = vec![0u8; rows.first().map_or(0, Vec::len)];
    for (y, row) in rows.iter().enumerate() {
        raw.extend_from_slice(&filter_row(filter(y), row, &previous, bpp));
        previous = row.clone();
    }
    raw
}

fn base64_decode(input: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1u32 << bits) - 1;
        }
    }
    out
}

fn first_idat(png: &[u8]) -> &[u8] {
    let mut pos = 8usize;
    loop {
        let len = be32(&png[pos..pos + 4]) as usize;
        let kind = &png[pos + 4..pos + 8];
        if kind == &b"IDAT"[..] {
            return &png[pos + 8..pos + 8 + len];
        }
        pos += 12 + len;
    }
}

const DYNAMIC_PNG: &str = r#"
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
3aSiAAAAAElFTkSuQmCC
"#;

fn image_ir(src: &str, fit: &str, width: i64, height: i64) -> String {
    json!({
        "ir": "0.1", "app": "ImageProof", "state": [],
        "tree": {
            "id": 0, "kind": "Column",
            "props": { "padding": 0, "align": "start", "color": "#ffffff" },
            "children": [{
                "id": 1, "kind": "Image",
                "props": {
                    "src": src, "fit": fit, "width": width, "height": height,
                    "color": "#000000", "border": "#000000"
                },
                "children": [], "events": {}
            }],
            "events": {}
        },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string()
}

fn image_ir_default_fit(src: &str) -> String {
    let mut value: Value =
        serde_json::from_str(&image_ir(src, "contain", 8, 8)).expect("image IR is JSON");
    value["tree"]["children"][0]["props"]
        .as_object_mut()
        .expect("image props")
        .remove("fit");
    value.to_string()
}

fn text_ir(
    value: &str,
    width: i64,
    height: i64,
    align: Option<&str>,
    max_lines: Option<usize>,
) -> String {
    let mut props = Map::new();
    props.insert("value".into(), json!(value));
    props.insert("width".into(), json!(width));
    props.insert("height".into(), json!(height));
    props.insert("size".into(), json!(16));
    props.insert("color".into(), json!("#000000"));
    if let Some(align) = align {
        props.insert("align".into(), json!(align));
    }
    if let Some(max_lines) = max_lines {
        props.insert("maxLines".into(), json!(max_lines));
    }
    json!({
        "ir": "0.1", "app": "TextProof", "state": [],
        "tree": {
            "id": 0, "kind": "Column",
            "props": { "padding": 0, "align": "start", "color": "#ffffff" },
            "children": [{
                "id": 1, "kind": "Text", "props": props,
                "children": [], "events": {}
            }],
            "events": {}
        },
        "theme": { "mode": "light", "accent": "#097FEB", "radius": 0 }
    })
    .to_string()
}

fn image_node(ir: &Ir, state: &StateView, w: u32, h: u32) -> LaidOut {
    layout(ir, state, w, h)
        .into_iter()
        .find(|n| n.kind == Kind::Image)
        .expect("an image node is laid out")
}

fn text_node(ir: &Ir, state: &StateView, w: u32, h: u32) -> LaidOut {
    layout(ir, state, w, h)
        .into_iter()
        .find(|n| n.kind == Kind::Text)
        .expect("a text node is laid out")
}

fn pixel(pixels: &[u8], width: u32, x: i64, y: i64) -> [u8; 4] {
    let i = ((y as usize * width as usize) + x as usize) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn dark_ink_min_x(pixels: &[u8], width: u32, height: u32) -> i64 {
    let mut min = i64::MAX;
    for y in 0..height {
        for x in 0..width {
            let p = pixel(pixels, width, x as i64, y as i64);
            if p[0] < 128 && p[1] < 128 && p[2] < 128 {
                min = min.min(x as i64);
            }
        }
    }
    assert!(min != i64::MAX, "text must paint dark ink");
    min
}

fn assert_no_new_props(value: &Value) {
    if let Some(node) = value.as_object() {
        if let Some(props) = node.get("props").and_then(Value::as_object) {
            for key in ["src", "fit", "maxLines"] {
                assert!(!props.contains_key(key), "unexpected opt-in prop {key}");
            }
            if node.get("kind").and_then(Value::as_str) == Some("Text") {
                assert!(!props.contains_key("align"), "Text carries a local align");
            }
        }
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            for child in children {
                assert_no_new_props(child);
            }
        }
    }
}

#[test]
fn png_filters_and_color_models_match_independently_computed_pixels() {
    let _guard = cache_guard();
    image::clear_cache();
    let dir = temp_dir("filters");
    for (color_type, bpp, mode) in [
        (6usize, 4usize, BlockMode::Stored),
        (2, 3, BlockMode::Fixed),
        (0, 1, BlockMode::Stored),
    ] {
        let width = 3usize;
        let rows: Vec<Vec<u8>> = (0..5).map(|y| source_row(width, y, bpp)).collect();
        let expected = expected_rgba(&rows, bpp);
        let raw = filtered_rows(&rows, bpp, |y| y as u8);
        let png = make_png(
            width as u32,
            rows.len() as u32,
            color_type as u8,
            &raw,
            mode,
        );
        let path = dir.join(format!("filters-{color_type}.png"));
        fs::write(&path, &png).expect("write generated PNG");
        let direct = image::decode(&png).expect("decode generated PNG");
        assert_eq!(direct.width, width);
        assert_eq!(direct.height, rows.len());
        assert_eq!(direct.pixels, expected, "color type {color_type}");
        let cached = image::cached(path.to_str().unwrap()).expect("load generated PNG");
        assert_eq!(cached.as_ref(), &direct);
    }
}

#[test]
fn stored_and_fixed_huffman_pngs_decode() {
    let dir = temp_dir("deflate");
    let rows: Vec<Vec<u8>> = (0..2).map(|y| source_row(5, y, 4)).collect();
    let expected = expected_rgba(&rows, 4);
    let raw = filtered_rows(&rows, 4, |_| 0);
    for (name, mode) in [("stored", BlockMode::Stored), ("fixed", BlockMode::Fixed)] {
        let png = make_png(5, 2, 6, &raw, mode);
        let path = dir.join(format!("{name}.png"));
        fs::write(&path, &png).expect("write generated PNG");
        let decoded = image::decode(&png).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(decoded.pixels, expected, "{name} Huffman block");
    }
}

#[test]
fn dynamic_huffman_fixture_decodes_all_five_filters() {
    let _guard = cache_guard();
    image::clear_cache();
    let png = base64_decode(DYNAMIC_PNG);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let idat = first_idat(&png);
    assert_eq!(
        (idat[2] >> 1) & 3,
        2,
        "fixture must use a dynamic Huffman block"
    );
    let dir = temp_dir("dynamic");
    let path = dir.join("dynamic.png");
    fs::write(&path, &png).expect("write checked-in PNG fixture");
    let decoded = image::cached(path.to_str().unwrap()).expect("decode dynamic fixture");
    assert_eq!((decoded.width, decoded.height), (64, 32));
    let mut expected = Vec::with_capacity(64 * 32 * 4);
    for y in 0..32 {
        for x in 0..64 {
            expected.extend_from_slice(&fixture_pixel(x, y));
        }
    }
    assert_eq!(decoded.pixels, expected);
}

#[test]
fn malformed_truncated_and_oversized_pngs_are_rejected() {
    let row = source_row(1, 0, 4);
    let raw = filter_row(0, &row, &[0; 4], 4);
    let valid = make_png(1, 1, 6, &raw, BlockMode::Stored);

    let mut bad_signature = valid.clone();
    bad_signature[0] ^= 1;
    assert!(image::decode(&bad_signature)
        .unwrap_err()
        .contains("signature"));
    assert!(image::decode(&valid[..valid.len() - 3])
        .unwrap_err()
        .contains("truncated"));

    let mut bad_crc = valid.clone();
    bad_crc[16] ^= 1;
    assert!(image::decode(&bad_crc).unwrap_err().contains("CRC-32"));

    let oversized = {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend_from_slice(&png_chunk(b"IHDR", &ihdr(4097, 1, 8, 6, 0)));
        out.extend_from_slice(&png_chunk(b"IDAT", &zlib_stream(&[], BlockMode::Stored)));
        out.extend_from_slice(&png_chunk(b"IEND", &[]));
        out
    };
    assert!(image::decode(&oversized)
        .unwrap_err()
        .contains("dimensions"));

    let bad_filter = make_png(1, 1, 6, &[5, 1, 2, 3, 4], BlockMode::Stored);
    assert!(image::decode(&bad_filter)
        .unwrap_err()
        .contains("filter type 5"));

    let mut bad_zlib = zlib_stream(&raw, BlockMode::Stored);
    let last = bad_zlib.len() - 1;
    bad_zlib[last] ^= 1;
    let bad_adler = {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend_from_slice(&png_chunk(b"IHDR", &ihdr(1, 1, 8, 6, 0)));
        out.extend_from_slice(&png_chunk(b"IDAT", &bad_zlib));
        out.extend_from_slice(&png_chunk(b"IEND", &[]));
        out
    };
    assert!(image::decode(&bad_adler).unwrap_err().contains("Adler-32"));

    for header in [
        ihdr(1, 1, 16, 6, 0),
        ihdr(1, 1, 8, 3, 0),
        ihdr(1, 1, 8, 6, 1),
    ] {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend_from_slice(&png_chunk(b"IHDR", &header));
        out.extend_from_slice(&png_chunk(b"IDAT", &zlib_stream(&raw, BlockMode::Stored)));
        out.extend_from_slice(&png_chunk(b"IEND", &[]));
        assert!(
            image::decode(&out).is_err(),
            "unsupported IHDR must be rejected"
        );
    }
}

#[test]
fn cache_counts_hits_misses_and_evictions() {
    let _guard = cache_guard();
    image::clear_cache();
    let dir = temp_dir("cache");
    let row = source_row(1, 0, 4);
    let raw = filter_row(0, &row, &[0; 4], 4);
    let png = make_png(1, 1, 6, &raw, BlockMode::Stored);
    let mut paths = Vec::new();
    for i in 0..=CACHE_CAPACITY {
        let path = dir.join(format!("cached-{i}.png"));
        fs::write(&path, &png).expect("write PNG fixture");
        paths.push(path);
    }
    for path in &paths {
        assert!(image::cached(path.to_str().unwrap()).is_some());
    }
    let mut stats = image::cache_stats();
    assert_eq!(stats.misses, (CACHE_CAPACITY + 1) as u64);
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.entries, CACHE_CAPACITY);
    assert_eq!(stats.evictions, 1);
    assert!(image::cached(paths.last().unwrap().to_str().unwrap()).is_some());
    assert_eq!(image::cache_stats().hits, 1);
    assert!(image::cached(paths[0].to_str().unwrap()).is_some());
    stats = image::cache_stats();
    assert_eq!(stats.misses, (CACHE_CAPACITY + 2) as u64);
    assert_eq!(stats.entries, CACHE_CAPACITY);
    assert_eq!(stats.evictions, 2);
}

#[test]
fn relative_paths_resolve_against_the_process_cwd() {
    let cwd = std::env::current_dir().expect("read process CWD");
    assert_eq!(
        image::resolve("assets/logo.png"),
        cwd.join("assets/logo.png")
    );
    assert_eq!(image::resolve("a/../logo.png"), cwd.join("logo.png"));
    let absolute = Path::new("/tmp/opurom-render-logo.png");
    assert_eq!(image::resolve(absolute.to_str().unwrap()), absolute);
}

#[test]
fn image_nodes_load_lazily_and_missing_resources_are_deterministic() {
    let _guard = cache_guard();
    let dir = temp_dir("draw");
    let row = source_row(2, 0, 4);
    let raw = filter_row(0, &row, &[0; 8], 4);
    let png = make_png(2, 1, 6, &raw, BlockMode::Stored);
    let valid_path = dir.join("valid.png");
    fs::write(&valid_path, &png).expect("write PNG fixture");
    let valid_ir = image_ir(valid_path.to_str().unwrap(), "fill", 8, 8);
    let ir = parse_ir(&valid_ir).expect("parse image IR");
    let state = StateView::from_ir(&ir);
    let laid = image_node(&ir, &state, 32, 16);
    assert_eq!(
        laid.image_src.as_deref(),
        Some(valid_path.to_str().unwrap())
    );
    assert_eq!(laid.image_fit, ImageFit::Fill);

    image::clear_cache();
    let first = Renderer::new(32, 16).pixels(&ir, &state);
    assert_eq!(image::cache_stats().hits, 0);
    assert_eq!(image::cache_stats().misses, 1);
    let second = Renderer::new(32, 16).pixels(&ir, &state);
    assert_eq!(first, second, "cached image frames are identical");
    assert_eq!(image::cache_stats().hits, 1);
    assert_eq!(image::cache_stats().misses, 1);

    let missing = dir.join("definitely-missing.png");
    let missing_ir = image_ir(missing.to_str().unwrap(), "fill", 8, 8);
    let missing_parsed = parse_ir(&missing_ir).expect("parse missing image IR");
    let missing_state = StateView::from_ir(&missing_parsed);
    image::clear_cache();
    let placeholder1 = Renderer::new(32, 16).pixels(&missing_parsed, &missing_state);
    let placeholder2 = Renderer::new(32, 16).pixels(&missing_parsed, &missing_state);
    assert_eq!(placeholder1, placeholder2, "placeholder is deterministic");
    assert_eq!(image::cache_stats().hits, 1);
    assert_eq!(image::cache_stats().misses, 1);
    let rect = image_node(&missing_parsed, &missing_state, 32, 16).rect;
    let cx = rect.x + 1 + (rect.w - 2) / 2;
    let cy = rect.y + 1 + (rect.h - 2) / 2;
    assert_eq!(
        pixel(&placeholder1, 32, cx, cy),
        [0xf1, 0xf3, 0xf5, 255],
        "missing resource paints a diagonal cross"
    );

    let invalid_path = dir.join("invalid.png");
    fs::write(&invalid_path, b"not a png").expect("write invalid resource");
    let invalid_ir = image_ir(invalid_path.to_str().unwrap(), "fill", 8, 8);
    let invalid_parsed = parse_ir(&invalid_ir).expect("parse invalid image IR");
    let invalid_state = StateView::from_ir(&invalid_parsed);
    image::clear_cache();
    let invalid = Renderer::new(32, 16).pixels(&invalid_parsed, &invalid_state);
    assert_eq!(
        invalid,
        Renderer::new(32, 16).pixels(&invalid_parsed, &invalid_state)
    );
    assert_eq!(
        pixel(&invalid, 32, cx, cy),
        [0xf1, 0xf3, 0xf5, 255],
        "invalid resource paints the same cross"
    );
}

#[test]
fn fit_rect_uses_integer_contain_cover_and_fill_math() {
    let dst = Rect {
        x: 0,
        y: 0,
        w: 8,
        h: 8,
    };
    assert_eq!(
        image::fit_rect(4, 2, dst, ImageFit::Fill),
        Rect {
            x: 0,
            y: 0,
            w: 8,
            h: 8
        }
    );
    assert_eq!(
        image::fit_rect(4, 2, dst, ImageFit::Contain),
        Rect {
            x: 0,
            y: 2,
            w: 8,
            h: 4
        }
    );
    assert_eq!(
        image::fit_rect(4, 2, dst, ImageFit::Cover),
        Rect {
            x: -4,
            y: 0,
            w: 16,
            h: 8
        }
    );
    assert_eq!(
        image::fit_rect(2, 4, dst, ImageFit::Contain),
        Rect {
            x: 2,
            y: 0,
            w: 4,
            h: 8
        }
    );
    assert_eq!(
        image::fit_rect(2, 4, dst, ImageFit::Cover),
        Rect {
            x: 0,
            y: -4,
            w: 8,
            h: 16
        }
    );
}

#[test]
fn default_image_fit_is_contain() {
    let _guard = cache_guard();
    image::clear_cache();
    let dir = temp_dir("default-fit");
    let row = [255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    let raw = filter_row(0, &row, &[0; 12], 3);
    let png = make_png(4, 1, 2, &raw, BlockMode::Stored);
    let path = dir.join("wide.png");
    fs::write(&path, &png).expect("write PNG fixture");

    let explicit_source = image_ir(path.to_str().unwrap(), "contain", 8, 8);
    let default_source = image_ir_default_fit(path.to_str().unwrap());
    let explicit = parse_ir(&explicit_source).expect("parse explicit fit IR");
    let default = parse_ir(&default_source).expect("parse default fit IR");
    let explicit_state = StateView::from_ir(&explicit);
    let default_state = StateView::from_ir(&default);
    assert_eq!(
        image_node(&default, &default_state, 32, 16).image_fit,
        ImageFit::Contain
    );
    assert_eq!(
        Renderer::new(32, 16).pixels(&default, &default_state),
        Renderer::new(32, 16).pixels(&explicit, &explicit_state),
        "the default must preserve the source aspect ratio"
    );
}

#[test]
fn fit_modes_scale_and_crop_with_nearest_neighbour_sampling() {
    let _guard = cache_guard();
    image::clear_cache();
    let dir = temp_dir("fit");
    let colors: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
    let mut row = Vec::new();
    for color in colors {
        row.extend_from_slice(&color);
    }
    let raw = filter_row(0, &row, &[0; 12], 3);
    let png = make_png(4, 1, 2, &raw, BlockMode::Stored);
    let path = dir.join("four-colors.png");
    fs::write(&path, &png).expect("write four-colour PNG");

    let render = |fit: &str| {
        let ir_source = image_ir(path.to_str().unwrap(), fit, 8, 8);
        let ir = parse_ir(&ir_source).expect("parse fit IR");
        let state = StateView::from_ir(&ir);
        let laid = image_node(&ir, &state, 32, 16);
        assert_eq!(
            laid.image_fit,
            match fit {
                "contain" => ImageFit::Contain,
                "cover" => ImageFit::Cover,
                _ => ImageFit::Fill,
            }
        );
        Renderer::new(32, 16).pixels(&ir, &state)
    };
    let fill = render("fill");
    let contain = render("contain");
    let cover = render("cover");
    let ir = parse_ir(&image_ir(path.to_str().unwrap(), "fill", 8, 8)).unwrap();
    let state = StateView::from_ir(&ir);
    let laid = image_node(&ir, &state, 32, 16);
    let x = laid.rect.x + laid.border_w;
    let y = laid.rect.y + laid.border_w;
    assert_eq!(pixel(&fill, 32, x, y + 3), [255, 0, 0, 255]);
    assert_eq!(pixel(&fill, 32, x + 2, y + 3), [0, 255, 0, 255]);
    assert_eq!(
        pixel(&contain, 32, x, y),
        [0, 0, 0, 255],
        "contain letterboxes"
    );
    assert_eq!(pixel(&contain, 32, x, y + 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&cover, 32, x, y + 3), [0, 255, 0, 255], "cover crops");
    assert_eq!(pixel(&cover, 32, x + 3, y + 3), [0, 0, 255, 255]);
}

#[test]
fn text_align_offsets_and_max_lines_are_opt_in() {
    let cases = [
        ("left", Align::Start),
        ("center", Align::Center),
        ("right", Align::End),
    ];
    let mut offsets = Vec::new();
    for (name, expected_align) in cases {
        let source = text_ir("OK", 60, 20, Some(name), None);
        let ir = parse_ir(&source).expect("parse aligned text IR");
        let state = StateView::from_ir(&ir);
        let node = text_node(&ir, &state, 96, 40);
        assert_eq!(node.text_align, expected_align);
        let scale = node.text_scale.clamp(1, 8);
        let tw = 2i64 * font::advance(scale);
        let origin = match expected_align {
            Align::Start => node.rect.x + 2,
            Align::Center => node.rect.x + ((node.rect.w - tw) / 2).max(2),
            Align::End => node.rect.x + (node.rect.w - tw - 2).max(2),
        };
        let pixels = Renderer::new(96, 40).pixels(&ir, &state);
        offsets.push(dark_ink_min_x(&pixels, 96, 40) - origin);
    }
    assert_eq!(
        offsets[0], offsets[1],
        "same glyph starts at the same relative offset"
    );
    assert_eq!(
        offsets[1], offsets[2],
        "same glyph starts at the same relative offset"
    );

    let plain = text_ir("Alpha Beta Gamma Delta Epsilon", 80, 56, None, None);
    let capped = text_ir("Alpha Beta Gamma Delta Epsilon", 80, 56, None, Some(1));
    let plain_ir = parse_ir(&plain).expect("parse plain text IR");
    let capped_ir = parse_ir(&capped).expect("parse maxLines text IR");
    let capped_state = StateView::from_ir(&capped_ir);
    assert_eq!(
        text_node(&capped_ir, &capped_state, 96, 80).max_lines,
        Some(1)
    );
    let capped_pixels = Renderer::new(96, 80).pixels(&capped_ir, &capped_state);
    assert_eq!(
        capped_pixels,
        Renderer::new(96, 80).pixels(&capped_ir, &capped_state),
        "maxLines rendering is deterministic"
    );
    let plain_state = StateView::from_ir(&plain_ir);
    assert_ne!(
        capped_pixels,
        Renderer::new(96, 80).pixels(&plain_ir, &plain_state),
        "maxLines must change an overflowing text node"
    );
    assert!(parse_ir(&text_ir("A", 40, 20, Some("sideways"), None)).is_err());
    assert!(parse_ir(&image_ir("x.png", "squeeze", 8, 8)).is_err());
}

#[test]
fn canonical_counter_without_new_props_keeps_three_golden_digests() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../opurom-spec/counter.opuir.json"
    );
    let source = fs::read_to_string(path).expect("golden counter IR is present");
    let json: Value = serde_json::from_str(&source).expect("golden counter IR is JSON");
    assert_no_new_props(&json);
    let ir = parse_ir(&source).expect("parse golden counter IR");
    let mut state = StateView::from_ir(&ir);
    state.set("count", StateValue::Int(0));
    assert_eq!(
        digest_hex(&Renderer::new(320, 240).pixels(&ir, &state)),
        "8d45f412996a99c6"
    );
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &state)),
        "318beb1dd337bc4e"
    );
    state.set("count", StateValue::Int(7));
    assert_eq!(
        digest_hex(&Renderer::new(800, 600).pixels(&ir, &state)),
        "40073cb95c912d80"
    );
}
