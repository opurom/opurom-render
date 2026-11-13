//! Deterministic SVG 1.1 vector backend: a second *software* backend that
//! snapshots the same `layout_at` boxes into vector elements. This is NOT a
//! GPU backend (no Metal/Vulkan/ANGLE), does no font shaping and decodes no
//! images: `<text>` uses the built-in 5x7 font metrics with a generic monospace
//! face, and box colours come straight from the layout palette.
//!
//! The output is byte-identical for identical IR + viewport + state + theme +
//! animation tick: every value is integer formatting over a deterministic
//! pre-order layout, with no time, randomness, host queries or external crate.

use crate::a11y::role_of;
use crate::ir::{Ir, Node, Role, StateView};
use crate::layout::{layout_at, LaidOut, Palette, SETTLED_MS};
use crate::{font, Align, Rgba};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn hex(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Deterministic 3-decimal rendering of a 0..1024 fixed-point opacity.
fn fixed3(v: i64) -> String {
    let m = (v.clamp(0, 1024) * 1000 + 512) / 1024;
    format!("{}.{:03}", m / 1000, m % 1000)
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Text placement from the built-in font metrics, mirroring the raster's
/// top-left glyph origin. Returns `(x, baseline_y, font_size_px)`.
fn text_pos(n: &LaidOut, text: &str) -> (i64, i64, i64) {
    let scale = n.text_scale.clamp(1, 8);
    let tw = (text.chars().count() as i64).saturating_mul(font::advance(scale));
    let th = font::GLYPH_H * scale;
    let pad = n.border_w.max(0) + 2;
    let (x0, y0) = match n.text_origin {
        Some((ox, oy)) => (ox, oy),
        None => {
            let x0 = match n.text_align {
                Align::Start => n.rect.x + pad,
                Align::Center => n.rect.x + ((n.rect.w - tw) / 2).max(pad),
                Align::End => n.rect.x + (n.rect.w - tw - pad).max(pad),
            };
            let y0 = n.rect.y + ((n.rect.h - th) / 2).max(pad);
            (x0, y0)
        }
    };
    (x0, y0 + th, th)
}

/// Every node in pre-order, including hidden ones (for `data-hidden` markers).
fn collect<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(n);
    for c in &n.children {
        collect(c, out);
    }
}

/// Deterministic SVG document at a given animation clock position.
pub fn svg_at(ir: &Ir, state: &StateView, width: u32, height: u32, elapsed_ms: i64) -> String {
    let pal = Palette::from_theme(&ir.theme);
    let laid = layout_at(ir, state, width, height, elapsed_ms);
    let by_id: BTreeMap<u32, &LaidOut> = laid.iter().map(|n| (n.id, n)).collect();
    let mut all: Vec<&Node> = Vec::new();
    collect(&ir.tree, &mut all);
    let roles: BTreeMap<u32, Role> = all.iter().copied().map(|n| (n.id, role_of(n))).collect();

    let mut defs = String::new();
    let mut body = String::new();
    let mut grad = 0usize;

    for n in &laid {
        let role = roles.get(&n.id).copied().unwrap_or(Role::None);
        let role_s = role.name();
        if n.fill.is_some() {
            let mut fill = hex(n.fill.unwrap());
            if let Some((a, b)) = n.effects.gradient {
                let id = format!("g{grad}");
                grad += 1;
                writeln!(
                    defs,
                    "  <linearGradient id=\"{id}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">"
                )
                .unwrap();
                writeln!(defs, "    <stop offset=\"0%\" stop-color=\"{}\"/>", hex(a)).unwrap();
                writeln!(
                    defs,
                    "    <stop offset=\"100%\" stop-color=\"{}\"/>",
                    hex(b)
                )
                .unwrap();
                writeln!(defs, "  </linearGradient>").unwrap();
                fill = format!("url(#{id})");
            }
            let rad = n.radius.max(0);
            let mut el = String::new();
            write!(
                el,
                "  <rect data-id=\"{}\" data-role=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\"",
                n.id,
                role_s,
                n.rect.x,
                n.rect.y,
                n.rect.w.max(0),
                n.rect.h.max(0),
                rad,
                rad,
                fill
            )
            .unwrap();
            if n.border_w > 0 {
                if let Some(b) = n.border {
                    write!(el, " stroke=\"{}\" stroke-width=\"{}\"", hex(b), n.border_w).unwrap();
                }
            }
            if n.effects.opacity < 1024 {
                write!(el, " opacity=\"{}\"", fixed3(n.effects.opacity)).unwrap();
            }
            el.push_str("/>\n");
            body.push_str(&el);
        }
        if let Some(text) = &n.text {
            if !text.is_empty() {
                let (x, y, size) = text_pos(n, text);
                let color = hex(n.text_color.unwrap_or(Rgba::rgb(0, 0, 0)));
                writeln!(
                    body,
                    "  <text data-id=\"{}\" data-role=\"{}\" x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"{}\" fill=\"{}\">{}</text>",
                    n.id,
                    role_s,
                    x,
                    y,
                    size,
                    color,
                    esc(text)
                )
                .unwrap();
            }
        }
    }

    // Hidden `when` nodes: their drawing is omitted; a marker records them.
    for n in all.iter().copied() {
        if !by_id.contains_key(&n.id) {
            let role = roles.get(&n.id).copied().unwrap_or(Role::None);
            writeln!(
                body,
                "  <g data-id=\"{}\" data-role=\"{}\" data-hidden=\"true\"/>",
                n.id,
                role.name()
            )
            .unwrap();
        }
    }

    let mut out = String::new();
    writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>").unwrap();
    writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">"
    )
    .unwrap();
    out.push_str("<defs>\n");
    out.push_str(&defs);
    out.push_str("</defs>\n");
    writeln!(
        out,
        "  <rect x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" fill=\"{}\"/>",
        hex(pal.bg)
    )
    .unwrap();
    out.push_str(&body);
    out.push_str("</svg>\n");
    out
}

/// Deterministic SVG document at the settled animation clock position.
pub fn svg(ir: &Ir, state: &StateView, width: u32, height: u32) -> String {
    svg_at(ir, state, width, height, SETTLED_MS)
}
