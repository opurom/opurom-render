//! `opurom-render`: deterministic IR 0.1.0 layout + RGBA8 software raster.
//!
//! Pipeline: `parse_ir` -> `StateView` -> `layout` -> RGBA frame -> dirty rects.
//! Same IR + same viewport + same state = byte-identical pixels on every host:
//! integer-only arithmetic, sorted state map, no wall-clock time, no unseeded
//! randomness, no host queries (`theme.mode == "system"` resolves to light).
//!
//! The interactive UI layer adds theme tokens, canonical style effects, conditional
//! `when` visibility, sixteen new node kinds and a fixed-timestep animation clock.
//! The component catalog adds optional accessibility props and a data-only
//! accessibility tree. The async language layer adds eight more distinct
//! primitives and a deterministic SVG 1.1 vector backend (a second software
//! backend, not a GPU backend). The GPU and transport layer adds deterministic page
//! transitions, gesture progress, fixed-seed particles and caller-supplied quality
//! tiers. The platform delivery layer adds an exact display-list snapshot of the
//! software raster stage. The visual fidelity layer adds five optional
//! deterministic framebuffer effects plus an opt-in deterministic particle
//! field: a fixed-count scatter of integer dots whose positions are a pure
//! function of a bounded integer seed. All of them are opt-in: a 0.1.0 IR renders
//! byte-identically to the core release, and effect-free `--commands` output stays
//! byte-identical to the effect-free command stream. The production layer adds
//! optional effect commands that reproduce those effects on the Metal compute
//! backend. The component library hardening adds 68 more distinct drawn primitives
//! for a 100-primitive library, each with a deterministic integer-only drawing, a
//! documented prop schema and an a11y default role; none of them change an earlier
//! frame. The media layer adds opt-in dependency-free PNG images and text line
//! limits/alignment, also leaving earlier frames byte-identical. The interaction
//! matrix adds eight deterministic states (default, hover, active, focus,
//! disabled, loading, success, error), six design variants and five semantic
//! tones; all are opt-in and leave earlier frames byte-identical. Loading and
//! success are business conventions with NO_FLUTTER_EQUIVALENT.

pub mod a11y;
pub mod aaa;
mod aaa_inverse;
pub mod aaa_walker;
pub mod anim;
pub mod canvas_prims;
pub mod curve;
pub mod display;
pub mod effects;
pub mod font;
pub mod geometry;
pub mod gestures;
pub mod gradient;
pub mod icons;
pub mod image;
pub mod incremental;
pub mod interaction;
pub mod ir;
pub mod layer;
pub mod layout;
pub mod motion;
pub mod nav;
pub mod particles;
pub mod path_ops;
pub mod perf;
pub mod primitives;
pub mod profile;
pub mod quality;
pub mod raster;
pub mod recipes;
pub mod scroll_physics;
pub mod showcase;
pub mod svg;
pub mod target_painter;
pub mod text_span;
pub mod typography;
pub mod visual_gate;

pub use a11y::{a11y_json, a11y_json_at, a11y_tree, a11y_tree_at, A11yNode};
pub use anim::{
    animate_progress, choose_quality, default_duration_ms, lerp, lerp_rgba, AnimSpec, AnimTable,
    AnimValue, Animator, Curve, FrameRate, GestureAnim, Particle, ParticleSystem, Quality,
    StepMode, Transition, TransitionKind, ANIMATE_DEFAULT_MS, ANIMATE_MAX_MS, INERTIA_SETTLE_MS,
    M3_MEDIUM1_MS, M3_SHORT1_MS, ONE, PARTICLE_FIELD, PARTICLE_MAX_LIFE, SPRING_SETTLE_MS,
    TRANSITION_MS,
};
pub use curve::cubic_bezier;
pub use display::{Command, DisplayList, MAX_COMMANDS, MAX_DIMENSION};
pub use effects::{
    elevation_level, project_state_level, VisualEffect, VisualEffectKind, EFFECT_MAX_OFFSET,
    EFFECT_MAX_RADIUS, EFFECT_UNIT, ELEVATION_TABLE,
};
pub use font::{measure, FontMetrics};
pub use gestures::{
    hit, tap_hit_rect, DragStartBehavior, Gesture, GestureEngine, GestureKind, Phase, PointerEvent,
    PointerKind, DOUBLE_TAP_INTERVAL_MS, DOUBLE_TAP_MIN_MS, DOUBLE_TAP_SLOP, DRAG_THRESHOLD,
    LONG_PRESS_MS, PAN_SLOP, SCALE_SLOP, SCALE_UNIT,
};
pub use icons::{icon_coverage, icon_shape, IconShape, Shape, ICON_NAMES};
pub use image::{
    CacheStats, Image, ImageFit, CACHE_CAPACITY, MAX_DECODED_BYTES, MAX_IMAGE_DIMENSION,
};
pub use incremental::{
    align_by_key, dep_values, deps, paint_hash, plan_dirty, subtree_hash, Hasher, IncrementalMeta,
    KeyedNode, LayoutCache, NodeIdentity, RecomputeCounters,
};
pub use interaction::{
    is_interactive, press_color, ripple_alpha_confirmed, ripple_alpha_held, ripple_frame,
    ripple_progress, ripple_radius_confirmed, ripple_radius_unconfirmed, ripple_target_radius,
    NodeState, PressPhase, Tone, Variant, CYAN, DEEP_INK, ERROR, FOCUS_RING, INFO, PRIMARY,
    RIPPLE_DURATION_MS, RIPPLE_FADE_IN_MS, RIPPLE_FADE_OUT_MS, RIPPLE_FADE_OUT_START_MS,
    RIPPLE_PEAK_ALPHA, RIPPLE_RADIUS_MS, RIPPLE_START_PERCENT, RIPPLE_TOTAL_MS,
    RIPPLE_UNCONFIRMED_MS, SUCCESS, WARNING,
};
pub use ir::{
    parse_ir, parse_state_json, Action, Align, Contrast, CornerRadiiProp, Directionality, Event,
    Ir, IrError, Kind, Node, Op, Props, RenderQuality, Role, Route, StateDecl, StateValue,
    StateView, Theme, ThemeMode, ThemeTokens, ValueType,
};
pub use layout::{
    fluid_grid, layout, layout_animated_at, layout_at, layout_cached_at, Breakpoint, Control,
    Direction, Effects, FluidGrid, LaidOut, Palette, Rect, MAX_GRID_COLUMNS, MIN_COLUMN_WIDTH,
    SETTLED_MS,
};
pub use motion::{MotionEvent, MotionSpec};
pub use nav::{NavError, Navigation, Navigator, MAX_STACK_DEPTH};
pub use particles::{Particles, PARTICLE_MAX_COUNT, PARTICLE_MAX_RADIUS, PARTICLE_MAX_SEED};
pub use perf::{
    apply_policy, perf_report, FrameBudget, PerfReport, PerfWindow, QualityTier, RenderConfig,
    TierCaps, PROMOTION_DEN, PROMOTION_NUM, PROMOTION_WINDOWS, WINDOW_FRAMES,
};
pub use profile::{
    disable_motion, MotionSpec as CliMotionSpec, RenderPolicy, RenderProfile, DEFAULT_DPR,
    DPI_UNIT, MAX_DPR,
};
pub use quality::{DprScale, QualityProfile};
pub use recipes::{
    ComponentState, ComponentVariant, RecipeKind, A11Y_ROLES, COLOR_SLOTS, COMPONENT_STATES,
    COMPONENT_VARIANTS,
};
pub use showcase::{
    visual_matrix, SceneCell, ShowcaseScene, ShowcaseTheme, FRAME_ARTIFACTS, SHOWCASE_VIEWPORTS,
};
pub use svg::{svg, svg_at};
pub use target_painter::{PainterRequirements, TargetPainter};
pub use text_span::{text_spans, text_spans_for, text_spans_json, TextSpan};
pub use typography::{FontSource, TextRole, TypographySpec};
pub use visual_gate::{
    check_component_states, check_contrast, check_focus_states, check_min_touch_targets,
    check_text_overlap, contrast_ratio_x100, relative_luminance_x100, LayoutFact, TextFact,
    VisualFinding,
};

