//! Deterministic display list: the exact raster commands in draw order.
//!
//! The JSON shape is intentionally fixed so the independent GPU crate can parse
//! it with a small standard-library parser and no extra dependency.

use crate::ir::IrError;
use crate::particles::{PARTICLE_MAX_COUNT, PARTICLE_MAX_RADIUS, PARTICLE_MAX_SEED};
use crate::{Rect, Rgba};
use std::fmt::{self, Write as _};

/// The GPU backend validates these limits before dispatching threads.
pub const MAX_DIMENSION: u32 = 4096;
pub const MAX_COMMANDS: usize = 1_000_000;
pub const COORD_BIAS: i64 = 1 << 30;
pub const COORD_LIMIT: i64 = 500_000_000;
/// Largest corner radius the GPU protocol accepts. The shader evaluates
/// `(qx*ry)^2 + (qy*rx)^2 <= (rx*ry)^2` in 64-bit integers, so `rx*ry` must
/// stay below 2^30.
pub const GPU_RADIUS_LIMIT: i64 = 1 << 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Solid {
        rect: Rect,
        clip: Rect,
        color: Rgba,
    },
    Rounded {
        rect: Rect,
        clip: Rect,
        radius: i64,
        color: Rgba,
    },
    Gradient {
        rect: Rect,
        clip: Rect,
        radius: i64,
        top: Rgba,
        bottom: Rgba,
        /// Alpha already reduced from fixed point to the software raster's u8.
        alpha: u8,
    },
    /// Rounded rectangle with four independent corner radii. The radii array
    /// order is top-left, top-right, bottom-left, bottom-right (the GPU
    /// contract), each pair clamped to [`GPU_RADIUS_LIMIT`].
    RoundRectPerCorner {
        rect: Rect,
        clip: Rect,
        radii: [[i64; 2]; 4],
        color: Rgba,
    },
    /// The same two-stop gradient along the rect's horizontal axis.
    GradientHorizontal {
        rect: Rect,
        clip: Rect,
        radius: i64,
        top: Rgba,
        bottom: Rgba,
        /// Alpha already reduced from fixed point to the software raster's u8.
        alpha: u8,
    },
    /// Deterministic particle field: `count` integer dots scattered over the
    /// rect by a seeded pure integer hash, painted in dot order.
    Particles {
        rect: Rect,
        clip: Rect,
        count: i64,
        seed: i64,
        radius: i64,
        color: Rgba,
    },
    /// Snapshot the frame before a node's ordinary draw commands.
    EffectBackdrop,
    /// Snapshot the node's silhouette before any local effect runs.
    EffectMask {
        rect: Rect,
        clip: Rect,
        bounds: Rect,
        shape_radius: i64,
        shape: bool,
    },
    EffectShadow {
        bounds: Rect,
        radius: i64,
        dx: i64,
        dy: i64,
        alpha: i64,
        color: Rgba,
    },
    /// Per-node M3 material elevation shadow. `level` is the resolved M3
    /// table index (1/2/3/4/6); the replay backend expands it via the shared
    /// elevation table. Recorded in capture only when elevation > 0.
    EffectElevation {
        rect: Rect,
        clip: Rect,
        radius: i64,
        /// Non-uniform or elliptical corner radii in GPU wire order
        /// (TL,TR,BL,BR), each `[rx,ry]`. `None` selects the historical scalar
        /// `radius` path so default frames stay byte-identical. Flutter
        /// `_paintShadows` -> `_paintBox` paints the shadow with the node's
        /// `borderRadius.toRRect`, so the silhouette must follow per-corner radii.
        radii: Option<[[i64; 2]; 4]>,
        level: i64,
        color: Rgba,
    },
    EffectGlow {
        bounds: Rect,
        radius: i64,
        alpha: i64,
        color: Rgba,
    },
    EffectGlass {
        rect: Rect,
        clip: Rect,
        bounds: Rect,
        shape_radius: i64,
        radius: i64,
        alpha: i64,
        color: Rgba,
    },
    EffectLiquid {
        rect: Rect,
        clip: Rect,
        bounds: Rect,
        shape_radius: i64,
        denom: i64,
        alpha: i64,
        color: Rgba,
    },
    /// Whole-frame box blur, applied after every node in layout order.
    EffectBlur {
        radius: i64,
    },
    /// Whole-frame discrete-Gaussian blur (the GPU `effect_gaussian_blur`).
    EffectGaussianBlur {
        radius: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayList {
    pub width: u32,
    pub height: u32,
    pub background: Rgba,
    /// Frozen visual profile for this frame. Compat keeps the historical
    /// wire header unchanged; Product uses the same header so sibling
    /// encoders can stay compatible with older parsers.
    pub quality: crate::QualityProfile,
    pub commands: Vec<Command>,
    /// Unsupported geometry encountered while recording. Carried out of band
    /// so the JSON wire format is unchanged; `to_json` reports it as an error.
    pub unsupported: Option<String>,
}

fn bad(message: impl Into<String>) -> IrError {
    IrError(message.into())
}

fn validate_coord(value: i64, what: &str) -> Result<(), IrError> {
    if !(-COORD_LIMIT..=COORD_LIMIT).contains(&value) {
        return Err(bad(format!("{what} {value} exceeds the integer range")));
    }
    Ok(())
}

fn validate_rect(r: Rect, what: &str, positive: bool) -> Result<(), IrError> {
    validate_coord(r.x, &format!("{what}.x"))?;
    validate_coord(r.y, &format!("{what}.y"))?;
    validate_coord(r.w, &format!("{what}.w"))?;
    validate_coord(r.h, &format!("{what}.h"))?;
    if positive && (r.w <= 0 || r.h <= 0) {
        return Err(bad(format!("{what} must have positive width and height")));
    }
    Ok(())
}

fn write_rect(out: &mut String, key: &str, r: Rect) -> fmt::Result {
    write!(out, "\"{key}\":[{},{},{},{}]", r.x, r.y, r.w, r.h)
}

fn write_rgba(out: &mut String, key: &str, c: Rgba, alpha: u8) -> fmt::Result {
    write!(out, "\"{key}\":[{},{},{},{}]", c.r, c.g, c.b, alpha)
}

fn fmt_err(e: fmt::Error) -> IrError {
    bad(format!("cannot format display list: {e}"))
}

fn validate_effect(command: &Command, index: usize) -> Result<(), IrError> {
    let what = &format!("commands[{index}].effect");
    let mut values: Vec<i64> = Vec::with_capacity(24);
    let push_rect = |values: &mut Vec<i64>, r: Rect| {
        values.extend_from_slice(&[r.x, r.y, r.w, r.h]);
    };
    match *command {
        Command::EffectBackdrop => return Ok(()),
        Command::EffectBlur { radius } => values.push(radius),
        Command::EffectGaussianBlur { radius } => {
            let (lo, hi) = (
                crate::effects::GAUSSIAN_MIN_RADIUS,
                crate::effects::GAUSSIAN_MAX_RADIUS,
            );
            if !(lo..=hi).contains(&radius) {
                return Err(bad(format!("{what}.radius {radius} must be {lo}..={hi}")));
            }
            values.push(radius);
        }
        Command::EffectMask {
            rect,
            clip,
            bounds,
            shape_radius,
            shape,
        } => {
            push_rect(&mut values, rect);
            push_rect(&mut values, clip);
            push_rect(&mut values, bounds);
            values.extend_from_slice(&[shape_radius, i64::from(shape)]);
        }
        Command::EffectShadow {
            bounds,
            radius,
            dx,
            dy,
            alpha,
            ..
        } => {
            push_rect(&mut values, bounds);
            values.extend_from_slice(&[radius, dx, dy, alpha]);
        }
        Command::EffectElevation {
            rect,
            clip,
            radius,
            radii,
            level,
            ..
        } => {
            // The same Flutter 3.50.0 level set as the IR parser, which is
            // the only producer of this command.
            if !matches!(level, 1 | 2 | 3 | 4 | 6 | 8 | 9 | 12 | 16 | 24) {
                return Err(bad(format!(
                    "{what}.level {level} must be one of levels 1,2,3,4,6,8,9,12,16,24"
                )));
            }
            push_rect(&mut values, rect);
            push_rect(&mut values, clip);
            values.extend_from_slice(&[radius, level]);
            if let Some(radii) = radii {
                for (corner, pair) in radii.iter().enumerate() {
                    for (axis, value) in pair.iter().enumerate() {
                        let label = format!("{what}.radii[{corner}][{axis}]");
                        validate_coord(*value, &label)?;
                        if *value < 0 || *value > GPU_RADIUS_LIMIT {
                            return Err(bad(format!("{label} must be 0..={GPU_RADIUS_LIMIT}")));
                        }
                    }
                }
            }
        }
        Command::EffectGlow {
            bounds,
            radius,
            alpha,
            ..
        } => {
            push_rect(&mut values, bounds);
            values.extend_from_slice(&[radius, alpha]);
        }
        Command::EffectGlass {
            rect,
            clip,
            bounds,
            shape_radius,
            radius,
            alpha,
            ..
        } => {
            push_rect(&mut values, rect);
            push_rect(&mut values, clip);
            push_rect(&mut values, bounds);
            values.extend_from_slice(&[shape_radius, radius, alpha]);
        }
        Command::EffectLiquid {
            rect,
            clip,
            bounds,
            shape_radius,
            denom,
            alpha,
            ..
        } => {
            push_rect(&mut values, rect);
            push_rect(&mut values, clip);
            push_rect(&mut values, bounds);
            values.extend_from_slice(&[shape_radius, denom, alpha]);
        }
        _ => unreachable!("regular commands are validated above"),
    }
    for value in values {
        validate_coord(value, what)?;
    }
    Ok(())
}

/// Merge adjacent `Solid` commands that paint the exact same colour and clip.
/// Flutter 3.50.0 keeps display-list ops in draw order and derives paint
/// results from the recorded geometry, not from the number of list entries;
/// merging two abutting rects of one solid paint is therefore a pure
/// canonicalisation of the list. Only full-width horizontal strips or
/// full-height vertical strips touching by an edge are merged, so the painted
/// pixel set (and the src-over result for every opaque/transparent colour) is
/// unchanged. Effect commands and all other geometry are hard barriers.
/// This kernel is opt-in: the frozen default display-list fixture stays
/// byte-identical (PERF-002 in `reports/perf-audit.json`).
pub fn coalesce_solids(commands: Vec<Command>) -> Vec<Command> {
    let mut out: Vec<Command> = Vec::with_capacity(commands.len());
    for command in commands {
        let mut merged = false;
        if let Some(Command::Solid { rect, clip, color }) = out.last_mut() {
            if let Command::Solid {
                rect: next_rect,
                clip: next_clip,
                color: next_color,
            } = &command
            {
                if clip == next_clip
                    && color == next_color
                    && rect.w == next_rect.w
                    && rect.x == next_rect.x
                    && rect.y.saturating_add(rect.h) == next_rect.y
                {
                    rect.h += next_rect.h;
                    merged = true;
                } else if clip == next_clip
                    && color == next_color
                    && rect.h == next_rect.h
                    && rect.y == next_rect.y
                    && rect.x.saturating_add(rect.w) == next_rect.x
                {
                    rect.w += next_rect.w;
                    merged = true;
                }
            }
        }
        if !merged {
            out.push(command);
        }
    }
    out
}

impl DisplayList {
    pub fn to_json(&self) -> Result<String, IrError> {
        if let Some(reason) = &self.unsupported {
            return Err(bad(reason.clone()));
        }
        if self.width == 0
            || self.height == 0
            || self.width > MAX_DIMENSION
            || self.height > MAX_DIMENSION
        {
            return Err(bad(format!(
                "invalid viewport {}x{}: dimensions must be 1..={MAX_DIMENSION}",
                self.width, self.height
            )));
        }
        if self.commands.len() > MAX_COMMANDS {
            return Err(bad(format!(
                "display list has too many commands: {}",
                self.commands.len()
            )));
        }
        for (index, command) in self.commands.iter().enumerate() {
            let (rect, clip, radius) = match command {
                Command::Solid { rect, clip, .. } => (*rect, *clip, 0),
                Command::RoundRectPerCorner { rect, clip, .. } => (*rect, *clip, 0),
                Command::Rounded {
                    rect, clip, radius, ..
                }
                | Command::Gradient {
                    rect, clip, radius, ..
                }
                | Command::GradientHorizontal {
                    rect, clip, radius, ..
                } => (*rect, *clip, *radius),
                Command::Particles { rect, clip, .. } => (*rect, *clip, 0),
                _ => {
                    validate_effect(command, index)?;
                    continue;
                }
            };
            validate_rect(rect, &format!("commands[{index}].rect"), true)?;
            validate_rect(clip, &format!("commands[{index}].clip"), false)?;
            validate_coord(radius, &format!("commands[{index}].radius"))?;
            if let Command::RoundRectPerCorner { radii, .. } = command {
                for (corner, pair) in radii.iter().enumerate() {
                    for (axis, value) in pair.iter().enumerate() {
                        validate_coord(
                            *value,
                            &format!("commands[{index}].radii[{corner}][{axis}]"),
                        )?;
                        if *value < 0 || *value > GPU_RADIUS_LIMIT {
                            return Err(bad(format!(
                                "commands[{index}].radii[{corner}][{axis}] must be 0..={GPU_RADIUS_LIMIT}"
                            )));
                        }
                    }
                }
            }
            if let Command::Particles {
                count,
                seed,
                radius,
                ..
            } = command
            {
                let what = format!("commands[{index}]");
                for (name, value, hi) in [
                    ("count", *count, PARTICLE_MAX_COUNT),
                    ("seed", *seed, PARTICLE_MAX_SEED),
                    ("radius", *radius, PARTICLE_MAX_RADIUS),
                ] {
                    if !(0..=hi).contains(&value) {
                        return Err(bad(format!("{what}.{name} {value} must be 0..={hi}")));
                    }
                }
            }
        }

        let mut out = String::with_capacity(128 + self.commands.len() * 192);
        writeln!(
            out,
            "{{\"version\":1,\"width\":{},\"height\":{},\"background\":[{},{},{},255],\"commands\":[",
            self.width, self.height, self.background.r, self.background.g, self.background.b
        )
        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
        for (index, command) in self.commands.iter().enumerate() {
            if index > 0 {
                out.push_str(",\n");
            }
            match command {
                Command::Solid { rect, clip, color } => {
                    out.push_str("{\"kind\":\"solid\",");
                    write_rect(&mut out, "rect", *rect)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rgba(&mut out, "color", *color, color.a)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push('}');
                }
                Command::Rounded {
                    rect,
                    clip,
                    radius,
                    color,
                } => {
                    out.push_str("{\"kind\":\"rounded\",");
                    write_rect(&mut out, "rect", *rect)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write!(&mut out, ",\"radius\":{radius},")
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write_rgba(&mut out, "color", *color, color.a)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push('}');
                }
                Command::RoundRectPerCorner {
                    rect,
                    clip,
                    radii,
                    color,
                } => {
                    out.push_str("{\"kind\":\"round_rect_per_corner\",");
                    write_rect(&mut out, "rect", *rect)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push_str(",\"radii\":[");
                    for (i, pair) in radii.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        write!(&mut out, "[{},{}]", pair[0], pair[1])
                            .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    }
                    out.push_str("],");
                    write_rgba(&mut out, "color", *color, color.a)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push('}');
                }
                Command::Gradient {
                    rect,
                    clip,
                    radius,
                    top,
                    bottom,
                    alpha,
                } => {
                    out.push_str("{\"kind\":\"gradient\",");
                    write_rect(&mut out, "rect", *rect)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write!(&mut out, ",\"radius\":{radius},")
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write_rgba(&mut out, "top", *top, *alpha)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rgba(&mut out, "bottom", *bottom, *alpha)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write!(&mut out, ",\"alpha\":{alpha}}}")
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                }
                Command::GradientHorizontal {
                    rect,
                    clip,
                    radius,
                    top,
                    bottom,
                    alpha,
                } => {
                    out.push_str("{\"kind\":\"gradient_horizontal\",");
                    write_rect(&mut out, "rect", *rect)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write!(&mut out, ",\"radius\":{radius},")
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write_rgba(&mut out, "top", *top, *alpha)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    out.push(',');
                    write_rgba(&mut out, "bottom", *bottom, *alpha)
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                    write!(&mut out, ",\"alpha\":{alpha}}}")
                        .map_err(|e| bad(format!("cannot format display list: {e}")))?;
                }
                Command::Particles {
                    rect,
                    clip,
                    count,
                    seed,
                    radius,
                    color,
                } => {
                    out.push_str("{\"kind\":\"particles\",");
                    write_rect(&mut out, "rect", *rect).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip).map_err(fmt_err)?;
                    write!(
                        &mut out,
                        ",\"count\":{count},\"seed\":{seed},\"radius\":{radius},"
                    )
                    .map_err(fmt_err)?;
                    write_rgba(&mut out, "color", *color, color.a).map_err(fmt_err)?;
                    out.push('}');
                }
                Command::EffectBackdrop => out.push_str("{\"kind\":\"effect_backdrop\"}"),
                Command::EffectBlur { radius } => {
                    write!(&mut out, "{{\"kind\":\"effect_blur\",\"radius\":{radius}}}")
                        .map_err(fmt_err)?;
                }
                Command::EffectGaussianBlur { radius } => {
                    write!(
                        &mut out,
                        "{{\"kind\":\"effect_gaussian_blur\",\"radius\":{radius}}}"
                    )
                    .map_err(fmt_err)?;
                }
                Command::EffectMask {
                    rect,
                    clip,
                    bounds,
                    shape_radius,
                    shape,
                } => {
                    out.push_str("{\"kind\":\"effect_mask\",");
                    write_rect(&mut out, "rect", *rect).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "bounds", *bounds).map_err(fmt_err)?;
                    write!(
                        &mut out,
                        ",\"shape_radius\":{shape_radius},\"shape\":{}}}",
                        u8::from(*shape)
                    )
                    .map_err(fmt_err)?;
                }
                Command::EffectShadow {
                    bounds,
                    radius,
                    dx,
                    dy,
                    alpha,
                    color,
                } => {
                    out.push_str("{\"kind\":\"effect_shadow\",");
                    write_rect(&mut out, "bounds", *bounds).map_err(fmt_err)?;
                    write!(
                        &mut out,
                        ",\"radius\":{radius},\"dx\":{dx},\"dy\":{dy},\"alpha\":{alpha},"
                    )
                    .map_err(fmt_err)?;
                    write_rgba(&mut out, "color", *color, color.a).map_err(fmt_err)?;
                    out.push('}');
                }
                Command::EffectElevation {
                    rect,
                    clip,
                    radius,
                    radii,
                    level,
                    color,
                } => {
                    out.push_str("{\"kind\":\"effect_elevation\",");
                    write_rect(&mut out, "rect", *rect).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip).map_err(fmt_err)?;
                    write!(&mut out, ",\"radius\":{radius},").map_err(fmt_err)?;
                    if let Some(radii) = radii {
                        out.push_str("\"radii\":[");
                        for (i, pair) in radii.iter().enumerate() {
                            if i > 0 {
                                out.push(',');
                            }
                            write!(&mut out, "[{},{}]", pair[0], pair[1]).map_err(fmt_err)?;
                        }
                        out.push_str("],");
                    }
                    write!(&mut out, "\"level\":{level},").map_err(fmt_err)?;
                    write_rgba(&mut out, "color", *color, color.a).map_err(fmt_err)?;
                    out.push('}');
                }
                Command::EffectGlow {
                    bounds,
                    radius,
                    alpha,
                    color,
                } => {
                    out.push_str("{\"kind\":\"effect_glow\",");
                    write_rect(&mut out, "bounds", *bounds).map_err(fmt_err)?;
                    write!(&mut out, ",\"radius\":{radius},\"alpha\":{alpha},").map_err(fmt_err)?;
                    write_rgba(&mut out, "color", *color, color.a).map_err(fmt_err)?;
                    out.push('}');
                }
                Command::EffectGlass {
                    rect,
                    clip,
                    bounds,
                    shape_radius,
                    radius,
                    alpha,
                    color,
                } => {
                    out.push_str("{\"kind\":\"effect_glass\",");
                    write_rect(&mut out, "rect", *rect).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "bounds", *bounds).map_err(fmt_err)?;
                    write!(
                        &mut out,
                        ",\"shape_radius\":{shape_radius},\"radius\":{radius},\"alpha\":{alpha},"
                    )
                    .map_err(fmt_err)?;
                    write_rgba(&mut out, "color", *color, color.a).map_err(fmt_err)?;
                    out.push('}');
                }
                Command::EffectLiquid {
                    rect,
                    clip,
                    bounds,
                    shape_radius,
                    denom,
                    alpha,
                    color,
                } => {
                    out.push_str("{\"kind\":\"effect_liquid\",");
                    write_rect(&mut out, "rect", *rect).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "clip", *clip).map_err(fmt_err)?;
                    out.push(',');
                    write_rect(&mut out, "bounds", *bounds).map_err(fmt_err)?;
                    write!(
                        &mut out,
                        ",\"shape_radius\":{shape_radius},\"denom\":{denom},\"alpha\":{alpha},"
                    )
                    .map_err(fmt_err)?;
                    write_rgba(&mut out, "color", *color, color.a).map_err(fmt_err)?;
                    out.push('}');
                }
            }
        }
        if self.commands.is_empty() {
            out.push_str("]}\n");
        } else {
            out.push_str("\n]}\n");
        }
        Ok(out)
    }
}

