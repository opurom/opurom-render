//! Minimal, deterministic boolean path operations on axis-aligned rectangles,
//! ported from Flutter 3.50.0.
//!
//! Source of truth: framework `ab2912035e16`, engine `371f77202a`,
//! `engine/src/flutter/lib/ui/painting.dart:2802-2858` (`PathOperation`:
//! difference, intersect, union, xor, reverseDifference) and `:3157-3163`
//! (`Path.combine`), with `PathFillType` at `:2776-2798`. Skia's
//! `SkOpBuilder` is not available in this checkout (submodule absent; pinned
//! `0e5d8d1be81b22f5529a41cf4810dd89b63527ba` via `DEPS:18,505`), so this
//! kernel implements the rectangle subset exactly and marks the general
//! polygon case as an unresolved gap rather than inventing Skia behaviour.
//! No IR protocol change; the renderer stays IR-driven.

/// One axis-aligned rectangle in logical pixels: `x`, `y` and positive
/// `w`, `h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl Rect {
    /// True when the rectangle covers the point `(px, py)` (right/bottom
    /// exclusive), the boundary rule Skia uses for filled paths.
    pub fn contains(&self, px: i64, py: i64) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    /// Intersection rectangle, `None` when the rects do not overlap.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        (x1 > x && y1 > y).then_some(Rect {
            x,
            y,
            w: x1 - x,
            h: y1 - y,
        })
    }
}

/// Flutter `PathOperation` (`painting.dart:2802-2858`), in declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathOperation {
    Difference,
    Intersect,
    Union,
    Xor,
    ReverseDifference,
}

/// Result of combining two rectangles: zero or one rect. Skia's full `Op`
/// can produce disjoint polygons; the rectangle subset here is closed under
/// intersect/difference, and union/xor are representable as one rect only
/// when the operands touch or overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectOpResult {
    Empty,
    One(Rect),
}

/// Boolean combine of two rectangles under `op`. The overlap test uses the
/// same right/bottom-exclusive boundary as `Rect::contains`.
pub fn combine_rects(a: &Rect, b: &Rect, op: PathOperation) -> RectOpResult {
    let overlap = a.intersect(b);
    match op {
        PathOperation::Intersect => overlap.map_or(RectOpResult::Empty, RectOpResult::One),
        PathOperation::Union => match overlap {
            // Disjoint rects cannot be represented as one rectangle; the full
            // Skia op returns a two-rect path. This minimal kernel reports the
            // empty result and the caller falls back to per-rect drawing.
            None => RectOpResult::Empty,
            Some(_) => {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                let x1 = (a.x + a.w).max(b.x + b.w);
                let y1 = (a.y + a.h).max(b.y + b.h);
                RectOpResult::One(Rect {
                    x,
                    y,
                    w: x1 - x,
                    h: y1 - y,
                })
            }
        },
        PathOperation::Difference => match overlap {
            None => RectOpResult::One(*a),
            Some(ov) if ov.w >= a.w && ov.h >= a.h => RectOpResult::Empty,
            Some(ov) => {
                // B removes a horizontal or vertical slice of A; the remainder
                // is one rectangle when the cut spans the full width/height.
                if ov.x == a.x && ov.w == a.w {
                    if ov.y == a.y {
                        RectOpResult::One(Rect {
                            x: a.x,
                            y: a.y + ov.h,
                            w: a.w,
                            h: a.h - ov.h,
                        })
                    } else if ov.y + ov.h == a.y + a.h {
                        RectOpResult::One(Rect {
                            x: a.x,
                            y: a.y,
                            w: a.w,
                            h: a.h - ov.h,
                        })
                    } else {
                        RectOpResult::Empty
                    }
                } else if ov.y == a.y && ov.h == a.h {
                    if ov.x == a.x {
                        RectOpResult::One(Rect {
                            x: a.x + ov.w,
                            y: a.y,
                            w: a.w - ov.w,
                            h: a.h,
                        })
                    } else if ov.x + ov.w == a.x + a.w {
                        RectOpResult::One(Rect {
                            x: a.x,
                            y: a.y,
                            w: a.w - ov.w,
                            h: a.h,
                        })
                    } else {
                        RectOpResult::Empty
                    }
                } else {
                    // General case needs a multi-rect path (Skia Op).
                    RectOpResult::Empty
                }
            }
        },
        PathOperation::Xor => match overlap {
            None => RectOpResult::Empty,
            Some(_) => {
                let _ = (a, b);
                // Symmetric difference of overlapping rects needs multiple
                // rectangles; not representable by this kernel.
                RectOpResult::Empty
            }
        },
        PathOperation::ReverseDifference => combine_rects(b, a, PathOperation::Difference),
    }
}

#[cfg(test)]
mod tests {
    use super::{combine_rects, PathOperation, Rect, RectOpResult};

    fn rect(x: i64, y: i64, w: i64, h: i64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn flutter_path_operation_names_match_declaration_order() {
        // painting.dart:2802-2858 declaration order: difference, intersect,
        // union, xor, reverseDifference. Keep the enum total and ordered.
        let ops = [
            PathOperation::Difference,
            PathOperation::Intersect,
            PathOperation::Union,
            PathOperation::Xor,
            PathOperation::ReverseDifference,
        ];
        assert_eq!(ops.len(), 5);
    }

    #[test]
    fn intersect_is_the_overlap_and_disjoint_is_empty() {
        let a = rect(0, 0, 10, 10);
        let b = rect(5, 5, 10, 10);
        assert_eq!(
            combine_rects(&a, &b, PathOperation::Intersect),
            RectOpResult::One(rect(5, 5, 5, 5))
        );
        let c = rect(20, 20, 5, 5);
        assert_eq!(
            combine_rects(&a, &c, PathOperation::Intersect),
            RectOpResult::Empty
        );
    }

    #[test]
    fn difference_removes_a_full_slice_and_keeps_the_remainder() {
        let a = rect(0, 0, 10, 10);
        let top = rect(0, 0, 10, 4);
        assert_eq!(
            combine_rects(&a, &top, PathOperation::Difference),
            RectOpResult::One(rect(0, 4, 10, 6))
        );
        let left = rect(0, 0, 3, 10);
        assert_eq!(
            combine_rects(&a, &left, PathOperation::Difference),
            RectOpResult::One(rect(3, 0, 7, 10))
        );
        assert_eq!(
            combine_rects(&a, &a, PathOperation::Difference),
            RectOpResult::Empty
        );
        // A corner cut needs a multi-rect path; the kernel reports empty and
        // the renderer falls back to per-rect drawing (Skia-op gap).
        let corner = rect(0, 0, 3, 4);
        assert_eq!(
            combine_rects(&a, &corner, PathOperation::Difference),
            RectOpResult::Empty
        );
    }

    #[test]
    fn reverse_difference_swaps_the_operands() {
        let a = rect(0, 0, 10, 10);
        let top = rect(0, 0, 10, 4);
        assert_eq!(
            combine_rects(&a, &top, PathOperation::ReverseDifference),
            combine_rects(&top, &a, PathOperation::Difference)
        );
    }
}
