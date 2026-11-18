# Scale acceptance gate source trace

Flutter 3.50.0 (`ab2912035e160d9cad6306df307576da6128ebdb`),
`packages/flutter/lib/src/gestures/scale.dart`
(blob `fe3cdf27a414de2f112c58ad5d633831f7f1536e`).

## The exact condition

`scale.dart:738-750` accepts the scale recognizer when any of:

1. `|currentSpan - initialSpan| > computeScaleSlop(event.kind)`;
2. `distance(currentFocalPoint - initialFocalPoint) > computePanSlop(event.kind, gestureSettings)`;
3. `max(_scaleFactor / _pointerScaleFactor,
        _pointerScaleFactor / _scaleFactor) > 1.05`.

## Why clause 3 is inert for touch pointer events

`scale.dart:455`:

```dart
double get _pointerScaleFactor => _initialSpan > 0.0 ? _currentSpan / _initialSpan : 1.0;
```

`scale.dart:463-469`:

```dart
double get _scaleFactor {
  double scale = _pointerScaleFactor;
  for (final _PointerPanZoomData p in _pointerPanZooms.values) {
    scale *= p.scale / _initialPanZoomScaleFactor;
  }
  return scale;
}
```

`_pointerPanZooms` is populated only by the pan/zoom `addAllowedPointerPanZoom`
path (`scale.dart:168,532`); ordinary touch pointers use `addAllowedPointer`
(`:513`) and do not enter that map. When the map is empty, `_scaleFactor` is
exactly `_pointerScaleFactor`, so both ratios in clause 3 are exactly `1.0`
and `1.0 > 1.05` is always false.

Clause 3 exists for trackpad pan/zoom events that carry their own `scale`
(`_PointerPanZoomData.scale`, `scale.dart:59-63,78-87`), not for touch spans.

## Opurom position

Opurom's `PointerEvent` has no pan/zoom channel, so modelling clause 3 with
the touch span ratio would invent a condition that Flutter never applies to
touch. The earlier attempt (`61d9dc7`) did exactly that and broke three scale
tests; it was reverted in `68c7c3d` for that reason. The current engine keeps
clauses 1 and 2 only (`ScaleTrack::exceeded`, `src/gestures.rs:455-464`),
which is the complete rule for the pointer events Opurom can receive.

The integer fixed-point scale value (`ScaleTrack::scale`, `SCALE_UNIT = 1024`)
is still reported in the `Gesture::Scale` update; it is derived from
`scale.dart:455` but is not an acceptance gate, matching Flutter.

## Verification pins

- `tests/scale_gesture.rs`: span slop, focal slop, first-accept and
  two-pointer scale ratio values.
- `tests/advanced_gesture_parity.rs` and `tests/rotation_callback.rs`: a pure
  rotation that preserves span and focal does not fabricate a scale.
- `tests/gesture_advanced_report.rs`: machine-readable rotation and drag-start
  fields consumed by `gesture-gate/gesture-advanced-gate.mjs`.
