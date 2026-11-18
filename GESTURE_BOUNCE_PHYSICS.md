# iOS bounce scroll physics

`opurom-render/src/scroll_physics.rs` implements the iOS ballistic scroll model
next to the existing Android clamping model. Closed-form only: no frame
stepping, no wall clock, no IR or native ABI change. Units are logical pixels,
seconds and pixels per second.

Flutter 3.50.0 (`ab2912035e160d9cad6306df307576da6128ebdb`) anchors:

- `packages/flutter/lib/src/widgets/scroll_simulation.dart:18-132`
  (`BouncingScrollSimulation`): `:43-48` a release outside the extents springs
  back, `:50-57` the iOS friction coefficient `0.135` (taken from
  `UIScrollView.decelerationRate`, `0.998^1000`), `:59-72` the handover from
  the friction phase to the spring phase at an extent, `:79-81`
  `maxSpringTransferVelocity = 5000`, `:99-126` the phase selection and its
  time offset.
- `packages/flutter/lib/src/physics/friction_simulation.dart:35-165`:
  `:51-57` the Newton stop time, `:117-134` the closed-form position and
  velocity, `:136-141` `finalX`, `:144-155` `timeAtX`, `:157-160` `isDone`.
- `packages/flutter/lib/src/widgets/scroll_physics.dart:748-946`
  (`BouncingScrollPhysics`): `:759-765` the deceleration rate and the rubber
  band decay constant, `:796-800` the rubber band spring, `:878-899` the
  ballistic simulation, `:904-905` the doubled minimum fling velocity,
  `:932-945` the rate dependent spring and maximum fling velocity.
- `packages/flutter/lib/src/physics/spring_simulation.dart:265-280,284-392`:
  the analytic spring solutions and the scroll snap to end.

## Model

1. A fling that starts inside the scroll extents decelerates with the iOS
   friction model, `x(t) = x0 + v0 * drag^t / log(drag) - v0 / log(drag)`,
   `dx(t) = v0 * drag^t`, `drag = 0.135`.
2. When the friction phase would carry the position past an extent, the
   simulation hands over to a spring at that extent. The velocity handed to
   the spring is capped at `5000` pixels per second, so a hard fling cannot
   turn into an unbounded stretch.
3. A release that starts outside the extents springs straight back to the
   nearest extent.
4. A stationary release outside the extents uses the rubber band spring: mass
   `1`, stiffness `1e5 * lambda^2`, damping `(1e5 + 1) * lambda` with
   `lambda = log(2) / 0.07`. The visible decay is `exp(-lambda * t)`, so the
   overshoot halves every 70 ms.

## API

- `FrictionSimulation::new(drag, position, velocity, constant_deceleration, tolerance)`
- `BouncingScrollSimulation::new(position, velocity, leading_extent, trailing_extent, spring, constant_deceleration, tolerance)`
- `ScrollDecelerationRate::{Normal, Fast}` with the Flutter rate dependent
  spring, constant deceleration, and fling velocity limits.
- `SpringDescription::{ios_default, ios_fast, ios_rubber_band}`
- `ScrollDriver::begin_bounce(now_ms, x0, v0, min, max, dpr, rate)` selects the
  motion, and `ScrollMotion::Bouncing` carries it.

A bounce keeps the overshoot: `ScrollDriver::offset_px` does not clamp a
bouncing motion to the scroll extents, matching Flutter, where the overscroll
is visible until the spring settles.

## Deviation notes

- Flutter's `timeAtX` Newton iteration uses the simulated velocity as the
  derivative, which is `log(drag)` times the analytic slope. The port keeps
  that form so the iteration converges to the same root.
- Cross-target libm bit equality is not claimed; the formulas are the same and
  positions are rounded once at the integer pixel boundary.

## Verification

`opurom-render/tests/bounce_scroll_physics.rs` pins the constants, the rubber
band half life, the friction closed form, the friction to spring handover, the
transfer velocity cap, the driver entry points and determinism.
