//! The Linear section's list or board is the window's to remember: picking
//! one tells the window, and the window's stored one lays the view out.
use super::linear_board::{click, open_view};
use crate::orchestrator::view::{Event, board::Layout};
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn picking_the_board_shows_it_and_tells_the_window(cx: &mut TestAppContext) {
    let (view, cx) = open_view(cx);
    let told = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|_, cx| {
        let told = told.clone();
        cx.subscribe(&view, move |_, event: &Event, _| {
            if let Event::Layout(layout) = event {
                told.borrow_mut().push(*layout);
            }
        })
    });
    assert!(!cx.update(|_, cx| view.read(cx).shows_board()));
    click(cx, "orchestrator-layout-board");
    assert!(cx.update(|_, cx| view.read(cx).shows_board()));
    assert_eq!(*told.borrow(), [Layout::Board]);
    // The window's stored layout lays it out without telling it back.
    cx.update(|_, cx| view.update(cx, |view, cx| view.set_layout(Layout::List, cx)));
    assert!(!cx.update(|_, cx| view.read(cx).shows_board()));
    assert_eq!(*told.borrow(), [Layout::Board]);
}

#[test]
fn layouts_are_stored_by_name() {
    for layout in [Layout::List, Layout::Board] {
        assert_eq!(Layout::parse(layout.name()), Some(layout));
    }
    assert_eq!(Layout::parse("grid"), None);
}
