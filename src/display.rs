//! Deterministic display list: the exact raster commands in draw order.
//!
//! The JSON shape is intentionally fixed so the independent GPU crate can parse
//! it with a small standard-library parser and no extra dependency.

use crate::ir::IrError;
use crate::{Rect, Rgba};
use std::fmt::{self, Write as _};

/// The GPU backend validates these limits before dispatching threads.
pub const MAX_DIMENSION: u32 = 4096;
pub const MAX_COMMANDS: usize = 1_000_000;
pub const COORD_BIAS: i64 = 1 << 30;
pub const COORD_LIMIT: i64 = 500_000_000;

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayList {
    pub width: u32,
    pub height: u32,
    pub background: Rgba,
    pub commands: Vec<Command>,
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

impl DisplayList {
    pub fn to_json(&self) -> Result<String, IrError> {
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
                Command::Rounded {
                    rect, clip, radius, ..
                }
                | Command::Gradient {
                    rect, clip, radius, ..
                } => (*rect, *clip, *radius),
                _ => {
                    validate_effect(command, index)?;
                    continue;
                }
            };
            validate_rect(rect, &format!("commands[{index}].rect"), true)?;
            validate_rect(clip, &format!("commands[{index}].clip"), false)?;
            validate_coord(radius, &format!("commands[{index}].radius"))?;
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
                Command::EffectBackdrop => out.push_str("{\"kind\":\"effect_backdrop\"}"),
                Command::EffectBlur { radius } => {
                    write!(&mut out, "{{\"kind\":\"effect_blur\",\"radius\":{radius}}}")
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
