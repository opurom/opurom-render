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

/// One parsed subpath: an open integer polyline in the 24x24 icon box, stroked
/// with the same 2-unit ribbon as the built-in shapes. `closed` records a
/// trailing `Z` so a closed subpath joins its last point back to its first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubPath {
    pub points: Vec<(i64, i64)>,
    pub closed: bool,
}

/// A package icon resolved from the IR `icons` table: its name and every
/// subpath of every `d` string, parsed once to integer 24x24 geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconPaths {
    pub name: String,
    pub subpaths: Vec<SubPath>,
}

impl IconPaths {
    /// Parse an IR icon entry from its name and `d` strings. The viewBox is
    /// fixed at `0 0 24 24` by the package contract; a different box would scale
    /// the glyph at raster time, so non-24 boxes fall back to the built-in set.
    pub fn from_paths(name: &str, view_box: &str, ds: &[String]) -> Option<IconPaths> {
        if view_box.trim() != "0 0 24 24" {
            return None;
        }
        let mut subpaths = Vec::new();
        for d in ds {
            subpaths.extend(parse_path_d(d)?);
        }
        if subpaths.is_empty() {
            return None;
        }
        Some(IconPaths {
            name: name.to_string(),
            subpaths,
        })
    }
}

/// Full turn in the trig fixed-point scale (angle units of 1/360 turn mapped to
/// a 4096-step circle for exact integer lookup).
const TAU: i64 = 4096;
/// Trig scale: `cos`/`sin` values are scaled by this fixed point.
const TRIG: i64 = 128;

/// Cosine/sine of an angle measured in `TAU` units, from a 64-step quarter-wave
/// table (interpolated), returned scaled by `TRIG`.
fn cos_sin_from_turn(angle: i64) -> (i64, i64) {
    let a = angle.rem_euclid(TAU);
    (cos_lookup(a), sin_lookup(a))
}

/// Cosine from the table for `a` in `0..TAU`, scaled by `TRIG`.
fn cos_lookup(a: i64) -> i64 {
    trig_lookup(a, false)
}

/// Sine from the table for `a` in `0..TAU`, scaled by `TRIG`.
fn sin_lookup(a: i64) -> i64 {
    trig_lookup(a, true)
}

/// Shared quadrant-folded lookup. `want_sin` selects sine; cosine is sine
/// shifted a quarter turn. Folded to `0..=quarter` and mirrored, so only an
/// eighth-wave table is stored.
fn trig_lookup(a: i64, want_sin: bool) -> i64 {
    let quarter = TAU / 4;
    let a = a.rem_euclid(TAU);
    let shifted = if want_sin {
        a
    } else {
        (a + quarter).rem_euclid(TAU)
    };
    // Fold into the first quadrant with sign.
    let (mut x, mut neg) = (shifted, false);
    if x >= 2 * quarter {
        x -= 2 * quarter;
        neg = true;
    }
    if x > quarter {
        x = 2 * quarter - x;
    }
    // Eighth-wave table: sin(k * quarter / 8) for k in 0..=8.
    let idx = x * 8;
    let step = quarter;
    let k = idx / step;
    let frac = idx % step;
    let lo = SINE_TABLE[k as usize];
    let hi = SINE_TABLE[(k as usize + 1).min(8)];
    let value = lo + (hi - lo) * frac / step;
    if neg {
        -value
    } else {
        value
    }
}

/// `sin(k * pi/16) * TRIG`, k = 0..=8, rounded to integers.
const SINE_TABLE: [i64; 9] = [
    0, 25,  // sin(11.25deg) ~ 0.1951
    49,  // sin(22.5deg)  ~ 0.3827
    72,  // sin(33.75deg) ~ 0.5556
    90,  // sin(45deg)    ~ 0.7071
    106, // sin(56.25deg) ~ 0.8315
    118, // sin(67.5deg)  ~ 0.9239
    125, // sin(78.75deg) ~ 0.9808
    128, // sin(90deg)    = 1.0
];

/// Integer square root (floor) for a non-negative `n`.
fn isqrt(n: i64) -> i64 {
    let n = n.max(0);
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x.max(1)) / 2;
    }
    x
}

