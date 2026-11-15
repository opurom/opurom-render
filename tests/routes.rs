//! Multi-page routes: parsing, per-route rendering, navigation stack wiring,
//! and the single-page backward-compatibility guard.

use opurom_render::{
    digest_hex, parse_ir, Navigator, Op, Renderer, StateView, TransitionKind, ONE, TRANSITION_MS,
};

// Home pushes Detail through a Button tap; Detail pops with `pop`. Detail
// numbers its nodes from its own root id (2) to prove per-route id seeding.
const ROUTES_IR: &str = r##"{
  "ir": "0.1.0", "app": "Multi",
  "state": [{"name": "count", "type": "int", "init": 0}],
  "initialRoute": "Home",
  "routes": {
    "Detail": {
      "tree": {
        "id": 2, "kind": "Column", "props": {}, "events": {},
        "children": [
          {"id": 3, "kind": "Text", "props": {"value": "Detail"}, "children": [], "events": {}},
          {"id": 4, "kind": "Button", "props": {"value": "Back"}, "children": [],
           "events": {"tap": {"op": "pop"}}}
        ]
      }
    },
    "Home": {
      "title": "Home",
      "tree": {
        "id": 0, "kind": "Column", "props": {}, "events": {},
        "children": [
          {"id": 1, "kind": "Text", "props": {"value": "Home {count}"}, "children": [], "events": {}},
          {"id": 2, "kind": "Button", "props": {"value": "Open"}, "children": [],
           "events": {"tap": {"op": "push", "target": "Detail"}}}
        ]
      }
    }
  },
  "theme": {"mode": "light", "accent": "#097FEB", "radius": 8}
}"##;

const SINGLE_IR: &str = r##"{
  "ir": "0.1.0", "app": "Single",
  "state": [],
  "tree": {"id": 0, "kind": "Text", "props": {"value": "one page"}, "children": [], "events": {}},
  "theme": {"mode": "light", "accent": "#097FEB", "radius": 8}
}"##;

#[test]
fn routes_parse_and_the_initial_route_is_the_root_tree() {
    let ir = parse_ir(ROUTES_IR).unwrap();
    assert_eq!(ir.routes.len(), 2);
    assert_eq!(ir.initial_route.as_deref(), Some("Home"));
    // `tree` mirrors the initial route, so single-tree consumers render the
    // entry page without knowing about routes.
    assert_eq!(ir.tree.kind.name(), "Column");
    assert_eq!(ir.tree.children[0].kind.name(), "Text");
    // Route titles are preserved; an absent title stays None.
    assert_eq!(ir.route("Home").unwrap().title.as_deref(), Some("Home"));
    assert_eq!(ir.route("Detail").unwrap().title, None);
    // Route trees seed their ids from their own root, so independent
    // numbering schemes all parse.
    assert_eq!(ir.route("Home").unwrap().tree.id, 0);
    assert_eq!(ir.route("Detail").unwrap().tree.id, 2);
    assert_eq!(ir.route("Detail").unwrap().tree.children[1].id, 4);
}

#[test]
fn the_initial_route_renders_deterministically() {
    let ir = parse_ir(ROUTES_IR).unwrap();
    let state = StateView::from_ir(&ir);
    let first = Renderer::new(160, 240).render(&ir, &state);
    let second = Renderer::new(160, 240).render(&ir, &state);
    assert_eq!(first.pixels.len(), 160 * 240 * 4);
    assert_eq!(digest_hex(&first.pixels), digest_hex(&second.pixels));
    // The mirrored tree and `with_route(initial)` render identically.
    let via_helper = Renderer::new(160, 240).render(&ir.with_route("Home").unwrap(), &state);
    assert_eq!(digest_hex(&via_helper.pixels), digest_hex(&first.pixels));
}

#[test]
fn push_renders_the_new_route_and_pop_returns_home() {
    let ir = parse_ir(ROUTES_IR).unwrap();
    let state = StateView::from_ir(&ir);
    let home_digest = digest_hex(&Renderer::new(160, 240).render(&ir, &state).pixels);

    let mut nav = Navigator::with_root(ir.initial_route.clone().unwrap(), TransitionKind::Slide);
    ir.check_push_target("Detail").unwrap();
    let started = nav
        .apply_nav("Detail")
        .unwrap()
        .expect("push starts a transition");
    assert_eq!(started.duration_ms, TRANSITION_MS);
    assert_eq!(nav.stack(), ["Home", "Detail"]);
    assert_eq!(nav.top(), Some("Detail"));

    let detail = ir.with_route(nav.top().unwrap()).unwrap();
    // Detail's route tree root is its own Column, distinct from Home's.
    assert_eq!(detail.tree.kind.name(), "Column");
    assert_eq!(detail.tree.children[0].kind.name(), "Text");
    let detail_digest = digest_hex(&Renderer::new(160, 240).render(&detail, &state).pixels);
    assert_ne!(detail_digest, home_digest, "each route paints its own tree");

    nav.apply_nav("..").unwrap();
    assert_eq!(nav.stack(), ["Home"]);
    let back = ir.with_route(nav.top().unwrap()).unwrap();
    assert_eq!(
        digest_hex(&Renderer::new(160, 240).render(&back, &state).pixels),
        home_digest,
        "popping home must repaint the entry page exactly"
    );
}

