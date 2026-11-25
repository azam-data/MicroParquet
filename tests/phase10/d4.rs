//! D4: free drag-and-resize layout. The maths is tested here with hand-worked numbers; the real drags are in `src/ui/render_tests_p10.rs`.

use micro_parquet::dashboard::*;

fn r(x: f64, y: f64, w: f64, h: f64) -> DashRect {
    DashRect { x, y, w, h }
}

fn page(n: usize, per_row: usize) -> DashPage {
    DashPage { id: "p1".into(), title: "P".into(), layout: PageLayout::Grid { per_row }, items: (0..n).map(|i| DashItem { id: format!("i{}", i + 1), ..Default::default() }).collect(), ..Default::default() }
}

#[test]
fn d4_a_move_snaps_to_10_units_and_an_item_dragged_off_the_page_is_clamped_never_lost() {
    let a = r(100.0, 100.0, 300.0, 200.0);
    // +23, -47 gives (123, 53), snapped to (120, 50)
    assert_eq!(drag_rect(a, Handle::Move, 23.0, -47.0), r(120.0, 50.0, 300.0, 200.0));
    // far to the left and up: against the edge, the same size
    assert_eq!(drag_rect(a, Handle::Move, -500.0, -500.0), r(0.0, 0.0, 300.0, 200.0));
    // far to the right and down: 1000 - 300 = 700 and 625 - 200 = 425, still the whole item
    assert_eq!(drag_rect(a, Handle::Move, 9000.0, 9000.0), r(700.0, 425.0, 300.0, 200.0));
}

#[test]
fn d4_each_corner_and_edge_resizes_snaps_and_keeps_the_minimum_size() {
    let a = r(100.0, 100.0, 300.0, 200.0);
    // SE +37 +14: right 437 -> 440, bottom 314 -> 310: 340 x 210
    assert_eq!(drag_rect(a, Handle::SE, 37.0, 14.0), r(100.0, 100.0, 340.0, 210.0));
    // NW +24 +24: left 124 -> 120, top 124 -> 120: x 120, y 120, w 400 - 120 = 280, h 300 - 120 = 180
    assert_eq!(drag_rect(a, Handle::NW, 24.0, 24.0), r(120.0, 120.0, 280.0, 180.0));
    // an edge moves one side only
    assert_eq!(drag_rect(a, Handle::E, 51.0, 99.0), r(100.0, 100.0, 350.0, 200.0));
    assert_eq!(drag_rect(a, Handle::N, 99.0, -31.0), r(100.0, 70.0, 300.0, 230.0));
    assert_eq!(drag_rect(a, Handle::SW, -30.0, 20.0), r(70.0, 100.0, 330.0, 220.0));
    assert_eq!(drag_rect(a, Handle::NE, 30.0, -20.0), r(100.0, 80.0, 330.0, 220.0));
    assert_eq!(drag_rect(a, Handle::S, 77.0, 31.0), r(100.0, 100.0, 300.0, 230.0));
    assert_eq!(drag_rect(a, Handle::W, -42.0, 5.0), r(60.0, 100.0, 340.0, 200.0));
    // dragged past the opposite side: the minimum (60 x 40) stays, on the fixed side
    assert_eq!(drag_rect(a, Handle::W, 290.0, 0.0), r(340.0, 100.0, 60.0, 200.0));
    assert_eq!(drag_rect(a, Handle::N, 0.0, 500.0), r(100.0, 260.0, 300.0, 40.0));
    // and never past the page
    assert_eq!(drag_rect(a, Handle::SE, 9000.0, 9000.0), r(100.0, 100.0, 900.0, 525.0));
    assert_eq!(drag_rect(a, Handle::NW, -9000.0, -9000.0), r(0.0, 0.0, 400.0, 300.0));
}