/// Approximate `atan2(y, x)` in `TAU` units (4096 per turn), integer only, from
/// the angle of the vector via a rational approximation then quadrant fixup.
fn atan2_approx(y: i64, x: i64) -> i64 {
    if x == 0 && y == 0 {
        return 0;
    }
    let ax = x.abs();
    let ay = y.abs();
    // angle in [0, quarter] using atan(s) ~ s*(pi/4) for the ratio s = ay/ax.
    let ratio = (ay * TRIG) / (ax + ay).max(1); // 0..TRIG
    let quarter = TAU / 4;
    let mut angle = ratio * quarter / TRIG;
    if ay > ax {
        angle = quarter - ((ax * TRIG) / (ax + ay).max(1)) * quarter / TRIG;
    }
    if x < 0 {
        angle = 2 * quarter - angle;
    }
    if y < 0 {
        angle = TAU - angle;
    }
    angle.rem_euclid(TAU)
}

/// Parse an SVG path `d` string into integer subpaths in the 24x24 box.
///
/// Supports M/m L/l H/h V/v C/c S/s Q/q T/t Z/z and downward-converts the
/// curves/arcs to line segments on a fixed subdivision (shortcut: A/a arcs and
/// cubic/quadratic curves are flattened to straight segments dense enough for a
/// 24-unit glyph; upgrade to exact arc/cubic rasterisation if a package ships
/// very large-radius curves). All math is integer fixed point scaled by
/// `PATH_FIX`, rounded back to whole icon units, so a path always parses to the
/// same integer points.
pub fn parse_path_d(d: &str) -> Option<Vec<SubPath>> {
    let tokens = tokenize_path(d)?;
    let mut sp = PathScanner {
        tokens: &tokens,
        pos: 0,
        cx: 0,
        cy: 0,
        start_x: 0,
        start_y: 0,
        prev_cmd: ' ',
        prev_c2x: 0,
        prev_c2y: 0,
        out: Vec::new(),
        cur: Vec::new(),
    };
    sp.run()?;
    Some(sp.out)
}

/// Fixed-point denominator for path arithmetic: coordinates are kept as
/// `PATH_FIX`-scaled integers and rounded to whole icon units when emitted.
const PATH_FIX: i64 = 64;
/// Straight segments per cubic/quadratic curve, and per arc quadrant.
const CURVE_STEPS: i64 = 8;

fn tokenize_path(d: &str) -> Option<Vec<PathTok>> {
    let bytes = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() || b == b',' {
            i += 1;
            continue;
        }
        if b.is_ascii_alphabetic() {
            out.push(PathTok::Cmd(b as char));
            i += 1;
            continue;
        }
        // Parse one number: sign, digits, optional fraction, optional exponent.
        let start = i;
        if b == b'+' || b == b'-' {
            i += 1;
        }
        let mut seen_digit = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            seen_digit = true;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
                seen_digit = true;
            }
        }
        if seen_digit && i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if !seen_digit {
            return None;
        }
        let text = std::str::from_utf8(&bytes[start..i]).ok()?;
        out.push(PathTok::Num(parse_fixed(text)?));
        continue;
    }
    Some(out)
}

/// Parse a decimal number string to a `PATH_FIX`-scaled integer.
fn parse_fixed(text: &str) -> Option<i64> {
    let mut value = 0i64;
    let mut frac = 0i64;
    let mut scale = PATH_FIX;
    let mut negative = false;
    let mut seen_dot = false;
    let mut seen_digit = false;
    for ch in text.chars() {
        match ch {
            '+' => {}
            '-' => negative = true,
            '.' => seen_dot = true,
            '0'..='9' => {
                seen_digit = true;
                let digit = ch as i64 - '0' as i64;
                if seen_dot {
                    scale /= 10;
                    if scale > 0 {
                        frac += digit * scale;
                    }
                } else {
                    value = value * 10 + digit;
                }
            }
            _ => return None,
        }
    }
    if !seen_digit {
        return None;
    }
    let fixed = value * PATH_FIX + frac;
    Some(if negative { -fixed } else { fixed })
}

enum PathTok {
    Cmd(char),
    Num(i64),
}

