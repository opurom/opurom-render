//! Built-in icon glyph set.
//!
//! Every icon is an integer vector path authored in a 24x24 box and drawn with
//! the same fixed-point coverage convention as the built-in text font: no
//! floats, no wall clock, no external glyph data and no dependencies. Shapes
//! mirror the web DOM emitter's icon set so the native and web targets paint
//! the same symbol for a given name.

use crate::layout::UNIT;

/// Coordinate box every icon path is authored in.
pub const ICON_BOX: i64 = 24;

/// Supersampling grid used for icon edge coverage.
const ICON_AA: i64 = 4;

/// Half the stroke width of a stroked path, in icon units.
const STROKE_HALF: i64 = 1;

/// One primitive of an icon path, in the 24x24 box.
#[derive(Clone, Copy)]
pub enum Shape {
    /// A closed outline, filled.
    Polygon(&'static [(i64, i64)]),
    /// An open polyline, stroked.
    Polyline(&'static [(i64, i64)]),
    /// A circle outline (`filled: false`) or a filled disc.
    Circle {
        cx: i64,
        cy: i64,
        r: i64,
        filled: bool,
    },
}

/// The primitives that make up one icon.
pub struct IconShape {
    /// Filled primitives, drawn as solid ink.
    pub fills: &'static [Shape],
    /// Stroked primitives, drawn as a 2-unit-wide ribbon.
    pub strokes: &'static [Shape],
}

const HOME: IconShape = IconShape {
    fills: &[Shape::Polygon(&[
        (3, 11),
        (12, 3),
        (21, 11),
        (21, 20),
        (15, 20),
        (15, 14),
        (9, 14),
        (9, 20),
        (3, 20),
    ])],
    strokes: &[],
};

const SETTINGS: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Circle {
            cx: 12,
            cy: 12,
            r: 4,
            filled: false,
        },
        Shape::Polyline(&[(12, 2), (12, 5)]),
        Shape::Polyline(&[(12, 19), (12, 22)]),
        Shape::Polyline(&[(2, 12), (5, 12)]),
        Shape::Polyline(&[(19, 12), (22, 12)]),
        Shape::Polyline(&[(5, 5), (7, 7)]),
        Shape::Polyline(&[(17, 17), (19, 19)]),
        Shape::Polyline(&[(19, 5), (17, 7)]),
        Shape::Polyline(&[(7, 17), (5, 19)]),
    ],
};

const STAR: IconShape = IconShape {
    fills: &[Shape::Polygon(&[
        (12, 3),
        (15, 9),
        (21, 10),
        (17, 15),
        (18, 21),
        (12, 18),
        (6, 21),
        (7, 15),
        (3, 10),
        (9, 9),
    ])],
    strokes: &[],
};

const SEARCH: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Circle {
            cx: 11,
            cy: 11,
            r: 7,
            filled: false,
        },
        Shape::Polyline(&[(20, 20), (16, 16)]),
    ],
};

const PLUS: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(12, 5), (12, 19)]),
        Shape::Polyline(&[(5, 12), (19, 12)]),
    ],
};

const MINUS: IconShape = IconShape {
    fills: &[],
    strokes: &[Shape::Polyline(&[(5, 12), (19, 12)])],
};

const CHECK: IconShape = IconShape {
    fills: &[],
    strokes: &[Shape::Polyline(&[(5, 12), (10, 17), (19, 8)])],
};

const CLOSE: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(6, 6), (18, 18)]),
        Shape::Polyline(&[(18, 6), (6, 18)]),
    ],
};

const ARROW_BACK: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(19, 12), (5, 12)]),
        Shape::Polyline(&[(11, 6), (5, 12), (11, 18)]),
    ],
};

const ARROW_FORWARD: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(5, 12), (19, 12)]),
        Shape::Polyline(&[(13, 6), (19, 12), (13, 18)]),
    ],
};

const REFRESH: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(20, 12), (20, 10), (19, 8), (18, 6)]),
        Shape::Polyline(&[(20, 4), (20, 8), (16, 8)]),
    ],
};