#[test]
fn d4_items_may_not_overlap_unless_the_page_allows_it_and_a_blocked_drag_keeps_the_last_good_place() {
    let mut p = page(2, 2);
    p.to_free().expect("free");
    p.items[0].rect = Some(r(0.0, 0.0, 300.0, 200.0));
    p.items[1].rect = Some(r(310.0, 0.0, 300.0, 200.0));
    let prev = p.items[1].rect.expect("rect");
    // 50 to the left would be x = 260: over item 1: refused, the item stays at 310
    assert_eq!(p.place(1, drag_rect(prev, Handle::Move, -50.0, 0.0), prev), prev);
    // touching is not overlapping: x = 300
    assert_eq!(p.place(1, drag_rect(prev, Handle::Move, -10.0, 0.0), prev), r(300.0, 0.0, 300.0, 200.0));
    // a resize into the neighbour is refused the same way
    assert_eq!(p.place(0, drag_rect(r(0.0, 0.0, 300.0, 200.0), Handle::E, 40.0, 0.0), r(0.0, 0.0, 300.0, 200.0)), r(0.0, 0.0, 300.0, 200.0));
    // with overlap turned on the same drag is taken
    p.allow_overlap = true;
    assert_eq!(p.place(1, drag_rect(prev, Handle::Move, -50.0, 0.0), prev), r(260.0, 0.0, 300.0, 200.0));
    // an item is not blocked by itself
    p.allow_overlap = false;
    assert_eq!(p.place(0, r(0.0, 10.0, 300.0, 200.0), r(0.0, 0.0, 300.0, 200.0)), r(0.0, 10.0, 300.0, 200.0));
}

#[test]
fn d4_grid_to_free_to_grid_gives_the_same_positions_for_items_that_still_sit_on_the_grid() {
    let mut p = page(5, 3);
    let before = item_rects(&p).expect("grid");
    p.to_free().expect("free");
    assert_eq!(p.layout, PageLayout::Free);
    assert_eq!(item_rects(&p).expect("free rects"), before, "Free starts where the grid had the items");
    assert!(p.off_grid_items().is_empty());
    assert_eq!(p.switch_message(), "Every item sits on the grid, so nothing moves.");
    p.to_grid();
    assert_eq!(p.layout, PageLayout::Grid { per_row: 3 }, "the items per row come back");
    assert_eq!(item_rects(&p).expect("grid again"), before);
    assert!(p.items.iter().all(|i| i.rect.is_none()));
    // one item moved: the dialog names it, and going back puts it in its cell
    p.to_free().expect("free");
    p.items[1].rect = Some(drag_rect(before[1], Handle::Move, 50.0, 30.0));
    assert_eq!(p.off_grid_items(), vec!["i2".to_string()]);
    assert_eq!(p.switch_message(), "1 of 5 items are not on the grid. They will go to the cells of their order and lose the place you gave them: i2.");
    p.to_grid();
    assert_eq!(item_rects(&p).expect("grid"), before);
}

#[test]
fn d4_a_new_item_takes_the_first_free_place_and_a_full_page_says_so() {
    let mut p = page(1, 2);
    p.to_free().expect("free");
    p.items[0].rect = Some(r(0.0, 0.0, 300.0, 200.0));
    // the first place a 300 x 200 item fits: right of the first one, touching it
    assert_eq!(p.first_free_spot(NEW_ITEM_W, NEW_ITEM_H).expect("a place"), r(300.0, 0.0, 300.0, 200.0));
    // a page covered by one item has no room
    p.items[0].rect = Some(r(0.0, 0.0, 1000.0, 625.0));
    let e = p.first_free_spot(NEW_ITEM_W, NEW_ITEM_H).expect_err("full").to_string();
    assert_eq!(e, "The page 'P' has no room for another item. Remove or shrink one, or turn on Allow overlap.");
    p.allow_overlap = true;
    assert_eq!(p.first_free_spot(NEW_ITEM_W, NEW_ITEM_H).expect("overlap is allowed"), r(0.0, 0.0, 300.0, 200.0));
}

#[test]
fn d4_the_free_rects_and_the_overlap_switch_are_saved_and_a_grid_page_writes_nothing_new() {
    let mut p = page(2, 2);
    let grid_json = serde_json::to_string(&p).expect("json");
    assert!(!grid_json.contains("allow_overlap") && !grid_json.contains("grid_per_row"), "a Grid page's bytes are as before: {grid_json}");
    p.to_free().expect("free");
    p.allow_overlap = true;
    p.items[0].rect = Some(drag_rect(p.items[0].rect.expect("r"), Handle::Move, 33.0, 0.0));
    let json = serde_json::to_string(&p).expect("json");
    let back: DashPage = serde_json::from_str(&json).expect("opens");
    assert_eq!(back, p);
    assert!(back.allow_overlap && back.grid_per_row == Some(2));
}
