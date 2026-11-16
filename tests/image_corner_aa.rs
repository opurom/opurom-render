//! Opt-in 4x coverage on rounded Image edges. The hard-clip default stays
//! byte-identical; edge_aa blends partially covered corner pixels.

use opurom_render::{digest_hex, parse_ir, Renderer, StateView};

const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn data_uri() -> String {
    let bytes = include_bytes!("fixtures/inline.png");
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

fn ir() -> String {
    format!(
        r##"{{"ir":"0.1.0","app":"ImageAA","state":[],
        "theme":{{"mode":"light","accent":"#097FEB","radius":4}},
        "tree":{{"id":0,"kind":"Image","props":{{"src":"{}","width":80,"height":80,
          "fit":"cover","radius":24}},"children":[],"events":{{}}}}}}"##,
        data_uri()
    )
}

fn render(aa: bool) -> Vec<u8> {
    let ir = parse_ir(&ir()).unwrap();
    let state = StateView::from_ir(&ir);
    let mut renderer = Renderer::new(80, 80);
    renderer.set_edge_aa(aa);
    renderer.render(&ir, &state).pixels
}

#[test]
fn default_image_corners_are_hard_clipped() {
    // The setter defaults off; a fresh renderer matches the plain path.
    let plain = render(false);
    let ir = parse_ir(&ir()).unwrap();
    let state = StateView::from_ir(&ir);
    assert_eq!(plain, Renderer::new(80, 80).render(&ir, &state).pixels);
}

#[test]
fn edge_coverage_blends_only_image_corner_pixels() {
    let plain = render(false);
    let aa = render(true);
    assert_ne!(plain, aa, "the opt-in flag must change the image corners");
    assert_eq!(plain.len(), aa.len());

    let mut differing = 0usize;
    let mut interior = 0usize;
    let mut blended = 0usize;
    for i in 0..plain.len() / 4 {
        let x = (i % 80) as i64;
        let y = (i / 80) as i64;
        assert_eq!(aa[i * 4 + 3], 255, "the frame stays opaque");
        if plain[i * 4..i * 4 + 4] == aa[i * 4..i * 4 + 4] {
            continue;
        }
        differing += 1;
        // The image fills 0..80 with radius 24: only the four corner squares
        // can be partially covered. The central band is untouched.
        if (24..56).contains(&x) && (24..56).contains(&y) {
            interior += 1;
        }
        // Stage is 247,248,250 and the image edge is darker: a strictly
        // intermediate channel proves coverage blending, not a binary flip.
        if (0..3).any(|k| {
            let off = plain[i * 4 + k] as i32;
            let on = aa[i * 4 + k] as i32;
            on > 216 && on < off && off > 230
        }) {
            blended += 1;
        }
    }
    assert!(differing > 0, "at least one corner pixel must change");
    assert_eq!(interior, 0, "interior pixels must not change");
    assert!(blended > 0, "corner pixels must carry partial coverage");
}

#[test]
fn image_corner_coverage_is_deterministic() {
    assert_eq!(render(true), render(true));
    assert_ne!(
        digest_hex(&render(true)),
        digest_hex(&render(false)),
        "digests must separate the two paths"
    );
}