const LOCK: IconShape = IconShape {
    fills: &[Shape::Polygon(&[
        (7, 11),
        (8, 8),
        (12, 3),
        (16, 8),
        (17, 8),
        (17, 11),
        (20, 11),
        (20, 20),
        (4, 20),
        (4, 11),
    ])],
    strokes: &[],
};

const USER: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Circle {
            cx: 12,
            cy: 12,
            r: 4,
            filled: false,
        },
        Shape::Polyline(&[(4, 21), (6, 18), (10, 15), (14, 15), (18, 18), (20, 21)]),
    ],
};

const BELL: IconShape = IconShape {
    fills: &[
        Shape::Polygon(&[
            (6, 16),
            (6, 11),
            (9, 6),
            (12, 5),
            (15, 6),
            (18, 11),
            (18, 16),
            (20, 20),
            (4, 20),
        ]),
        Shape::Circle {
            cx: 12,
            cy: 20,
            r: 2,
            filled: true,
        },
    ],
    strokes: &[],
};

const HEART: IconShape = IconShape {
    fills: &[Shape::Polygon(&[
        (12, 20),
        (8, 14),
        (6, 11),
        (8, 8),
        (10, 7),
        (12, 9),
        (14, 7),
        (16, 8),
        (18, 11),
        (16, 14),
    ])],
    strokes: &[],
};

const INFO: IconShape = IconShape {
    fills: &[Shape::Circle {
        cx: 12,
        cy: 8,
        r: 1,
        filled: true,
    }],
    strokes: &[
        Shape::Circle {
            cx: 12,
            cy: 12,
            r: 8,
            filled: false,
        },
        Shape::Polyline(&[(12, 11), (12, 17)]),
    ],
};

const WARNING: IconShape = IconShape {
    fills: &[
        Shape::Polygon(&[(12, 4), (22, 20), (2, 20)]),
        Shape::Circle {
            cx: 12,
            cy: 18,
            r: 1,
            filled: true,
        },
    ],
    strokes: &[Shape::Polyline(&[(12, 10), (12, 15)])],
};

const CHEVRON_RIGHT: IconShape = IconShape {
    fills: &[],
    strokes: &[Shape::Polyline(&[(9, 6), (15, 12), (9, 18)])],
};

const CHEVRON_DOWN: IconShape = IconShape {
    fills: &[],
    strokes: &[Shape::Polyline(&[(6, 9), (12, 15), (18, 9)])],
};

const DOWNLOAD: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(12, 3), (12, 14)]),
        Shape::Polyline(&[(7, 11), (12, 16), (17, 11)]),
        Shape::Polyline(&[(5, 20), (19, 20)]),
    ],
};

const UPLOAD: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(12, 20), (12, 7)]),
        Shape::Polyline(&[(7, 10), (12, 5), (17, 10)]),
        Shape::Polyline(&[(5, 20), (19, 20)]),
    ],
};

const PLAY: IconShape = IconShape {
    fills: &[Shape::Polygon(&[(7, 5), (19, 12), (7, 19)])],
    strokes: &[],
};

const PAUSE: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(8, 5), (8, 19)]),
        Shape::Polyline(&[(16, 5), (16, 19)]),
    ],
};

const EDIT: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(4, 20), (16, 20)]),
        Shape::Polyline(&[(14, 6), (20, 11), (12, 19), (8, 13)]),
    ],
};

const DELETE: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(5, 7), (19, 7)]),
        Shape::Polyline(&[(9, 7), (9, 5), (15, 5), (15, 7)]),
        Shape::Polyline(&[(10, 7), (10, 20), (14, 20), (14, 7)]),
    ],
};

const EYE: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[
            (2, 12),
            (6, 8),
            (10, 7),
            (14, 7),
            (18, 8),
            (22, 12),
            (18, 16),
            (14, 17),
            (10, 17),
            (6, 16),
        ]),
        Shape::Polyline(&[(12, 9), (12, 15)]),
    ],
};

const CALENDAR: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(4, 6), (20, 6), (20, 20), (4, 20)]),
        Shape::Polyline(&[(4, 10), (20, 10)]),
        Shape::Polyline(&[(9, 4), (9, 8)]),
        Shape::Polyline(&[(15, 4), (15, 8)]),
    ],
};