/// One RGBA8 pixel. The software frame is always opaque (a = 255).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r, g, b, a: 255 }
    }

    /// Strict `#RRGGBB` parse; no named colours, no shorthand.
    pub fn from_hex(s: &str) -> Option<Rgba> {
        let b = s.as_bytes();
        if b.len() != 7 || b[0] != b'#' {
            return None;
        }
        let digit = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let pair = |hi: u8, lo: u8| -> Option<u8> { Some(digit(hi)? * 16 + digit(lo)?) };
        Some(Rgba::rgb(
            pair(b[1], b[2])?,
            pair(b[3], b[4])?,
            pair(b[5], b[6])?,
        ))
    }
}

/// FNV-1a 64-bit over RGBA bytes, implemented inline so the CLI has no extra deps.
pub fn digest(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn digest_hex(bytes: &[u8]) -> String {
    format!("{:016x}", digest(bytes))
}

/// Axis-aligned changed region in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Renderer diffs are bounded, covering and ordered by `(y, x)`.
    pub dirty: Vec<DirtyRect>,
}

/// Borrowed pixel view with the exact field layout of the framework implementation's
/// `FrameView` (a local struct: the renderer has no cross-repo dependency).
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

impl Frame {
    pub fn view(&self) -> FrameView<'_> {
        FrameView {
            width: self.width,
            height: self.height,
            pixels: &self.pixels,
        }
    }
}

const MAX_PIXELS: u64 = 16_777_216; // 4096x4096 RGBA = 64 MiB
const MAX_DIRTY: usize = 64;

use std::cell::RefCell;

/// Keeps the previous RGBA buffer so successive renders can be diffed.
pub struct Renderer {
    width: u32,
    height: u32,
    prev: Option<Vec<u8>>,
    /// Viewport size of the buffered frame, for the resize diff.
    prev_size: Option<(u32, u32)>,
    /// Previous laid-out node list, used to locate the nodes that changed.
    prev_laid: Option<Vec<LaidOut>>,
    /// Ripple disc painted in the previous press frame. The ripple extends
    /// outside its node box, so the next incremental frame must re-rasterize
    /// this area even when the node boxes are unchanged — otherwise moving a
    /// press leaves the old ripple in the buffer. `None` means no prior ripple.
    prev_press_disc: Option<crate::layout::Rect>,
    /// Hint for `finish_frame`: a known superset of the changed pixels. When
    /// set, the full-frame pixel diff is skipped and this rect is emitted as
    /// the dirty list. Incremental/press paths know their own dirty region;
    /// re-scanning 10 MB every frame just to rediscover it was the dominant
    /// per-frame cost. Cleared after one frame.
    dirty_hint: Option<crate::layout::Rect>,
    /// Nodes and pixels the most recent incremental render recomputed.
    redrawn_nodes: usize,
    redraw_pixels: u64,
    /// Cross-tick implicit-animation state: each animated node's previous and
    /// current prop targets. A `RefCell` lets the `&self` snapshot methods
    /// advance the animation without changing their signatures.
    anim: RefCell<AnimTable>,
    /// Opt-in 4x geometric edge coverage; off by default for golden parity.
    edge_aa: bool,
    /// Opt-in display-layer split: skip bitmap text for system-font overlay.
    suppress_text: bool,
    /// Device pixel ratio in fixed point (`1024 == 1x`). Layout keeps logical
    /// pixels; the raster pass uses this to size stroke/edge coverage. The
    /// default of 1x is byte-identical to the pinned renderer.
    dpr: i64,
    /// Frozen visual quality profile for raster and display-list output.
    quality: QualityProfile,
}

/// True when the IR node with `id` is tappable: either a `gesture: tap` prop
/// or a `tap` event. The Cards demo Action nodes use the event form. The press
/// phase is caller-owned; only authored tappable nodes gain the Text layer.
fn node_has_tap(node: &crate::ir::Node, id: u32) -> bool {
    if node.id == id {
        return node.props.gesture == Some(crate::gestures::GestureKind::Tap)
            || node.events.iter().any(|e| e.name == "tap");
    }
    node.children.iter().any(|c| node_has_tap(c, id))
}

/// Nearest button-semantic node at or above `target`, following the IR tree.
/// The hit test can land on a Button's Text child; Flutter paints the splash
/// on the Material box, so the ripple host is the Button ancestor.
pub fn ink_host(node: &crate::ir::Node, target: u32, ancestor: Option<u32>) -> Option<u32> {
    let here = if is_ink_node(node) {
        Some(node.id)
    } else {
        ancestor
    };
    if node.id == target {
        return here;
    }
    for c in &node.children {
        if let Some(host) = ink_host(c, target, here) {
            return Some(host);
        }
    }
    None
}

fn is_ink_node(node: &crate::ir::Node) -> bool {
    node.kind == Kind::Button
        || node.props.variant == Some(Variant::Fab)
        || node.props.role == Some(crate::ir::Role::Button)
}

/// Resolve a press to its ripple host and translate the touch point from the
/// hit node's coordinates into the host's coordinates.
fn press_host<'a>(
    laid: &'a [LaidOut],
    ir: &Ir,
    press: &PressPhase,
) -> Option<(&'a LaidOut, PressPhase)> {
    let id = ink_host(&ir.tree, press.node_id, None)?;
    let host = laid.iter().find(|n| n.id == id)?;
    let hit = laid.iter().find(|n| n.id == press.node_id)?;
    let touch = press
        .touch
        .map(|(tx, ty)| (hit.rect.x + tx - host.rect.x, hit.rect.y + ty - host.rect.y));
    Some((
        host,
        PressPhase {
            node_id: id,
            press_start_tick: press.press_start_tick,
            confirm_tick: press.confirm_tick,
            touch,
        },
    ))
}

impl Renderer {
    pub fn try_new(width: u32, height: u32) -> Result<Renderer, IrError> {
        if width == 0 || height == 0 || width as u64 * height as u64 > MAX_PIXELS {
            return Err(IrError(format!(
                "invalid viewport {width}x{height}: both dimensions must be non-zero and at most {MAX_PIXELS} pixels"
            )));
        }
        Ok(Renderer {
            width,
            height,
            prev: None,
            prev_size: None,
            prev_laid: None,
            prev_press_disc: None,
            dirty_hint: None,
            redrawn_nodes: 0,
            redraw_pixels: 0,
            anim: RefCell::new(AnimTable::new()),
            edge_aa: QualityProfile::default().edge_coverage(),
            suppress_text: QualityProfile::default().system_font_overlay(),
            dpr: crate::profile::DEFAULT_DPR,
            quality: QualityProfile::default(),
        })
    }