struct PathScanner<'a> {
    tokens: &'a [PathTok],
    pos: usize,
    cx: i64,
    cy: i64,
    start_x: i64,
    start_y: i64,
    prev_cmd: char,
    prev_c2x: i64,
    prev_c2y: i64,
    out: Vec<SubPath>,
    cur: Vec<(i64, i64)>,
}

impl<'a> PathScanner<'a> {
    fn run(&mut self) -> Option<()> {
        let mut cmd = ' ';
        while self.pos < self.tokens.len() {
            if let PathTok::Cmd(c) = self.tokens[self.pos] {
                cmd = c;
                self.pos += 1;
            } else if cmd == ' ' {
                return None; // a coordinate list must follow a command letter
            }
            match cmd {
                'M' | 'm' => {
                    let mut first = true;
                    loop {
                        let x = self.num()?;
                        let y = self.num()?;
                        let (px, py) = self.apply(cmd, x, y);
                        if first {
                            self.flush();
                            self.start_x = px;
                            self.start_y = py;
                            first = false;
                        }
                        self.cx = px;
                        self.cy = py;
                        self.cur.push((unit(px), unit(py)));
                        if !self.next_is_num() {
                            break;
                        }
                    }
                    self.prev_cmd = cmd;
                }
                'L' | 'l' | 'H' | 'h' | 'V' | 'v' => loop {
                    let (px, py) = self.line_point(cmd)?;
                    self.cx = px;
                    self.cy = py;
                    self.cur.push((unit(px), unit(py)));
                    self.prev_cmd = cmd;
                    if !self.next_is_num() {
                        break;
                    }
                },
                'C' | 'c' => loop {
                    let x1 = self.num()?;
                    let y1 = self.num()?;
                    let x2 = self.num()?;
                    let y2 = self.num()?;
                    let x = self.num()?;
                    let y = self.num()?;
                    let (p1x, p1y) = self.apply(cmd, x1, y1);
                    let (p2x, p2y) = self.apply(cmd, x2, y2);
                    let (ex, ey) = self.apply(cmd, x, y);
                    self.cubic(p1x, p1y, p2x, p2y, ex, ey);
                    self.prev_c2x = p2x;
                    self.prev_c2y = p2y;
                    self.prev_cmd = cmd;
                    if !self.next_is_num() {
                        break;
                    }
                },
                'S' | 's' => loop {
                    let x2 = self.num()?;
                    let y2 = self.num()?;
                    let x = self.num()?;
                    let y = self.num()?;
                    let (r1x, r1y) = self.reflect_control('C');
                    let (p2x, p2y) = self.apply(cmd, x2, y2);
                    let (ex, ey) = self.apply(cmd, x, y);
                    self.cubic(r1x, r1y, p2x, p2y, ex, ey);
                    self.prev_c2x = p2x;
                    self.prev_c2y = p2y;
                    self.prev_cmd = cmd;
                    if !self.next_is_num() {
                        break;
                    }
                },
                'Q' | 'q' => loop {
                    let x1 = self.num()?;
                    let y1 = self.num()?;
                    let x = self.num()?;
                    let y = self.num()?;
                    let (p1x, p1y) = self.apply(cmd, x1, y1);
                    let (ex, ey) = self.apply(cmd, x, y);
                    self.quad(p1x, p1y, ex, ey);
                    self.prev_c2x = p1x;
                    self.prev_c2y = p1y;
                    self.prev_cmd = cmd;
                    if !self.next_is_num() {
                        break;
                    }
                },
                'T' | 't' => loop {
                    let x = self.num()?;
                    let y = self.num()?;
                    let (qx, qy) = self.reflect_control('Q');
                    let (ex, ey) = self.apply(cmd, x, y);
                    self.quad(qx, qy, ex, ey);
                    self.prev_c2x = qx;
                    self.prev_c2y = qy;
                    self.prev_cmd = cmd;
                    if !self.next_is_num() {
                        break;
                    }
                },
                'A' | 'a' => loop {
                    let rx = self.num()?;
                    let ry = self.num()?;
                    let rot = self.num()? / PATH_FIX; // degrees
                    let large = self.num()? != 0;
                    let sweep = self.num()? != 0;
                    let x = self.num()?;
                    let y = self.num()?;
                    let (ex, ey) = self.apply(cmd, x, y);
                    self.arc(rx, ry, rot, large, sweep, ex, ey);
                    self.prev_cmd = cmd;
                    if !self.next_is_num() {
                        break;
                    }
                },
                'Z' | 'z' => {
                    if !self.cur.is_empty() {
                        let mut done = std::mem::take(&mut self.cur);
                        // A closed subpath re-states its first point for the join.
                        if let Some(&first) = done.first() {
                            if done.last() != Some(&first) {
                                done.push(first);
                            }
                        }
                        self.out.push(SubPath {
                            points: done,
                            closed: true,
                        });
                    }
                    self.cx = self.start_x;
                    self.cy = self.start_y;
                    self.prev_cmd = cmd;
                }
                _ => return None,
            }
        }
        self.flush();
        Some(())
    }