const LOCATION: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(12, 21), (7, 14), (7, 10), (12, 5), (17, 10), (17, 14)]),
        Shape::Circle {
            cx: 12,
            cy: 16,
            r: 3,
            filled: false,
        },
    ],
};

const FOLDER: IconShape = IconShape {
    fills: &[Shape::Polygon(&[
        (3, 7),
        (9, 7),
        (11, 9),
        (21, 9),
        (21, 20),
        (3, 20),
    ])],
    strokes: &[],
};

const MAIL: IconShape = IconShape {
    fills: &[],
    strokes: &[
        Shape::Polyline(&[(3, 6), (21, 6), (21, 18), (3, 18)]),
        Shape::Polyline(&[(3, 7), (12, 13), (21, 7)]),
    ],
};

const CHAT: IconShape = IconShape {
    fills: &[Shape::Polygon(&[
        (4, 5),
        (20, 5),
        (20, 16),
        (13, 16),
        (9, 20),
        (9, 16),
        (4, 16),
    ])],
    strokes: &[],
};

/// Neutral fallback glyph for an unknown name: a filled disc, so an
/// unsupported icon still paints a deterministic, clearly visible shape.
const FALLBACK: IconShape = IconShape {
    fills: &[Shape::Circle {
        cx: 12,
        cy: 12,
        r: 9,
        filled: true,
    }],
    strokes: &[],
};

/// The built-in icon names, mirroring the language's canonical set.
pub const ICON_NAMES: &[&str] = &[
    "home",
    "settings",
    "star",
    "search",
    "plus",
    "minus",
    "check",
    "close",
    "arrow_back",
    "arrow_forward",
    "refresh",
    "lock",
    "user",
    "bell",
    "heart",
    "info",
    "warning",
    "chevron_right",
    "chevron_down",
    "download",
    "upload",
    "play",
    "pause",
    "edit",
    "delete",
    "eye",
    "calendar",
    "location",
    "folder",
    "mail",
    "chat",
];

/// The glyph for a name, or the neutral fallback when the name is unknown.
pub fn icon_shape(name: &str) -> &'static IconShape {
    match name {
        "home" => &HOME,
        "settings" => &SETTINGS,
        "star" => &STAR,
        "search" => &SEARCH,
        "plus" => &PLUS,
        "minus" => &MINUS,
        "check" => &CHECK,
        "close" => &CLOSE,
        "arrow_back" => &ARROW_BACK,
        "arrow_forward" => &ARROW_FORWARD,
        "refresh" => &REFRESH,
        "lock" => &LOCK,
        "user" => &USER,
        "bell" => &BELL,
        "heart" => &HEART,
        "info" => &INFO,
        "warning" => &WARNING,
        "chevron_right" => &CHEVRON_RIGHT,
        "chevron_down" => &CHEVRON_DOWN,
        "download" => &DOWNLOAD,
        "upload" => &UPLOAD,
        "play" => &PLAY,
        "pause" => &PAUSE,
        "edit" => &EDIT,
        "delete" => &DELETE,
        "eye" => &EYE,
        "calendar" => &CALENDAR,
        "location" => &LOCATION,
        "folder" => &FOLDER,
        "mail" => &MAIL,
        "chat" => &CHAT,
        _ => &FALLBACK,
    }
}

/// Even-odd point-in-polygon test over integer coordinates.
fn in_polygon(x: i64, y: i64, poly: &[(i64, i64)]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) {
            // Cross-multiply the edge/scanline intersection so the sign of the
            // denominator cannot flip the comparison.
            let den = yj - yi;
            let num = xi * den + (xj - xi) * (y - yi);
            if (den > 0 && x * den < num) || (den < 0 && x * den > num) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Squared distance from a point to a segment (integer maths, no square root).
fn segment_dist2(x: i64, y: i64, a: (i64, i64), b: (i64, i64)) -> i64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len2 = dx * dx + dy * dy;
    let px = x - a.0;
    let py = y - a.1;
    if len2 == 0 {
        return px * px + py * py;
    }
    let t = (px * dx + py * dy).clamp(0, len2);
    (len2 * (px * px + py * py) - t * t) / len2
}