    /// Enable opt-in geometric edge coverage on the software raster. The
    /// default path stays byte-identical; capture/display-list output ignores it.
    pub fn set_edge_aa(&mut self, on: bool) {
        self.edge_aa = on;
    }

    /// Set the device pixel ratio as a fixed-point value (`1024 == 1x`).
    /// Logical layout is unchanged; a ratio above 1x enables the finer edge
    /// coverage grid. The default 1x keeps every pinned frame byte-identical.
    pub fn set_dpr(&mut self, dpr: i64) -> Result<(), IrError> {
        if dpr < crate::profile::DPI_UNIT || dpr > crate::profile::MAX_DPR {
            return Err(IrError(format!(
                "invalid dpr {dpr}: expected {}..={} (fixed point, 1024 == 1x)",
                crate::profile::DPI_UNIT,
                crate::profile::MAX_DPR
            )));
        }
        self.dpr = dpr;
        Ok(())
    }

    /// Effective device pixel ratio in fixed point (`1024 == 1x`).
    pub fn dpr(&self) -> i64 {
        self.dpr
    }

    /// Skip bitmap text so a display host can overlay system-font spans. The
    /// default path stays byte-identical; capture mode is unaffected.
    pub fn set_suppress_text(&mut self, on: bool) {
        self.suppress_text = on;
    }

    /// Select the frozen visual quality profile. Compat keeps historical
    /// golden bytes; Product enables the full visual contract.
    pub fn set_quality(&mut self, profile: QualityProfile) {
        self.quality = profile;
        self.edge_aa = profile.edge_coverage();
        self.suppress_text = profile.system_font_overlay();
    }

    pub fn quality(&self) -> QualityProfile {
        self.quality
    }

    /// Convenience constructor; use `try_new` for untrusted CLI input.
    pub fn new(width: u32, height: u32) -> Renderer {
        Renderer::try_new(width, height).unwrap_or_else(|e| panic!("{e}"))
    }

    pub fn viewport(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Nodes and pixels recomputed by the most recent `render_incremental_at`
    /// call, so callers can observe that incremental work skipped unchanged
    /// regions. Zero after a full render.
    pub fn redraw_stats(&self) -> (usize, u64) {
        (self.redrawn_nodes, self.redraw_pixels)
    }

    /// Change the viewport size, as a window resize or a split-screen split does.
    /// The buffered frame is kept, so the next render diffs against the old
    /// surface instead of reporting the whole new one as changed.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), IrError> {
        if width == 0 || height == 0 || width as u64 * height as u64 > MAX_PIXELS {
            return Err(IrError(format!(
                "invalid viewport {width}x{height}: both dimensions must be non-zero and at most {MAX_PIXELS} pixels"
            )));
        }
        self.width = width;
        self.height = height;
        Ok(())
    }

    pub fn compute_layout(&self, ir: &Ir, state: &StateView) -> Vec<LaidOut> {
        crate::layout::layout(ir, state, self.width, self.height)
    }

