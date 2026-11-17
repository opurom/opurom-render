//! Clip-edge AA closure A — internal seam consistency, NOT Flutter parity.
//!
//! Scope: content clipped to a rounded container only. The hard cuts in
//! `set_quiet`, the effect mask and ripple clip are untouched; axis-aligned
//! ClipRect edges need no coverage. No byte-identical Flutter claim is made.

use opurom_render::{digest_hex, parse_ir, Renderer, StateView};

const IR: &str = r##"{
  "ir":"0.1.0","app":"ClipAA","state":[],
  "tree":{"id":0,"kind":"Column","props":{"padding":0,"align":"start"},
    "children":[{"id":1,"kind":"Container","props":{"width":40,"height":40,"radius":8,"color":"#112233"},
      "children":[{"id":2,"kind":"Container","props":{"width":40,"height":40,"color":"#FF0000"},
        "children":[],"events":{}}],"events":{}}],"events":{}},
  "theme":{"mode":"light","accent":"#097FEB","radius":4}
}"##;

fn pixels(edge_aa: bool) -> Vec<u8> {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(48, 48);
    renderer.set_edge_aa(edge_aa);
    renderer.render(&ir, &state).pixels
}

fn pixel(p: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * 48 + x) * 4;
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

#[test]
fn rounded_clip_content_uses_intermediate_coverage_when_opted_in() {
    let off = pixels(false);
    let on = pixels(true);
    assert_ne!(digest_hex(&off), digest_hex(&on));

    // The child red box reaches the parent rounded corner. With a binary clip
    // its corner pixel is pure red; with coverage it is a partial blend of the
    // red child and the underlying rounded container / stage.
    assert_eq!(pixel(&off, 3, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&on, 3, 1), [235, 130, 133, 255]);
    assert_eq!(pixel(&off, 2, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&on, 2, 2), [242, 73, 75, 255]);
    for (x, y) in [(3, 1), (2, 2)] {
        assert_ne!(pixel(&on, x, y)[0], 255, "not a hard red cut at {x},{y}");
        assert!(pixel(&on, x, y)[1] > 0, "intermediate coverage at {x},{y}");
        assert!(
            pixel(&on, x, y)[1] < 255,
            "intermediate coverage at {x},{y}"
        );
    }
    assert_eq!(digest_hex(&on), "9ca5593018668ebb");
}

#[test]
fn default_clip_edge_path_is_byte_identical() {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let mut plain = Renderer::new(48, 48);
    let mut setter = Renderer::new(48, 48);
    setter.set_edge_aa(false);
    assert_eq!(
        plain.render(&ir, &state).pixels,
        setter.render(&ir, &state).pixels
    );
}

#[test]
fn clip_edge_aa_does_not_change_capture_commands() {
    let ir = parse_ir(IR).unwrap();
    let state = StateView::from_ir(&ir);
    let plain = Renderer::new(48, 48);
    let mut aa = Renderer::new(48, 48);
    aa.set_edge_aa(true);
    assert_eq!(
        plain.display_list(&ir, &state).to_json().unwrap(),
        aa.display_list(&ir, &state).to_json().unwrap()
    );
}
