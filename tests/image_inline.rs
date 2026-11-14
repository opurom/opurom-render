//! An inlined PNG data URI decodes through the same bounded cache as a file
//! path, so one IR carries its pixels to every target. Malformed payloads stay
//! a cached miss instead of a render error.

use opurom_render::image::{self, ImageFit};
use opurom_render::{parse_ir, Renderer, StateView};
use std::fs;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn png_data_uri() -> String {
    let bytes = fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../opurom-examples/assets/photo1.png"
    ))
    .expect("scene asset must exist");
    // Tiny inline base64 encoder keeps this test dependency-free.
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/png;base64,");
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn ir_with(src: &str) -> String {
    format!(
        r##"{{"ir":"0.1","app":"Inline","state":[],
        "theme":{{"mode":"light","accent":"#097FEB","radius":8}},
        "tree":{{"id":0,"kind":"Column","props":{{"padding":8}},"children":[
          {{"id":1,"kind":"Image","props":{{"src":{src:?},"width":120,"height":90,"fit":"contain"}},"children":[],"events":{{}}}}
        ],"events":{{}}}}}}"##
    )
}

#[test]
fn inlined_png_decodes_and_is_cached() {
    let _g = LOCK.lock().unwrap();
    image::clear_cache();
    let uri = png_data_uri();
    let loaded = image::cached(&uri);
    assert!(loaded.is_some(), "a valid inlined PNG must decode");
    let img = loaded.unwrap();
    assert_eq!((img.width, img.height), (320, 240));
    // Second lookup is a cache hit, no re-decode.
    assert!(image::cached(&uri).is_some());
    assert!(image::cache_stats().hits >= 1);
    assert_eq!(ImageFit::default(), ImageFit::Contain);
}

#[test]
fn malformed_inlined_png_is_a_cached_miss_not_an_error() {
    let _g = LOCK.lock().unwrap();
    image::clear_cache();
    let bad = "data:image/png;base64,not-a-real-png";
    assert!(image::cached(bad).is_none());
    let ir = parse_ir(&ir_with(bad)).unwrap();
    let state = StateView::from_ir(&ir);
    // Rendering still succeeds with the deterministic placeholder tile.
    let px = Renderer::new(160, 120).pixels(&ir, &state);
    assert_eq!(px.len(), 160 * 120 * 4);
}

#[test]
fn data_uri_key_does_not_collide_with_a_relative_path() {
    let _g = LOCK.lock().unwrap();
    image::clear_cache();
    let uri = png_data_uri();
    assert!(image::cached(&uri).is_some());
    // A filesystem path is resolved separately and must not reuse the URI hit.
    assert!(image::cached("assets/does-not-exist.png").is_none());
}