    fn flush(&mut self) {
        if self.cur.len() >= 2 {
            self.out.push(SubPath {
                points: std::mem::take(&mut self.cur),
                closed: false,
            });
        } else {
            self.cur.clear();
        }
    }

    fn next_is_num(&self) -> bool {
        matches!(self.tokens.get(self.pos), Some(PathTok::Num(_)))
    }

    fn num(&mut self) -> Option<i64> {
        match self.tokens.get(self.pos) {
            Some(PathTok::Num(n)) => {
                self.pos += 1;
                Some(*n)
            }
            _ => None,
        }
    }

    /// Resolve a coordinate pair against the current point; relative commands
    /// add to it, absolute ones stand alone.
    fn apply(&self, cmd: char, x: i64, y: i64) -> (i64, i64) {
        if cmd.is_ascii_lowercase() {
            (self.cx + x, self.cy + y)
        } else {
            (x, y)
        }
    }

    fn line_point(&mut self, cmd: char) -> Option<(i64, i64)> {
        let lower = cmd.to_ascii_lowercase();
        let (dx, dy) = match lower {
            'l' => (self.num()?, self.num()?),
            'h' => (self.num()?, 0),
            'v' => (0, self.num()?),
            _ => return None,
        };
        if cmd.is_ascii_lowercase() {
            Some(match lower {
                'h' => (self.cx + dx, self.cy),
                'v' => (self.cx, self.cy + dy),
                _ => (self.cx + dx, self.cy + dy),
            })
        } else {
            Some((dx, dy))
        }
    }

    /// Flatten an SVG elliptical arc from the current point to `(ex, ey)`.
    ///
    /// Handles the axis-aligned circular/elliptical arcs Tabler uses (rot = 0)
    /// with the standard endpoint -> centre parameterisation; a rotated arc is
    /// flattened to a straight segment (rare in the icon set). Integer only.
    #[allow(clippy::too_many_arguments)]
    fn arc(&mut self, rx: i64, ry: i64, rot_deg: i64, large: bool, sweep: bool, ex: i64, ey: i64) {
        let (sx, sy) = (self.cx, self.cy);
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0 || ry == 0 || (sx == ex && sy == ey) {
            self.cx = ex;
            self.cy = ey;
            self.cur.push((unit(ex), unit(ey)));
            return;
        }
        if rot_deg % 360 != 0 {
            // shortcut: rotated arcs flatten to a chord; upgrade if a package
            // ships a rotated elliptical arc.
            self.cx = ex;
            self.cy = ey;
            self.cur.push((unit(ex), unit(ey)));
            return;
        }
        let dx2 = (sx - ex) / 2;
        let dy2 = (sy - ey) / 2;
        let lam = (dx2 * dx2) / rx.max(1) / rx.max(1) + (dy2 * dy2) / ry.max(1) / ry.max(1);
        if lam > PATH_FIX {
            // Radii too small: scale up by sqrt(lam) (in fixed point).
            let scale = isqrt(lam * PATH_FIX);
            rx = rx * scale / PATH_FIX;
            ry = ry * scale / PATH_FIX;
        }
        let rx2 = rx * rx;
        let ry2 = ry * ry;
        let num = (rx2 * ry2 - rx2 * dy2 * dy2 - ry2 * dx2 * dx2).max(0);
        let den = (rx2 * dy2 * dy2 + ry2 * dx2 * dx2).max(1);
        let coef = isqrt(num.saturating_mul(PATH_FIX) / den);
        let sign = if large == sweep { -1 } else { 1 };
        // Centre offset in the (unrotated) ellipse frame.
        let cxp = sign * coef * rx * dy2 / (ry * PATH_FIX).max(1);
        let cyp = -sign * coef * ry * dx2 / (rx * PATH_FIX).max(1);
        let cx = cxp + (sx + ex) / 2;
        let cy = cyp + (sy + ey) / 2;
        // Start/end angles on the unit circle of the ellipse frame.
        let ux = (dx2 - cxp) / rx.max(1);
        let uy = (dy2 - cyp) / ry.max(1);
        let vx = (-dx2 - cxp) / rx.max(1);
        let vy = (-dy2 - cyp) / ry.max(1);
        let theta1 = atan2_approx(uy, ux);
        let mut dtheta = atan2_approx(ux * vy - uy * vx, ux * vx + uy * vy);
        if !sweep && dtheta > 0 {
            dtheta -= TAU;
        } else if sweep && dtheta < 0 {
            dtheta += TAU;
        }
        let steps = (dtheta.abs() * 60 / TAU).clamp(2, 96);
        for step in 1..=steps {
            let theta = theta1 + dtheta * step / steps;
            let (ct, st) = cos_sin_from_turn(theta);
            let px = cx + rx * ct / TRIG;
            let py = cy + ry * st / TRIG;
            self.cur.push((unit(px), unit(py)));
        }
        self.cx = ex;
        self.cy = ey;
    }

