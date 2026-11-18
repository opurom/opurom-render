//! Elevation shadow silhouette follows per-corner/elliptical radii.
//!
//! Flutter `box_decoration.dart:449-459` `_paintShadows` shifts/inflates the
//! rect then `_paintBox` (`:430-445`) reuses the same `borderRadius.toRRect`,
//! so a rounded node's shadow silhouette must match its corner radii. Default
//! scalar-radius nodes keep the byte-identical historical shadow path.

use opurom_render::{digest_hex, parse_ir, Command, Renderer, StateView};

fn ir(radius: i64, corners: &str) -> String {
    let corner = if corners.is_empty() {
        String::new()
    } else {
        format!(",\"cornerRadii\":{corners}")
    };
    format!(
        r##"{{
  "ir":"0.1.0","app":"ElevRadii","state":[],
  "tree":{{"id":0,"kind":"Column","props":{{"padding":20,"color":"#FFFFFF","border":"#FFFFFF"}},"events":{{}},
    "children":[{{"id":1,"kind":"Container","props":{{"width":80,"height":60,
      "color":"#FFFFFF","border":"#FFFFFF","radius":{radius}{corner},"elevation":3}},"events":{{}},"children":[]}}]}},
  "theme":{{"mode":"light","accent":"#097FEB","radius":4}}
}}"##
    )
}

fn pixels(ir_text: &str) -> Vec<u8> {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(160, 140).render_at(&ir, &state, 0).pixels
}

fn command(ir_text: &str) -> Command {
    let ir = parse_ir(ir_text).unwrap();
    let state = StateView::from_ir(&ir);
    Renderer::new(160, 140)
        .display_list(&ir, &state)
        .commands
        .into_iter()
        .find(|c| matches!(c, Command::EffectElevation { .. }))
        .expect("elevation command present")
}

#[test]
fn uniform_radius_omits_radii_and_keeps_scalar_bytes() {
    // Default scalar path: no `radii` field, scalar radius only.
    let scalar = ir(30, "");
    match command(&scalar) {
        Command::EffectElevation { radius, radii, .. } => {
            assert_eq!(radius, 30);
            assert!(radii.is_none(), "uniform node must omit radii");
        }
        _ => unreachable!(),
    }
    let json = Renderer::new(160, 140)
        .display_list(
            &parse_ir(&scalar).unwrap(),
            &StateView::from_ir(&parse_ir(&scalar).unwrap()),
        )
        .to_json()
        .unwrap();
    assert!(
        !json.contains("\"radii\""),
        "default command JSON is byte-stable"
    );
}

#[test]
fn per_corner_radii_are_carried_in_gpu_wire_order() {
    let c = r#"{"topLeft":[40,10],"topRight":[40,10],"bottomLeft":[40,10],"bottomRight":[40,10]}"#;
    match command(&ir(0, c)) {
        Command::EffectElevation { radii: Some(r), .. } => {
            let expected = [[40, 10], [40, 10], [40, 10], [40, 10]];
            assert_eq!(r, expected, "TL,TR,BL,BR order with [rx,ry] pairs");
        }
        _ => panic!("per-corner node must emit radii"),
    }
    let txt = ir(0, c);
    let parsed = parse_ir(&txt).unwrap();
    let json = Renderer::new(160, 140)
        .display_list(&parsed, &StateView::from_ir(&parsed))
        .to_json()
        .unwrap();
    assert!(json.contains("\"radii\":[[40,10],[40,10],[40,10],[40,10]]"));
}

#[test]
fn elliptical_shadow_silhouette_differs_from_circular() {
    // Same box, same elevation, same scalar radius 30; only the corner shape
    // differs (elliptical 40x10 vs circular 30). Holding the scalar constant
    // isolates the per-corner path: ignoring radii makes the two identical.
    let ell =
        r#"{"topLeft":[40,10],"topRight":[40,10],"bottomLeft":[40,10],"bottomRight":[40,10]}"#;
    let a = pixels(&ir(30, ell));
    let b = pixels(&ir(30, ""));
    assert_ne!(
        digest_hex(&a),
        digest_hex(&b),
        "elliptical corners must change the shadow silhouette"
    );
    let mut changed = 0usize;
    for i in (0..a.len()).step_by(4) {
        if a[i] != b[i] {
            changed += 1;
        }
    }
    assert!(
        changed > 100,
        "shadow silhouette change must be visible, got {changed} px"
    );
}

#[test]
fn asymmetric_corners_cast_asymmetric_shadow() {
    // Round only the top-left corner; the shadow band under that corner must
    // differ from the band under the square bottom-right corner.
    let tl = r#"{"topLeft":[40,10]}"#;
    let px = pixels(&ir(0, tl));
    let at = |x: usize, y: usize| px[(y * 160 + x) * 4];
    // Top edge band above the TL corner vs above the BR corner.
    let left_band = at(24, 16);
    let right_band = at(96, 16);
    assert_ne!(
        left_band, right_band,
        "one rounded corner must break shadow symmetry"
    );
}

#[test]
fn radii_validation_rejects_out_of_range() {
    // Construct a command with an oversized radius and confirm validation.
    // GPU_RADIUS_LIMIT is 1<<15; 1<<16 must be rejected.
    let parsed = parse_ir(&ir(30, "")).unwrap();
    let state = StateView::from_ir(&parsed);
    let mut dl = Renderer::new(160, 140).display_list(&parsed, &state);
    for c in dl.commands.iter_mut() {
        if let Command::EffectElevation { radii, .. } = c {
            *radii = Some([[1 << 16, 0], [0, 0], [0, 0], [0, 0]]);
        }
    }
    assert!(
        dl.to_json().is_err(),
        "out-of-range radii must fail validation"
    );
}