    /// Layout at a given animation clock position (milliseconds). Advances the
    /// renderer's implicit-animation table, so a prop change animates.
    pub fn compute_layout_at(&self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> Vec<LaidOut> {
        crate::layout::layout_animated_at(
            ir,
            state,
            self.width,
            self.height,
            elapsed_ms,
            &mut self.anim.borrow_mut(),
        )
    }

    /// Pure raster: no mutation, no hidden state.
    pub fn pixels(&self, ir: &Ir, state: &StateView) -> Vec<u8> {
        self.pixels_at(ir, state, SETTLED_MS)
    }

    /// Pure raster at a given animation clock position.
    pub fn pixels_at(&self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> Vec<u8> {
        let pal = Palette::from_theme(&ir.theme);
        let laid = crate::layout::layout_animated_at(
            ir,
            state,
            self.width,
            self.height,
            elapsed_ms,
            &mut self.anim.borrow_mut(),
        );
        let mut canvas = raster::Canvas::new(self.width, self.height, pal.stage);
        canvas.set_edge_aa(self.edge_aa);
        canvas.set_dpr(self.dpr);
        canvas.set_suppress_text(self.suppress_text);
        canvas.set_render_quality(ir.render_quality.as_ref());
        for n in &laid {
            raster::draw(&mut canvas, n);
        }
        effects::apply_frame_blurs(&mut canvas, &laid);
        canvas.pixels
    }

    /// Pure display list at the settled animation clock position.
    pub fn display_list(&self, ir: &Ir, state: &StateView) -> DisplayList {
        self.display_list_at(ir, state, SETTLED_MS)
    }

    /// Record the exact raster commands, including CPU-decomposed glyph blocks,
    /// plus effect commands in the order the software framebuffer applies
    /// them. The software frame and the incremental renderer state are unchanged.
    pub fn display_list_at(&self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> DisplayList {
        let pal = Palette::from_theme(&ir.theme);
        // A fresh animation table keeps this query pure: layout_animated_at
        // writes schedules into the table, so reusing the renderer's persistent
        // one would advance it and produce commands for a different frame.
        let laid = crate::layout::layout_animated_at(
            ir,
            state,
            self.width,
            self.height,
            elapsed_ms,
            &mut AnimTable::new(),
        );
        let mut canvas = raster::Canvas::capturing(self.width, self.height, pal.stage);
        canvas.set_edge_aa(self.edge_aa);
        canvas.set_dpr(self.dpr);
        canvas.set_suppress_text(self.suppress_text);
        for n in &laid {
            let frame = crate::effects::EffectFrame::new(self.width, self.height, n);
            if frame.has_local {
                canvas.emit(Command::EffectBackdrop);
            }
            raster::draw(&mut canvas, n);
            if frame.has_local {
                for command in crate::effects::local_commands(frame, &n.visual_effects) {
                    canvas.emit(command);
                }
            }
        }
        for command in crate::effects::frame_blur_commands(&laid) {
            canvas.emit(command);
        }
        DisplayList {
            unsupported: canvas.take_unsupported(),
            width: self.width,
            height: self.height,
            background: pal.stage,
            quality: self.quality,
            commands: canvas.into_commands(),
        }
    }

    /// Render and diff against the previous frame. Unchanged pixels => empty
    /// dirty list; changed pixels => a bounded, covering rect list.
    pub fn render(&mut self, ir: &Ir, state: &StateView) -> Frame {
        self.render_at(ir, state, SETTLED_MS)
    }

    /// Render at a given animation clock position and diff against the previous
    /// frame. The clock is the only time source; no wall clock is consulted.
    pub fn render_at(&mut self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> Frame {
        let laid = crate::layout::layout_animated_at(
            ir,
            state,
            self.width,
            self.height,
            elapsed_ms,
            &mut self.anim.borrow_mut(),
        );
        let pixels = self.raster_laid(&laid, ir);
        self.finish_frame(laid, pixels)
    }

    /// Render at `elapsed_ms` with an optional caller-owned press phase: the
    /// pressed node resolves to `Active` (authored `disabled` still wins) and
    /// its ripple is painted with the same caller clock. No phase, or no press,
    /// is byte-identical to [`Renderer::render_at`].
    pub fn render_with_press(
        &mut self,
        ir: &Ir,
        state: &StateView,
        elapsed_ms: i64,
        press: Option<&PressPhase>,
    ) -> Frame {
        let mut laid = crate::layout::layout_animated_at(
            ir,
            state,
            self.width,
            self.height,
            elapsed_ms,
            &mut self.anim.borrow_mut(),
        );
        if let Some(p) = press {
            // Flutter gives press feedback only to the ink host. The hit test
            // may land on a text/row inside a Button; tint the Button instead.
            let host_id = press_host(&laid, ir, p).map(|(host, _)| host.id);
            if let Some(id) = host_id {
                if let Some(n) = laid.iter_mut().find(|n| n.id == id) {
                    if n.state != NodeState::Disabled {
                        n.state = NodeState::Active;
                    }
                }
            }
        }
        let pixels = self.raster_laid_press(&laid, ir, press, elapsed_ms);
        self.prev_press_disc = press
            .and_then(|p| press_host(&laid, ir, p))
            .map(|(host, hp)| ripple_disc(host, hp.touch, self.width, self.height));
        self.finish_frame(laid, pixels)
    }

    /// Incremental press frame: identical pixels to `render_with_press`, but
    /// only re-rasterizes the dirty region (the press node's state colour and
    /// the ripple disc) over the previous buffer. A ripple paints outside the
    /// node box, so its full-lifetime disc is unioned into the region; this
    /// also clears the previous frame's disc. Falls back to the full path
    /// whenever incremental reuse is unsafe.
    pub fn render_with_press_incremental(
        &mut self,
        ir: &Ir,
        state: &StateView,
        elapsed_ms: i64,
        press: Option<&PressPhase>,
    ) -> Frame {
        let mut laid = crate::layout::layout_animated_at(
            ir,
            state,
            self.width,
            self.height,
            elapsed_ms,
            &mut self.anim.borrow_mut(),
        );
        if let Some(p) = press {
            // Flutter gives press feedback only to the ink host. The hit test
            // may land on a text/row inside a Button; tint the Button instead.
            let host_id = press_host(&laid, ir, p).map(|(host, _)| host.id);
            if let Some(id) = host_id {
                if let Some(n) = laid.iter_mut().find(|n| n.id == id) {
                    if n.state != NodeState::Disabled {
                        n.state = NodeState::Active;
                    }
                }
            }
        }
        // Frame-level effects (blur/glass) read the whole frame and are applied
        // after raster; reusing a previously-blurred buffer would blur twice.
        // Fall back to the exact full path for such layouts.
        if !layout_is_incremental_safe(&laid) {
            return self.render_with_press(ir, state, elapsed_ms, press);
        }
        let pal = Palette::from_theme(&ir.theme);
        let reusable = self.prev.is_some()
            && self.prev_laid.is_some()
            && self.prev_size == Some((self.width, self.height));
        let host = press.and_then(|p| press_host(&laid, ir, p));
        let cur_disc = host
            .as_ref()
            .map(|(host, hp)| ripple_disc(host, hp.touch, self.width, self.height));
        let region = if reusable {
            let mut r = changed_region(
                &laid,
                self.prev_laid.as_ref().unwrap(),
                self.width,
                self.height,
            );
            // A press moving to another host leaves its old splash in the
            // buffer. Re-rasterize both discs so the old one is cleared.
            if let Some(d) = cur_disc {
                r = union_rect(r, d);
            }
            if let Some(d) = self.prev_press_disc {
                r = union_rect(r, d);
            }
            r
        } else {
            Rect {
                x: 0,
                y: 0,
                w: self.width as i64,
                h: self.height as i64,
            }
        };

        let pixels = if region.w <= 0 || region.h <= 0 {
            self.prev
                .clone()
                .unwrap_or_else(|| self.raster_laid_press(&laid, ir, press, elapsed_ms))
        } else {
            let mut canvas = raster::Canvas::from_pixels(
                self.width,
                self.height,
                self.prev
                    .clone()
                    .unwrap_or_else(|| vec![0u8; (self.width * self.height * 4) as usize]),
            );
            canvas.set_edge_aa(self.edge_aa);
            canvas.set_dpr(self.dpr);
            canvas.set_suppress_text(self.suppress_text);
            canvas.set_render_quality(ir.render_quality.as_ref());
            canvas.clip = Some(region);
            canvas.fill_rect(region, pal.stage);
            for n in &laid {
                if !rect_intersects(n.rect, region) {
                    continue;
                }
                if host.as_ref().map_or(false, |(h, _)| h.id == n.id)
                    && n.kind == crate::Kind::Text
                    && n.state == NodeState::Active
                    && node_has_tap(&ir.tree, n.id)
                {
                    let c = n.text_color.unwrap_or(n.accent);
                    canvas.fill_rounded_radii(
                        n.rect,
                        Rgba {
                            r: c.r,
                            g: c.g,
                            b: c.b,
                            a: 26,
                        },
                        n.radii,
                    );
                }
                raster::draw(&mut canvas, n);
                if let Some((h, hp)) = host.as_ref() {
                    if h.id == n.id {
                        canvas.draw_node_ripple(n, hp, elapsed_ms, n.accent);
                    }
                }
            }
            canvas.pixels
        };
        self.prev_press_disc = cur_disc;
        self.dirty_hint = Some(region);
        self.finish_frame(laid, pixels)
    }

    /// Post-layout bookkeeping shared by every full render: diff against the
    /// buffered frame, then commit the new buffer and node list.
    fn finish_frame(&mut self, laid: Vec<LaidOut>, pixels: Vec<u8>) -> Frame {
        let dirty = if let Some(r) = self.dirty_hint.take() {
            if r.w <= 0 || r.h <= 0 {
                Vec::new()
            } else {
                vec![DirtyRect {
                    x: r.x.max(0) as u32,
                    y: r.y.max(0) as u32,
                    w: r.w.max(0) as u32,
                    h: r.h.max(0) as u32,
                }]
            }
        } else {
            match (self.prev.as_deref(), self.prev_size) {
                (Some(prev), Some((pw, ph))) => {
                    diff_resize_rects(prev, pw, ph, &pixels, self.width, self.height)
                }
                _ => vec![DirtyRect {
                    x: 0,
                    y: 0,
                    w: self.width,
                    h: self.height,
                }],
            }
        };
        self.prev = Some(pixels.clone());
        self.prev_size = Some((self.width, self.height));
        self.prev_laid = Some(laid);
        Frame {
            width: self.width,
            height: self.height,
            pixels,
            dirty,
        }
    }

    /// Shared full raster: lay out and draw every node over a fresh canvas.
    fn raster_laid(&self, laid: &[LaidOut], ir: &Ir) -> Vec<u8> {
        let pal = Palette::from_theme(&ir.theme);
        let mut canvas = raster::Canvas::new(self.width, self.height, pal.stage);
        canvas.set_edge_aa(self.edge_aa);
        canvas.set_dpr(self.dpr);
        canvas.set_suppress_text(self.suppress_text);
        canvas.set_render_quality(ir.render_quality.as_ref());
        for n in laid {
            raster::draw(&mut canvas, n);
        }
        effects::apply_frame_blurs(&mut canvas, laid);
        canvas.pixels
    }

    /// Press-phase raster: identical to the default pass plus one ripple drawn
    /// for the pressed node after its box, tinted by that node's accent. With
    /// no phase the two loops are the same sequence.
    fn raster_laid_press(
        &self,
        laid: &[LaidOut],
        ir: &Ir,
        press: Option<&PressPhase>,
        elapsed_ms: i64,
    ) -> Vec<u8> {
        let pal = Palette::from_theme(&ir.theme);
        let mut canvas = raster::Canvas::new(self.width, self.height, pal.stage);
        canvas.set_edge_aa(self.edge_aa);
        canvas.set_dpr(self.dpr);
        canvas.set_suppress_text(self.suppress_text);
        let host = press.and_then(|p| press_host(laid, ir, p));
        canvas.set_render_quality(ir.render_quality.as_ref());
        for n in laid {
            // A tappable Text ink host has no authored fill, so Active
            // darkening has nothing to paint. Draw the M3 state layer under
            // the glyph: low-alpha accent, only for a pressed tap-gesture Text.
            if host.as_ref().map_or(false, |(h, _)| h.id == n.id)
                && n.kind == crate::Kind::Text
                && n.state == crate::interaction::NodeState::Active
                && node_has_tap(&ir.tree, n.id)
            {
                let c = n.text_color.unwrap_or(n.accent);
                let layer = Rgba {
                    r: c.r,
                    g: c.g,
                    b: c.b,
                    a: 26, // 10% state layer on primary
                };
                canvas.fill_rounded_radii(n.rect, layer, n.radii);
            }
            raster::draw(&mut canvas, n);
            if let Some((h, hp)) = host.as_ref() {
                if h.id == n.id {
                    canvas.draw_node_ripple(n, hp, elapsed_ms, n.accent);
                }
            }
        }
        effects::apply_frame_blurs(&mut canvas, laid);
        canvas.pixels
    }

    /// Incremental render: reuse the previous frame as the starting buffer and
    /// redraw only the nodes that changed. The output is byte-identical to
    /// `render_at` for the same inputs; `redraw_stats` reports the work that
    /// was actually recomputed.
    ///
    /// The redraw region is the union of the new and previous bounding boxes
    /// of every node whose layout differs from the previous one, expanded by
    /// the deterministic shadow margin, so moved and resized nodes clear their
    /// old footprint too. The returned `dirty` list is the pixel diff between
    /// the previous frame and the incremental result, which is identical to
    /// `render_at`'s list because the pixels are identical. Frames whose layout
    /// carries visual effects or particle fields fall back to a full raster,
    /// because a blurred or backdrop effect reads pixels outside any one
    /// node's box.
    pub fn render_incremental_at(&mut self, ir: &Ir, state: &StateView, elapsed_ms: i64) -> Frame {
        let laid = crate::layout::layout_animated_at(
            ir,
            state,
            self.width,
            self.height,
            elapsed_ms,
            &mut self.anim.borrow_mut(),
        );
        let pal = Palette::from_theme(&ir.theme);
        // A previous frame is reusable only when the viewport is unchanged, so
        // a resize or the first frame still rasterizes the whole surface.
        // A previous press frame painted outside its node box; a full raster
        // is required to clear that ripple even when node boxes did not move.
        let reusable = self.prev_press_disc.is_none()
            && self.prev.is_some()
            && self.prev_laid.is_some()
            && self.prev_size == Some((self.width, self.height));
        self.prev_press_disc = None;
        let (pixels, redrawn_nodes, redraw_pixels) = if !reusable {
            let pixels = self.raster_laid(&laid, ir);
            (pixels, laid.len(), self.width as u64 * self.height as u64)
        } else {
            let prev = self.prev.as_ref().unwrap();
            let prev_laid = self.prev_laid.as_ref().unwrap();
            let region = changed_region(&laid, prev_laid, self.width, self.height);
            if region.w <= 0 || region.h <= 0 {
                // Nothing changed: reuse the buffer without touching it.
                (prev.to_vec(), 0, 0)
            } else if !layout_is_incremental_safe(&laid) {
                let pixels = self.raster_laid(&laid, ir);
                (pixels, laid.len(), self.width as u64 * self.height as u64)
            } else {
                let mut canvas =
                    raster::Canvas::from_pixels(self.width, self.height, prev.to_vec());
                canvas.set_edge_aa(self.edge_aa);
                canvas.set_dpr(self.dpr);
                canvas.set_suppress_text(self.suppress_text);
                canvas.set_render_quality(ir.render_quality.as_ref());
                canvas.clip = Some(region);
                canvas.fill_rect(region, pal.stage);
                let mut redrawn = 0usize;
                for n in &laid {
                    if rect_intersects(n.rect, region) {
                        raster::draw(&mut canvas, n);
                        redrawn += 1;
                    }
                }
                let pixels = canvas.pixels;
                let redraw_pixels = region.w as u64 * region.h as u64;
                (pixels, redrawn, redraw_pixels)
            }
        };

        let dirty = match (self.prev.as_deref(), self.prev_size) {
            (Some(prev), Some((pw, ph))) => {
                diff_resize_rects(prev, pw, ph, &pixels, self.width, self.height)
            }
            _ => vec![DirtyRect {
                x: 0,
                y: 0,
                w: self.width,
                h: self.height,
            }],
        };
        self.redrawn_nodes = redrawn_nodes;
        self.redraw_pixels = redraw_pixels;
        self.prev = Some(pixels.clone());
        self.prev_size = Some((self.width, self.height));
        self.prev_laid = Some(laid);
        Frame {
            width: self.width,
            height: self.height,
            pixels,
            dirty,
        }
    }
}

/// True when every node in the layout draws only within its own box (plus the
/// deterministic shadow margin), so a changed region can be redrawn without
/// reading pixels that live outside every changed node.
fn layout_is_incremental_safe(laid: &[LaidOut]) -> bool {
    laid.iter()
        .all(|n| n.visual_effects.is_empty() && n.particles.is_none())
}

/// Union of two rectangles, accounting for the zero-sized empty rect.
fn union_rect(a: crate::layout::Rect, b: crate::layout::Rect) -> crate::layout::Rect {
    use crate::layout::Rect;
    if b.w <= 0 || b.h <= 0 {
        return a;
    }
    if a.w <= 0 || a.h <= 0 {
        return b;
    }
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Bounding box a node's ripple can cover across its whole lifetime: the disc
/// centred at the touch point (or node centre) with the maximum radius the
/// raster uses. Clamped to the viewport, with margin so the previous frame's
/// disc is fully repainted and cleared.
fn ripple_disc(
    n: &LaidOut,
    touch: Option<(i64, i64)>,
    width: u32,
    height: u32,
) -> crate::layout::Rect {
    use crate::layout::Rect;
    let (cx, cy) = match touch {
        Some((tx, ty)) => (n.rect.x + tx, n.rect.y + ty),
        None => (n.rect.x + n.rect.w / 2, n.rect.y + n.rect.h / 2),
    };
    let r = crate::interaction::ripple_target_radius(n.rect.w, n.rect.h);
    // The raster clips the ripple to the node box (Flutter InkRipple), so the
    // dirty region is the disc intersected with the box, not the whole disc.
    // A small margin covers anti-aliasing at the clipped edge.
    const MARGIN: i64 = 2;
    let x0 = (cx - r - MARGIN).max(n.rect.x).max(0).min(width as i64);
    let y0 = (cy - r - MARGIN).max(n.rect.y).max(0).min(height as i64);
    let x1 = (cx + r + MARGIN)
        .min(n.rect.x + n.rect.w)
        .max(0)
        .min(width as i64);
    let y1 = (cy + r + MARGIN)
        .min(n.rect.y + n.rect.h)
        .max(0)
        .min(height as i64);
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Union of the new and previous bounding boxes of every node whose layout
/// differs, expanded by the shadow margin and clamped to the viewport. An empty
/// rect means no node changed.
fn changed_region(
    laid: &[LaidOut],
    prev_laid: &[LaidOut],
    width: u32,
    height: u32,
) -> crate::layout::Rect {
    use crate::layout::Rect;
    const MARGIN: i64 = 16;
    let mut x0 = i64::MAX;
    let mut y0 = i64::MAX;
    let mut x1 = i64::MIN;
    let mut y1 = i64::MIN;
    let mut grow = |r: crate::layout::Rect| {
        x0 = x0.min(r.x - MARGIN);
        y0 = y0.min(r.y - MARGIN);
        x1 = x1.max(r.x + r.w + MARGIN);
        y1 = y1.max(r.y + r.h + MARGIN);
    };
    // Index both sides by node id: the old nested scans were O(n^2) on every
    // incremental frame. The dirty rect depends only on min/max, so iteration
    // order does not affect the result.
    let prev_by_id: std::collections::HashMap<u32, &LaidOut> =
        prev_laid.iter().map(|p| (p.id, p)).collect();
    let cur_ids: std::collections::HashSet<u32> = laid.iter().map(|n| n.id).collect();
    for n in laid {
        // `phase` carries the animation clock. For every node except a spinner
        // it does not affect drawing, so a clock-only change must not mark the
        // node dirty. Spinners rotate with phase and stay clock-sensitive.
        let unchanged = prev_by_id.get(&n.id).map_or(false, |p| {
            if n.control == Some(Control::Spinner) {
                return *p == n;
            }
            let mut a = n.clone();
            let mut b = (*p).clone();
            a.phase = 0;
            b.phase = 0;
            a == b
        });
        if !unchanged {
            grow(n.rect);
        }
    }
    // Deleted nodes leave a hole the new layout never paints over.
    for p in prev_laid {
        if !cur_ids.contains(&p.id) {
            grow(p.rect);
        }
    }
    if x1 < x0 || y1 < y0 {
        return Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
    }
    Rect {
        x: x0.max(0).min(width as i64),
        y: y0.max(0).min(height as i64),
        w: (x1 - x0).clamp(0, width as i64),
        h: (y1 - y0).clamp(0, height as i64),
    }
}

/// True when two rects overlap.
fn rect_intersects(a: crate::layout::Rect, b: crate::layout::Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// Diff a resized frame against the previous one.
///
/// The shared region (the smaller of both surfaces) is diffed pixel by pixel;
/// the strip that only exists in the new surface is dirty by definition. The
/// result is bounded and covers every changed pixel of the new surface.
fn diff_resize_rects(
    prev: &[u8],
    prev_w: u32,
    prev_h: u32,
    next: &[u8],
    next_w: u32,
    next_h: u32,
) -> Vec<DirtyRect> {
    let full = || {
        vec![DirtyRect {
            x: 0,
            y: 0,
            w: next_w,
            h: next_h,
        }]
    };
    if prev.len() != prev_w as usize * prev_h as usize * 4
        || next.len() != next_w as usize * next_h as usize * 4
    {
        return full();
    }
    let ow = prev_w.min(next_w) as usize;
    let oh = prev_h.min(next_h) as usize;

    // The strip that the new surface gained: new columns and new rows. The
    // two strips are disjoint: the column strip stops at the old height.
    let mut rects = Vec::new();
    if next_w > prev_w {
        rects.push(DirtyRect {
            x: prev_w,
            y: 0,
            w: next_w - prev_w,
            h: prev_h,
        });
    }
    if next_h > prev_h {
        rects.push(DirtyRect {
            x: 0,
            y: prev_h,
            w: next_w,
            h: next_h - prev_h,
        });
    }

    // Diff the shared region only; the strips above are already covered.
    let shared_rects = match diff_strided(prev, prev_w as usize, next, next_w as usize, ow, oh) {
        Some(r) => r,
        // Too many rects: one full-frame rect stays bounded.
        None => return full(),
    };
    rects.extend(shared_rects);
    if rects.len() > MAX_DIRTY {
        return full();
    }
    // Strips are appended before the shared-region rects; emit one canonical
    // order so identical state changes always produce the same rect list.
    rects.sort_by_key(|r| (r.y, r.x, r.h, r.w));
    rects
}

/// Diff two bands of the same height whose rows may have different strides.
/// `cols` is the number of columns both bands share; the rects are returned in
/// the coordinate system of the new band.
///
/// Rows and columns are coalesced into a list of rects that covers every
/// changed pixel. `None` means the list grew past the bound and the caller
/// must fall back to one full-frame rect.
fn diff_strided(
    prev: &[u8],
    prev_stride: usize,
    next: &[u8],
    next_stride: usize,
    cols: usize,
    rows: usize,
) -> Option<Vec<DirtyRect>> {
    let w = cols;
    let h = rows;
    let mut row_min = vec![u32::MAX; h];
    let mut row_max = vec![0u32; h];
    let mut row_any = vec![false; h];
    let mut col_min = vec![u32::MAX; w];
    let mut col_max = vec![0u32; w];
    let mut col_any = vec![false; w];

    // Rows are compared in machine-word chunks first, then only the mismatched
    // words are expanded to per-pixel updates. One `usize` covers 2 pixels on
    // 32-bit and 4 on 64-bit hosts; identical rows cost a handful of loads.
    // Byte order never matters: two equal words always compare equal and two
    // different words always compare different, so the set of dirty pixels is
    // exactly the scalar result.
    const WORD: usize = std::mem::size_of::<usize>();
    let words_per_row = w * 4 / WORD;
    let tail = (w * 4) % WORD;
    for y in 0..h {
        let base = y * next_stride * 4;
        let pbase = y * prev_stride * 4;
        for wi in 0..words_per_row {
            let a = pbase + wi * WORD;
            let b = base + wi * WORD;
            let pv = usize::from_ne_bytes(prev[a..a + WORD].try_into().unwrap());
            let nv = usize::from_ne_bytes(next[b..b + WORD].try_into().unwrap());
            if pv == nv {
                continue;
            }
            // XOR of the two words: every set bit is a differing byte.
            let mut diff = pv ^ nv;
            let first_px = wi * (WORD / 4);
            while diff != 0 {
                let byte_index = diff.trailing_zeros() as usize / 8;
                let xu = first_px + byte_index / 4;
                if xu < w {
                    let (xu, yu) = (xu as u32, y as u32);
                    if !row_any[y] {
                        row_any[y] = true;
                        row_min[y] = xu;
                        row_max[y] = xu;
                    } else {
                        row_min[y] = row_min[y].min(xu);
                        row_max[y] = row_max[y].max(xu);
                    }
                    if !col_any[xu as usize] {
                        col_any[xu as usize] = true;
                        col_min[xu as usize] = yu;
                        col_max[xu as usize] = yu;
                    } else {
                        col_min[xu as usize] = col_min[xu as usize].min(yu);
                        col_max[xu as usize] = col_max[xu as usize].max(yu);
                    }
                }
                diff &= diff - 1;
            }
        }
        if tail > 0 {
            let a = pbase + words_per_row * WORD;
            let b = base + words_per_row * WORD;
            if next[b..b + tail] != prev[a..a + tail] {
                for k in 0..tail {
                    if next[b + k] != prev[a + k] {
                        let xu = (words_per_row * WORD + k) / 4;
                        if xu < w {
                            let (xu, yu) = (xu as u32, y as u32);
                            if !row_any[y] {
                                row_any[y] = true;
                                row_min[y] = xu;
                                row_max[y] = xu;
                            } else {
                                row_min[y] = row_min[y].min(xu);
                                row_max[y] = row_max[y].max(xu);
                            }
                            if !col_any[xu as usize] {
                                col_any[xu as usize] = true;
                                col_min[xu as usize] = yu;
                                col_max[xu as usize] = yu;
                            } else {
                                col_min[xu as usize] = col_min[xu as usize].min(yu);
                                col_max[xu as usize] = col_max[xu as usize].max(yu);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut rects = Vec::new();
    let mut y = 0usize;
    while y < h {
        if !row_any[y] {
            y += 1;
            continue;
        }
        let band_start = y;
        let mut xmin = usize::MAX;
        let mut xmax = 0usize;
        while y < h && row_any[y] {
            xmin = xmin.min(row_min[y] as usize);
            xmax = xmax.max(row_max[y] as usize);
            y += 1;
        }
        let band_end = y; // exclusive
        let bs = band_start as u32;
        let be = (band_end - 1) as u32;
        let band_h = (band_end - band_start) as u32;

        let active = |x: usize| col_any[x] && col_min[x] <= be && col_max[x] >= bs;
        let mut x = xmin;
        while x <= xmax {
            while x <= xmax && !active(x) {
                x += 1;
            }
            if x > xmax {
                break;
            }
            let run_start = x;
            let mut run_end = x;
            let mut gap = 0usize;
            while x <= xmax {
                if active(x) {
                    run_end = x;
                    gap = 0;
                } else {
                    gap += 1;
                    if gap > 8 {
                        break; // merge small gaps, split large empty stretches
                    }
                }
                x += 1;
            }
            rects.push(DirtyRect {
                x: run_start as u32,
                y: bs,
                w: (run_end - run_start + 1) as u32,
                h: band_h,
            });
        }
    }

    if rects.len() > MAX_DIRTY {
        return None;
    }
    Some(rects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(w: u32, h: u32) -> Vec<u8> {
        vec![0u8; w as usize * h as usize * 4]
    }

    /// Paint one changed region into a strided RGBA buffer.
    fn paint(buf: &mut [u8], stride: usize, r: (u32, u32, u32, u32)) {
        let (x, y, w, h) = r;
        for yy in y..y + h {
            for xx in x..x + w {
                buf[(yy as usize * stride + xx as usize) * 4] = 200;
            }
        }
    }

    fn covered(rects: &[DirtyRect], x: u32, y: u32) -> bool {
        rects
            .iter()
            .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
    }

    /// The emitted rects must cover the union of every input region.
    fn assert_covers(rects: &[DirtyRect], inputs: &[(u32, u32, u32, u32)]) {
        for &r in inputs {
            let (x, y, w, h) = r;
            for yy in y..y + h {
                for xx in x..x + w {
                    assert!(
                        covered(rects, xx, yy),
                        "({xx},{yy}) of {r:?} not covered by {rects:?}"
                    );
                }
            }
        }
    }

    fn is_canonical(rects: &[DirtyRect]) -> bool {
        rects
            .windows(2)
            .all(|p| (p[0].y, p[0].x, p[0].h, p[0].w) <= (p[1].y, p[1].x, p[1].h, p[1].w))
    }

    #[test]
    fn fnv_vectors() {
        assert_eq!(digest(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(digest_hex(b"foobar"), "85944171f73967e8");
        assert_eq!(digest_hex(b"foobar"), digest_hex(b"foobar"));
        assert_ne!(digest(b"foobar"), digest(b"foobas"));
    }

    #[test]
    fn hex_color_parsing() {
        assert_eq!(Rgba::from_hex("#4F46E5"), Some(Rgba::rgb(0x4f, 0x46, 0xe5)));
        assert_eq!(Rgba::from_hex("#4f46e5"), Some(Rgba::rgb(0x4f, 0x46, 0xe5)));
        assert_eq!(Rgba::from_hex("#4F46E5FF"), None);
        assert_eq!(Rgba::from_hex("4F46E5"), None);
        assert_eq!(Rgba::from_hex("#GGGGGG"), None);
    }

    #[test]
    fn dirty_rects_cover_changes_and_vanish_when_equal() {
        let (w, h) = (16u32, 8u32);
        let a = vec![0u8; w as usize * h as usize * 4];
        let mut b = a.clone();
        for y in 2..5 {
            for x in 3..7 {
                let i = (y * w as usize + x) * 4;
                b[i] = 200;
            }
        }
        assert!(
            diff_strided(&a, w as usize, &a, w as usize, w as usize, h as usize)
                .unwrap()
                .is_empty()
        );
        let rects = diff_strided(&a, w as usize, &b, w as usize, w as usize, h as usize).unwrap();
        assert!(!rects.is_empty());
        assert!(rects.len() <= MAX_DIRTY);
        for y in 0..h {
            for x in 0..w {
                let i = (y as usize * w as usize + x as usize) * 4;
                if a[i..i + 4] != b[i..i + 4] {
                    assert!(
                        rects
                            .iter()
                            .any(|r| { x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h }),
                        "changed pixel ({x},{y}) not covered by {rects:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn overlapping_adjacent_and_contained_rects_merge_deterministically() {
        let (w, h) = (64u32, 32u32);
        let a = blank(w, h);
        let mut b = a.clone();
        let inputs = [
            (4, 4, 8, 6),  // base
            (8, 6, 8, 6),  // overlaps the base
            (12, 4, 4, 6), // touches the base on its right edge
            (6, 5, 2, 2),  // contained in the base
            (4, 10, 4, 2), // touches the base on its bottom edge
            (20, 4, 3, 3), // a small gap away
        ];
        for &r in &inputs {
            paint(&mut b, w as usize, r);
        }
        let rects = diff_strided(&a, w as usize, &b, w as usize, w as usize, h as usize).unwrap();
        let again = diff_strided(&a, w as usize, &b, w as usize, w as usize, h as usize).unwrap();
        assert_eq!(rects, again, "identical diffs must emit identical lists");
        assert!(!rects.is_empty());
        assert!(
            rects.len() < inputs.len(),
            "overlapping inputs must merge: {rects:?}"
        );
        assert!(rects.len() <= MAX_DIRTY);
        assert_covers(&rects, &inputs);
        assert!(is_canonical(&rects), "not ordered by (y, x): {rects:?}");
    }

    #[test]
    fn many_small_rects_cover_their_union_within_the_cap() {
        let (w, h) = (96u32, 96u32);
        let a = blank(w, h);
        let mut b = a.clone();
        let mut inputs = Vec::new();
        for i in 0..120u32 {
            let r = (4 + (i % 12) * 8, 4 + (i / 12) * 8, 2, 2);
            paint(&mut b, w as usize, r);
            inputs.push(r);
        }
        let rects = diff_strided(&a, w as usize, &b, w as usize, w as usize, h as usize).unwrap();
        assert!(rects.len() <= MAX_DIRTY, "{} rects", rects.len());
        assert!(rects.len() < inputs.len(), "small rects must coalesce");
        assert_covers(&rects, &inputs);
        assert!(is_canonical(&rects), "not ordered by (y, x): {rects:?}");
        for r in &rects {
            assert!(r.w > 0 && r.h > 0 && r.x + r.w <= w && r.y + r.h <= h);
        }
    }

    #[test]
    fn too_many_small_rects_fall_back_to_one_full_frame_rect() {
        let (w, h) = (100u32, 100u32);
        let a = blank(w, h);
        let mut b = a.clone();
        let mut inputs = Vec::new();
        for i in 0..9u32 {
            for j in 0..9u32 {
                let r = (1 + j * 11, 1 + i * 11, 1, 1);
                paint(&mut b, w as usize, r);
                inputs.push(r);
            }
        }
        // 81 separated runs exceed the cap, so the shared diff gives up.
        assert!(diff_strided(&a, w as usize, &b, w as usize, w as usize, h as usize).is_none());
        let rects = diff_resize_rects(&a, w, h, &b, w, h);
        assert_eq!(rects, vec![DirtyRect { x: 0, y: 0, w, h }]);
        assert!(rects.len() <= MAX_DIRTY);
        assert_covers(&rects, &inputs);
    }

    #[test]
    fn resize_strips_and_shared_rects_together_respect_the_cap() {
        let (pw, ph) = (100u32, 100u32);
        let (nw, nh) = (120u32, 120u32);
        let a = blank(pw, ph);
        let mut b = blank(nw, nh);
        for i in 0..8u32 {
            for j in 0..8u32 {
                paint(&mut b, nw as usize, (1 + j * 11, 1 + i * 11, 1, 1));
            }
        }
        let shared = diff_strided(&a, pw as usize, &b, nw as usize, pw as usize, ph as usize)
            .expect("64 shared rects still fit the cap");
        assert_eq!(
            shared.len(),
            MAX_DIRTY,
            "shared diff sits exactly at the cap"
        );
        // Two gained strips push the combined list past the cap.
        let rects = diff_resize_rects(&a, pw, ph, &b, nw, nh);
        assert_eq!(
            rects,
            vec![DirtyRect {
                x: 0,
                y: 0,
                w: nw,
                h: nh
            }]
        );
        assert!(rects.len() <= MAX_DIRTY);
    }

    #[test]
    fn emitted_rects_are_canonically_ordered_and_cover_a_resize() {
        let (pw, ph) = (40u32, 30u32);
        let (nw, nh) = (48u32, 36u32);
        let a = blank(pw, ph);
        let mut b = blank(nw, nh);
        let inputs = [(2, 2, 3, 3), (5, 4, 3, 3), (20, 12, 4, 4), (2, 20, 6, 2)];
        for &r in &inputs {
            paint(&mut b, nw as usize, r);
        }
        let rects = diff_resize_rects(&a, pw, ph, &b, nw, nh);
        assert!(rects.len() > 1, "a resize must report strips plus changes");
        assert!(is_canonical(&rects), "not ordered by (y, x): {rects:?}");
        assert!(rects.len() <= MAX_DIRTY);
        assert_covers(&rects, &inputs);
        // A changed pixel or a newly exposed pixel is dirty by definition.
        for y in 0..nh {
            for x in 0..nw {
                let i = (y as usize * nw as usize + x as usize) * 4;
                let dirty = if x < pw && y < ph {
                    let o = (y as usize * pw as usize + x as usize) * 4;
                    a[o..o + 4] != b[i..i + 4]
                } else {
                    true
                };
                if dirty {
                    assert!(covered(&rects, x, y), "({x},{y}) not covered by {rects:?}");
                }
            }
        }
    }
}

#[cfg(test)]
mod perf_row_diff_tests {
    use super::{diff_strided, digest_hex, DirtyRect};

    fn scalar_cover(a: &[u8], b: &[u8], w: usize, h: usize) -> Vec<DirtyRect> {
        // Reference: the row/col min-max coalescing over a scalar compare.
        let mut row_min = vec![u32::MAX; h];
        let mut row_max = vec![0u32; h];
        let mut row_any = vec![false; h];
        let mut col_min = vec![u32::MAX; w];
        let mut col_max = vec![0u32; w];
        let mut col_any = vec![false; w];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                if a[i..i + 4] != b[i..i + 4] {
                    let (xu, yu) = (x as u32, y as u32);
                    if !row_any[y] {
                        row_any[y] = true;
                        row_min[y] = xu;
                        row_max[y] = xu;
                    } else {
                        row_min[y] = row_min[y].min(xu);
                        row_max[y] = row_max[y].max(xu);
                    }
                    if !col_any[x] {
                        col_any[x] = true;
                        col_min[x] = yu;
                        col_max[x] = yu;
                    } else {
                        col_min[x] = col_min[x].min(yu);
                        col_max[x] = col_max[x].max(yu);
                    }
                }
            }
        }
        // Reuse the same band coalescing by calling the vectorized function on
        // the same inputs and comparing coverage, not the rect list.
        let rects = diff_strided(a, w, b, w, w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let changed = a[i..i + 4] != b[i..i + 4];
                let covered = rects.iter().any(|r| {
                    x >= r.x as usize
                        && x < (r.x + r.w) as usize
                        && y >= r.y as usize
                        && y < (r.y + r.h) as usize
                });
                assert!(
                    !changed || covered,
                    "changed pixel ({x},{y}) must be covered"
                );
            }
        }
        row_min.clear();
        row_max.clear();
        col_min.clear();
        col_max.clear();
        row_any.clear();
        col_any.clear();
        let _ = digest_hex(&a);
        rects
    }

    #[test]
    fn vectorized_row_diff_matches_scalar_coverage() {
        // Deterministic pseudo-random frames with a handful of changed pixels.
        let (w, h) = (64usize, 32usize);
        let mut a = vec![0u8; w * h * 4];
        let mut b = a.clone();
        let mut seed = 0x12345678u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u8
        };
        for i in 0..a.len() {
            a[i] = next();
        }
        b.copy_from_slice(&a);
        for _ in 0..64 {
            let i = (next() as usize) % (w * h);
            let off = (next() as usize) % 4;
            b[i * 4 + off] = next();
        }
        scalar_cover(&a, &b, w, h);
    }

    #[test]
    fn unchanged_frames_produce_no_dirty_rects() {
        let (w, h) = (33usize, 17usize);
        let mut a = vec![0u8; w * h * 4];
        let mut seed = 0xdeadbeefu64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u8
        };
        for i in 0..a.len() {
            a[i] = next();
        }
        let rects = diff_strided(&a, w, &a, w, w, h).unwrap();
        assert!(rects.is_empty());
    }
}