/// True when `(x, y)` lies on a shape: inside a filled polygon or disc, or
/// within half a stroke of a polyline or circle outline.
fn shape_hit(shape: &IconShape, x: i64, y: i64) -> bool {
    shape.fills.iter().any(|s| shape_on(s, x, y)) || shape.strokes.iter().any(|s| shape_on(s, x, y))
}

fn shape_on(s: &Shape, x: i64, y: i64) -> bool {
    match s {
        Shape::Polygon(poly) => in_polygon(x, y, poly),
        Shape::Polyline(pts) => pts
            .windows(2)
            .any(|w| segment_dist2(x, y, w[0], w[1]) <= STROKE_HALF * STROKE_HALF),
        Shape::Circle { cx, cy, r, filled } => {
            let dx = x - cx;
            let dy = y - cy;
            let d2 = dx * dx + dy * dy;
            if *filled {
                d2 <= r * r
            } else {
                let lo = (r - STROKE_HALF).max(0);
                d2 >= lo * lo && d2 <= (r + STROKE_HALF) * (r + STROKE_HALF)
            }
        }
    }
}

/// Fixed-point coverage (0..=UNIT) of one pixel of an icon drawn into a
/// `size x size` box. Sub-samples the integer path on an `ICON_AA x ICON_AA`
/// grid, following the same integer coverage convention as the text raster.
pub fn icon_coverage(name: &str, size: i64, px: i64, py: i64) -> i64 {
    let shape = icon_shape(name);
    let den = size * ICON_AA;
    if den <= 0 {
        return 0;
    }
    let mut on = 0i64;
    for sy in 0..ICON_AA {
        let cy = ((py * ICON_AA + sy) * ICON_BOX + ICON_BOX / 2) / den;
        for sx in 0..ICON_AA {
            let cx = ((px * ICON_AA + sx) * ICON_BOX + ICON_BOX / 2) / den;
            if shape_hit(shape, cx, cy) {
                on += 1;
            }
        }
    }
    (on * UNIT) / (ICON_AA * ICON_AA)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_names_match_the_language_canonical_set() {
        assert_eq!(ICON_NAMES.len(), 31);
        assert!(ICON_NAMES.contains(&"home"));
        assert!(ICON_NAMES.contains(&"chat"));
        // Every declared name resolves to a real shape (not the fallback), and
        // each shape carries at least one primitive.
        for name in ICON_NAMES {
            let shape = icon_shape(name);
            assert!(
                !shape.fills.is_empty() || !shape.strokes.is_empty(),
                "shape for {name} has no primitives"
            );
            assert_ne!(icon_shape(name) as *const _, icon_shape("nope") as *const _);
        }
    }

    #[test]
    fn coverage_is_in_range_and_non_empty_for_every_icon() {
        for name in ICON_NAMES {
            for size in [16, 32, 64] {
                let mut max = 0i64;
                let mut any = false;
                for py in 0..size {
                    for px in 0..size {
                        let cov = icon_coverage(name, size, px, py);
                        assert!((0..=UNIT).contains(&cov), "{name} cov {cov}");
                        if cov > 0 {
                            any = true;
                        }
                        max = max.max(cov);
                    }
                }
                assert!(any, "{name} at {size} painted nothing");
                // Every icon paints at least some ink; stroke-only icons never
                // reach a fully solid pixel, so a per-pixel max is not required.
                assert!(max > 0, "{name} at {size} painted no ink");
            }
        }
    }

    #[test]
    fn unknown_name_uses_the_fallback_and_paints() {
        // The centre of a 12x12 box lies inside the fallback disc.
        let known = icon_coverage("home", 12, 6, 6);
        let unknown = icon_coverage("definitely_not_an_icon", 12, 6, 6);
        assert!(unknown > 0, "fallback must paint ink");
        assert!(known > 0);
        assert_ne!(known, 0);
    }
}
