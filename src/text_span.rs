//! Deterministic text-span export for the system-font display layer
//! (SYSTEM_FONT_DISPLAY.md §2). Additive: no kernel font rasterization, no new
//! dependency, and no change to the default render path.

use crate::{Align, Ir, Kind, LaidOut, Rgba, StateView};

/// One laid-out text run for a host to paint with the system font. All values
/// are logical pixels and deterministic integers; there are no floats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSpan {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub text: String,
    pub color: Rgba,
    /// Resolved text scale (same unit as `LaidOut.text_scale`).
    pub size: i64,
    pub align: Align,
    /// Absolute glyph origin, or `None` when the text is centred in its box.
    pub origin: Option<(i64, i64)>,
    /// Exact typography for the system-font layer; none means the field is
    /// omitted from JSON so default spans stay byte-identical.
    pub weight: Option<String>,
    pub letter_spacing: Option<i64>,
    pub font_px: Option<i64>,
    /// Absolute baseline from vertical metrics, emitted only when activated.
    pub baseline: Option<i64>,
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Start => "start",
        Align::Center => "center",
        Align::End => "end",
    }
}

/// Collect every laid-out node carrying non-empty text, in document (layout)
/// order. Graphics-only nodes and icon glyphs are skipped: an icon's text is
/// its raster graphic, not display text for a system-font overlay.
pub fn text_spans(laid: &[LaidOut]) -> Vec<TextSpan> {
    laid.iter()
        .filter_map(|n| {
            // Icon glyphs are raster graphics, not display text: overlaying a
            // system-font TextView on an icon would corrupt its picture.
            if n.kind == Kind::Icon {
                return None;
            }
            let text = n.text.as_ref()?;
            if text.is_empty() {
                return None;
            }
            Some(TextSpan {
                x: n.rect.x,
                y: n.rect.y,
                w: n.rect.w,
                h: n.rect.h,
                text: text.clone(),
                color: n.text_color.unwrap_or(Rgba::rgb(0, 0, 0)),
                size: n.text_scale,
                align: n.text_align,
                origin: n.text_origin,
                weight: n.text_weight.clone(),
                letter_spacing: n.text_letter_spacing,
                font_px: n.text_font_px,
                baseline: n.text_baseline,
            })
        })
        .collect()
}

/// Serialize spans to a deterministic JSON array. Field order is fixed and
/// every number is an integer, so two runs over the same input are byte-equal.
/// Shape: `[{"x":..,"y":..,"w":..,"h":..,"text":"..","color":[r,g,b,a],
/// "size":..,"align":"start|center|end","origin":[x,y]|null, ...opt-in
/// "weight"/"letterSpacing"/"fontPx"/"baseline" when declared}, ...]`. The
/// opt-in keys appear only when active; default spans keep the exact pinned
/// bytes. Field order is fixed and every number is an integer.
pub fn text_spans_json(laid: &[LaidOut]) -> String {
    fn escape(text: &str, out: &mut String) {
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    use std::fmt::Write as _;
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c => out.push(c),
            }
        }
    }

    let spans = text_spans(laid);
    let mut out = String::from("[");
    for (i, s) in spans.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"x\":");
        out.push_str(&s.x.to_string());
        out.push_str(",\"y\":");
        out.push_str(&s.y.to_string());
        out.push_str(",\"w\":");
        out.push_str(&s.w.to_string());
        out.push_str(",\"h\":");
        out.push_str(&s.h.to_string());
        out.push_str(",\"text\":\"");
        escape(&s.text, &mut out);
        out.push_str("\",\"color\":[");
        out.push_str(&s.color.r.to_string());
        out.push(',');
        out.push_str(&s.color.g.to_string());
        out.push(',');
        out.push_str(&s.color.b.to_string());
        out.push(',');
        out.push_str(&s.color.a.to_string());
        out.push_str("],\"size\":");
        out.push_str(&s.size.to_string());
        out.push_str(",\"align\":\"");
        out.push_str(align_name(s.align));
        out.push_str("\",\"origin\":");
        match s.origin {
            Some((ox, oy)) => {
                out.push('[');
                out.push_str(&ox.to_string());
                out.push(',');
                out.push_str(&oy.to_string());
                out.push(']');
            }
            None => out.push_str("null"),
        }
        // Opt-in typography: each key is emitted only when explicitly declared,
        // so a default span's bytes are unchanged from the pinned golden.
        if let Some(w) = &s.weight {
            out.push_str(",\"weight\":\"");
            escape(w, &mut out);
            out.push('"');
        }
        if let Some(ls) = s.letter_spacing {
            out.push_str(",\"letterSpacing\":");
            out.push_str(&ls.to_string());
        }
        if let Some(fp) = s.font_px {
            out.push_str(",\"fontPx\":");
            out.push_str(&fp.to_string());
        }
        if let Some(bl) = s.baseline {
            out.push_str(",\"baseline\":");
            out.push_str(&bl.to_string());
        }
        out.push('}');
    }
    out.push(']');
    out
}

/// Convenience entry: lay out `(ir, state, width, height)` and return the span
/// JSON. Mirrors `layout()` so mobile hosts need only the IR and state.
pub fn text_spans_for(ir: &Ir, state: &StateView, width: u32, height: u32) -> String {
    text_spans_json(&crate::layout(ir, state, width, height))
}