    fn reflect_control(&self, curve: char) -> (i64, i64) {
        if self.prev_cmd.to_ascii_uppercase() == curve {
            (2 * self.cx - self.prev_c2x, 2 * self.cy - self.prev_c2y)
        } else {
            (self.cx, self.cy)
        }
    }

    fn cubic(&mut self, x1: i64, y1: i64, x2: i64, y2: i64, ex: i64, ey: i64) {
        let (sx, sy) = (self.cx, self.cy);
        for step in 1..=CURVE_STEPS {
            let t = (step * PATH_FIX) / CURVE_STEPS;
            let mt = PATH_FIX - t;
            // Bezier basis scaled by PATH_FIX^3, then divided back down.
            let b0 = mt * mt * mt;
            let b1 = 3 * mt * mt * t;
            let b2 = 3 * mt * t * t;
            let b3 = t * t * t;
            let x = (b0 * sx + b1 * x1 + b2 * x2 + b3 * ex) / (PATH_FIX * PATH_FIX * PATH_FIX);
            let y = (b0 * sy + b1 * y1 + b2 * y2 + b3 * ey) / (PATH_FIX * PATH_FIX * PATH_FIX);
            self.cur.push((unit(x), unit(y)));
        }
        self.cx = ex;
        self.cy = ey;
    }

    fn quad(&mut self, x1: i64, y1: i64, ex: i64, ey: i64) {
        let (sx, sy) = (self.cx, self.cy);
        for step in 1..=CURVE_STEPS {
            let t = (step * PATH_FIX) / CURVE_STEPS;
            let mt = PATH_FIX - t;
            let b0 = mt * mt;
            let b1 = 2 * mt * t;
            let b2 = t * t;
            let x = (b0 * sx + b1 * x1 + b2 * ex) / (PATH_FIX * PATH_FIX);
            let y = (b0 * sy + b1 * y1 + b2 * ey) / (PATH_FIX * PATH_FIX);
            self.cur.push((unit(x), unit(y)));
        }
        self.cx = ex;
        self.cy = ey;
    }
}

/// Round a `PATH_FIX`-scaled coordinate to whole icon units, clamped to the box.
fn unit(fixed: i64) -> i64 {
    ((fixed + PATH_FIX / 2) / PATH_FIX).clamp(0, ICON_BOX)
}

