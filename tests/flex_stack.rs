//! Flex-weighted vertical stacks: a weighted child absorbs the leftover main
//! axis so a bottom pane reaches the stack end on every viewport.
use opurom_render::{layout, parse_ir, LaidOut, StateView};

fn node<'a>(laid: &'a [LaidOut], id: u32) -> &'a LaidOut {
    laid.iter().find(|n| n.id == id).expect("node id missing")
}

#[test]
fn flex_children_split_the_leftover_space() {
    let ir = r##"{"ir":"0.1.0","app":"Flex","state":[],"tree":{"id":0,"kind":"Column","props":{"padding":8,"gap":8},"children":[{"id":1,"kind":"Container","props":{"height":50},"children":[],"events":{}},{"id":2,"kind":"Container","props":{"flex":1},"children":[],"events":{}},{"id":3,"kind":"Container","props":{"flex":3},"children":[],"events":{}}],"events":{}},"theme":{"mode":"light","accent":"#097FEB","radius":4}}"##;
    let parsed = parse_ir(ir).unwrap();
    let laid = layout(&parsed, &StateView::from_ir(&parsed), 400, 800);
    let fixed = node(&laid, 1).rect;
    let a = node(&laid, 2).rect;
    let b = node(&laid, 3).rect;
    assert_eq!(fixed.h, 50, "fixed pane keeps its height");
    assert_eq!(b.y + b.h, 792, "last flex pane reaches the bottom padding");
    assert_eq!(a.h + b.h, 718, "panes consume exactly the leftover");
    assert!(b.h > a.h, "flex 3 takes more than flex 1");
}

#[test]
fn weighted_keypad_pane_reaches_the_bottom() {
    // Pre-order ids: root 0, fixed 1, display 2, keypad 3, grid 4, cells 5..=24.
    let mut keys = String::new();
    for id in 5..=24u32 {
        if id > 5 {
            keys.push(',');
        }
        keys.push_str(&format!(
            r#"{{"id":{id},"kind":"Button","props":{{"label":"{id}"}},"children":[],"events":{{}}}}"#
        ));
    }
    let ir = format!(
        r##"{{"ir":"0.1.0","app":"Calc","state":[],"tree":{{"id":0,"kind":"Column","props":{{"padding":8,"gap":8}},"children":[{{"id":1,"kind":"Container","props":{{"height":48}},"children":[],"events":{{}}}},{{"id":2,"kind":"Container","props":{{"flex":1}},"children":[],"events":{{}}}},{{"id":3,"kind":"Container","props":{{"flex":2}},"children":[{{"id":4,"kind":"Grid","props":{{"columns":4,"compactColumns":4,"fillHeight":true}},"children":[{keys}],"events":{{}}}}],"events":{{}}}}],"events":{{}}}},"theme":{{"mode":"light","accent":"#097FEB","radius":4}}}}"##
    );
    let parsed = parse_ir(&ir).unwrap();
    let laid = layout(&parsed, &StateView::from_ir(&parsed), 390, 844);
    let pane = node(&laid, 3).rect;
    assert_eq!(
        pane.y + pane.h,
        836,
        "keypad pane reaches the bottom padding: y={} h={}",
        pane.y,
        pane.h
    );
    let last = node(&laid, 24).rect;
    assert_eq!(
        last.y + last.h,
        836,
        "the keypad's last row reaches the bottom too"
    );
}