#[cfg(test)]
mod coalescing_tests {
    use super::{coalesce_solids, Command};
    use crate::{Rect, Rgba};

    fn solid(x: i64, y: i64, w: i64, h: i64) -> Command {
        Command::Solid {
            rect: Rect { x, y, w, h },
            clip: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            color: Rgba::rgb(7, 11, 19),
        }
    }

    #[test]
    fn adjacent_vertical_strips_merge_into_one_rect() {
        let merged = coalesce_solids(vec![solid(0, 0, 10, 5), solid(0, 5, 10, 7)]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], solid(0, 0, 10, 12));
    }

    #[test]
    fn adjacent_horizontal_strips_merge_into_one_rect() {
        let merged = coalesce_solids(vec![solid(0, 0, 4, 9), solid(4, 0, 6, 9)]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], solid(0, 0, 10, 9));
    }

    #[test]
    fn different_colours_clips_and_gaps_do_not_merge() {
        let mut other = solid(0, 5, 10, 3);
        if let Command::Solid { color, .. } = &mut other {
            *color = Rgba::rgb(1, 2, 3);
        }
        assert_eq!(coalesce_solids(vec![solid(0, 0, 10, 5), other]).len(), 2);
        assert_eq!(
            coalesce_solids(vec![solid(0, 0, 10, 5), solid(0, 6, 10, 3)]).len(),
            2
        );
        let mut clipped = solid(0, 5, 10, 3);
        if let Command::Solid { clip, .. } = &mut clipped {
            clip.w = 50;
        }
        assert_eq!(coalesce_solids(vec![solid(0, 0, 10, 5), clipped]).len(), 2);
    }

    #[test]
    fn effect_commands_are_barriers() {
        let merged = coalesce_solids(vec![
            solid(0, 0, 10, 5),
            Command::EffectBackdrop,
            solid(0, 5, 10, 7),
        ]);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn merge_is_transitive_over_a_run() {
        let merged = coalesce_solids(vec![
            solid(0, 0, 10, 3),
            solid(0, 3, 10, 4),
            solid(0, 7, 10, 5),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], solid(0, 0, 10, 12));
    }
}
