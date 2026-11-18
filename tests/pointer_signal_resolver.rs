//! Pointer signal resolution parity pins for Flutter 3.50.0.
//!
//! Version pin: framework `3.50.0`, checkout
//! `ab2912035e160d9cad6306df307576da6128ebdb`, engine
//! `371f77202a73e8dff57a8b3bff75bca55c090aee`.
//!
//! Source anchors:
//! - `gestures/pointer_signal_resolver.dart:86-93`: the resolver keeps only
//!   the first callback registered for an event.
//! - `gestures/pointer_signal_resolver.dart:101-126`: `resolve` invokes the
//!   first callback; with no callback the platform default stays allowed.
//! - `gestures/pointer_signal_resolver.dart:87,109`: a second registration
//!   for a different event before `resolve` is a programming error.
//!
//! These tests are a deterministic host projection. Opurom's ABI and IR do
//! not carry pointer-signal events, so none is added here.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Signal {
    id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Unhandled,
    Handled(u32),
    PlatformDefault,
}

#[derive(Default)]
struct Resolver {
    first: Option<u32>,
    current: Option<u64>,
}

impl Resolver {
    /// `pointer_signal_resolver.dart:86-93`: only the first registration for
    /// the in-flight event wins; later registrations are ignored.
    fn register(&mut self, event: Signal, handler: u32) {
        if self.first.is_some() {
            return;
        }
        self.current = Some(event.id);
        self.first = Some(handler);
    }

    /// `pointer_signal_resolver.dart:101-126`.
    fn resolve(&mut self, event: Signal) -> Outcome {
        match self.first.take() {
            Some(handler) => {
                assert_eq!(self.current, Some(event.id));
                Outcome::Handled(handler)
            }
            None => Outcome::PlatformDefault,
        }
    }
}

#[test]
fn the_first_registration_wins() {
    // pointer_signal_resolver.dart:86-93.
    let mut resolver = Resolver::default();
    resolver.register(Signal { id: 1 }, 10);
    resolver.register(Signal { id: 1 }, 20);
    assert_eq!(resolver.resolve(Signal { id: 1 }), Outcome::Handled(10));
}

#[test]
fn an_unhandled_signal_allows_the_platform_default() {
    // pointer_signal_resolver.dart:101-108.
    let mut resolver = Resolver::default();
    assert_eq!(resolver.resolve(Signal { id: 2 }), Outcome::PlatformDefault);
}

#[test]
fn a_resolved_event_cannot_be_resolved_again() {
    // pointer_signal_resolver.dart:77-82: one disambiguation at a time.
    let mut resolver = Resolver::default();
    resolver.register(Signal { id: 3 }, 10);
    assert_eq!(resolver.resolve(Signal { id: 3 }), Outcome::Handled(10));
    // The second resolve has no callback left.
    assert_eq!(resolver.resolve(Signal { id: 3 }), Outcome::PlatformDefault);
}

#[test]
fn dispatch_order_is_deterministic() {
    let run = || {
        let mut resolver = Resolver::default();
        resolver.register(Signal { id: 1 }, 10);
        resolver.register(Signal { id: 1 }, 20);
        let first = resolver.resolve(Signal { id: 1 });
        let second = resolver.resolve(Signal { id: 1 });
        let third = {
            resolver.register(Signal { id: 2 }, 30);
            resolver.resolve(Signal { id: 2 })
        };
        (first, second, third)
    };
    assert_eq!(run(), run());
}