/// Fixed-point coverage (0..=UNIT) of one pixel of a package icon drawn into a
/// `size x size` box. Same integer sub-sampling as [`icon_coverage`]; each
/// subpath's segments are stroked with the shared `STROKE_HALF` ribbon.
pub fn path_coverage(icon: &IconPaths, size: i64, px: i64, py: i64) -> i64 {
    let den = size * ICON_AA;
    if den <= 0 {
        return 0;
    }
    let mut on = 0i64;
    for sy in 0..ICON_AA {
        let cy = ((py * ICON_AA + sy) * ICON_BOX + ICON_BOX / 2) / den;
        for sx in 0..ICON_AA {
            let cx = ((px * ICON_AA + sx) * ICON_BOX + ICON_BOX / 2) / den;
            if path_hit(icon, cx, cy) {
                on += 1;
            }
        }
    }
    (on * UNIT) / (ICON_AA * ICON_AA)
}

fn path_hit(icon: &IconPaths, x: i64, y: i64) -> bool {
    icon.subpaths.iter().any(|sp| {
        sp.points
            .windows(2)
            .any(|w| segment_dist2(x, y, w[0], w[1]) <= STROKE_HALF * STROKE_HALF)
            || (sp.closed && closed_segment_hit(sp, x, y))
    })
}

fn closed_segment_hit(sp: &SubPath, x: i64, y: i64) -> bool {
    match (sp.points.first(), sp.points.last()) {
        (Some(&a), Some(&b)) => segment_dist2(x, y, b, a) <= STROKE_HALF * STROKE_HALF,
        _ => false,
    }
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
    fn trig_helpers_sane() {
        // cos/sin at cardinal turns, scaled by TRIG.
        assert_eq!(cos_sin_from_turn(0), (TRIG, 0));
        assert_eq!(cos_sin_from_turn(TAU / 4), (0, TRIG));
        assert_eq!(cos_sin_from_turn(TAU / 2), (-TRIG, 0));
        assert_eq!(cos_sin_from_turn(TAU * 3 / 4), (0, -TRIG));
        assert_eq!(atan2_approx(0, TRIG), 0);
        assert_eq!(atan2_approx(TRIG, 0), TAU / 4);
        assert_eq!(atan2_approx(0, -TRIG), TAU / 2);
        assert_eq!(atan2_approx(-TRIG, 0), TAU * 3 / 4);
    }

    #[test]
    fn path_parser_moveto_lineto_closepath() {
        let subs = parse_path_d("M3 3 L21 3 Z").unwrap();
        assert_eq!(subs.len(), 1);
        let sp = &subs[0];
        assert!(sp.closed, "trailing Z closes the subpath");
        assert_eq!(sp.points.first(), Some(&(3, 3)));
        assert_eq!(sp.points.last(), Some(&(3, 3)), "closed path repeats first");
        assert!(sp.points.contains(&(21, 3)));
    }

    #[test]
    fn path_parser_relative_and_curves() {
        // Relative lineto and a cubic that ends where it is told.
        let subs = parse_path_d("m0 0 l10 0 c0 5 5 10 10 10 z").unwrap();
        let sp = &subs[0];
        assert_eq!(sp.points.first(), Some(&(0, 0)));
        assert!(sp.points.contains(&(10, 0)));
        // Relative cubic end (10,10) lands at (20,10); curve points stay in-box.
        assert!(sp.points.iter().any(|&(x, y)| x == 20 && y == 10));
        assert!(sp
            .points
            .iter()
            .all(|&(x, y)| (0..=24).contains(&x) && (0..=24).contains(&y)));
    }

    #[test]
    fn path_parser_real_tabler_path_paints() {
        // A genuine Tabler stroke path from the music package (`music`).
        let d = "M8 4h10a2 2 0 0 1 2 2v10a2 2 0 0 1 -2 2h-10a2 2 0 0 1 -2 -2v-10a2 2 0 0 1 2 -2";
        let icon = IconPaths::from_paths("music", "0 0 24 24", &[d.to_string()]).unwrap();
        let mut any = false;
        let size = 32;
        for py in 0..size {
            for px in 0..size {
                if path_coverage(&icon, size, px, py) > 0 {
                    any = true;
                }
            }
        }
        assert!(any, "a real Tabler path must paint ink");
    }

    #[test]
    fn path_parser_rejects_foreign_viewbox_and_garbage() {
        assert!(IconPaths::from_paths("x", "0 0 48 48", &["M0 0 L1 1".into()]).is_none());
        assert!(IconPaths::from_paths("x", "0 0 24 24", &["not a path".into()]).is_none());
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