#[test]
fn pop_at_the_root_is_ignored() {
    let mut nav = Navigator::with_root("Home", TransitionKind::Slide);
    assert_eq!(nav.apply_nav(".."), Ok(None));
    assert_eq!(nav.stack(), ["Home"]);
    assert_eq!(nav.progress(), ONE);
}

#[test]
fn push_targets_are_validated_against_the_declared_routes() {
    let ir = parse_ir(ROUTES_IR).unwrap();
    ir.check_push_target("Detail").unwrap();
    let err = ir.check_push_target("Ghost").unwrap_err();
    assert!(err.to_string().contains("Ghost"), "{err}");
    assert!(err.to_string().contains("Detail"), "{err}");

    // A push to an undeclared route is a parse error too.
    let bad = ROUTES_IR.replace(
        r#"{"op": "push", "target": "Detail"}"#,
        r#"{"op": "push", "target": "Missing"}"#,
    );
    let err = parse_ir(&bad).unwrap_err();
    assert!(err.to_string().contains("Missing"), "{err}");

    // A single-page document has no route set: any target is accepted.
    let single = parse_ir(SINGLE_IR).unwrap();
    single.check_push_target("Anything").unwrap();
}

#[test]
fn single_page_ir_stays_a_single_page() {
    let ir = parse_ir(SINGLE_IR).unwrap();
    assert!(ir.routes.is_empty());
    assert_eq!(ir.initial_route, None);
    assert_eq!(ir.with_route("Home"), None);
    assert_eq!(ir.tree.kind.name(), "Text");
}

#[test]
fn push_and_pop_actions_parse_with_their_contract() {
    let ir = parse_ir(ROUTES_IR).unwrap();
    let open = &ir.route("Home").unwrap().tree.children[1];
    assert_eq!(open.events[0].actions[0].op, Op::Push);
    assert_eq!(open.events[0].actions[0].target, "Detail");
    let back = &ir.route("Detail").unwrap().tree.children[1];
    assert_eq!(back.events[0].actions[0].op, Op::Pop);
    assert_eq!(back.events[0].actions[0].target, "");
}

#[test]
fn routes_alongside_a_top_level_tree_are_rejected() {
    let both = SINGLE_IR.replace(
        r#""theme""#,
        r#""routes": {"Home": {"tree": {"id": 0, "kind": "Text", "props": {"value": "x"}, "children": [], "events": {}}}}, "theme""#,
    );
    let err = parse_ir(&both).unwrap_err();
    assert!(err.to_string().contains("routes"), "{err}");
}

#[test]
fn an_unknown_initial_route_is_rejected() {
    let unknown = ROUTES_IR.replace(r#""initialRoute": "Home""#, r#""initialRoute": "Ghost""#);
    let err = parse_ir(&unknown).unwrap_err();
    assert!(err.to_string().contains("Ghost"), "{err}");
}

#[test]
fn empty_routes_object_is_rejected() {
    let ir = r##"{
      "ir": "0.1.0", "app": "Multi", "state": [], "initialRoute": "Home",
      "routes": {},
      "theme": {"mode": "light", "accent": "#097FEB", "radius": 8}
    }"##;
    let err = parse_ir(ir).unwrap_err();
    assert!(err.to_string().contains("at least one route"), "{err}");
}

#[test]
fn push_with_a_value_and_pop_with_a_target_are_rejected() {
    let with_value = ROUTES_IR.replace(
        r#"{"op": "push", "target": "Detail"}"#,
        r#"{"op": "push", "target": "Detail", "value": 1}"#,
    );
    assert!(parse_ir(&with_value).is_err());
    let with_target = ROUTES_IR.replace(r#"{"op": "pop"}"#, r#"{"op": "pop", "target": "Home"}"#);
    assert!(parse_ir(&with_target).is_err());
}

#[test]
fn a_missing_entry_route_falls_back_to_the_first_declared_route() {
    let ir = ROUTES_IR.replace("  \"initialRoute\": \"Home\",\n", "");
    assert!(!ir.contains("initialRoute"), "initialRoute must be removed");
    let ir = parse_ir(&ir).unwrap();
    // Sorted key order makes the fallback deterministic: "Detail" < "Home".
    assert_eq!(ir.initial_route.as_deref(), Some("Detail"));
    assert_eq!(ir.tree.kind.name(), "Column");
    assert_eq!(ir.tree.children[0].kind.name(), "Text");
}
