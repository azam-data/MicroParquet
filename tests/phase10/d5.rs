//! D5: design templates. The picture, the slots, the project copy and the hash are tested here; the window is in `src/ui/render_tests_p10.rs`.

use micro_parquet::dashboard::*;
use micro_parquet::project::Project;
use micro_parquet::templates::*;

fn png_of(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).expect("png");
    out
}

fn slot(name: &str, kind: SlotKind, x: f64, y: f64, w: f64, h: f64) -> Slot {
    Slot { name: name.into(), kind, rect: DashRect { x, y, w, h } }
}

fn four_slots() -> Vec<Slot> {
    vec![slot("Deposits chart", SlotKind::Chart, 20.0, 20.0, 460.0, 280.0), slot("Wing table", SlotKind::Pivot, 500.0, 20.0, 480.0, 280.0), slot("CASA", SlotKind::Kpi, 20.0, 320.0, 220.0, 100.0), slot("Notes", SlotKind::Any, 260.0, 320.0, 720.0, 200.0)]
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mp_d5_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("dir");
    d
}

#[test]
fn d5_a_1920_by_1080_png_with_4_slots_is_the_same_template_after_save_and_reopen() {
    let d = design_from_bytes(&png_of(1920, 1080)).expect("a design");
    assert_eq!((d.w, d.h), (1920, 1080), "under 2,000 px: not made smaller");
    // the page takes the picture's shape: 1000 x 562.5 units, nothing is stretched
    assert!((d.page_h() - 562.5).abs() < 1e-9 && d.note.is_none());
    let slots = four_slots();
    let data = dir("save");
    save_to_folder(&data, "Bank blue", &d, &slots).expect("saved");
    // in a project
    let mut p = Project::from_json(&std::fs::read_to_string(crate::common::fixtures_dir().join("phase9_replay/proj/phase9_replay.mpproj")).expect("fixture"), "x").expect("opens");
    let tref = TemplateRef { name: "Bank blue".into(), blake3: d.blake3.clone() };
    let mut page = DashPage { id: "p1".into(), title: "P".into(), ..Default::default() };
    page.apply_template(tref.clone(), slots.clone(), d.page_h()).expect("applied");
    p.dashboard = Some(Dashboard { id: "d1".into(), title: "D".into(), pages: vec![page], template_library: vec![tref.clone()], templates: vec![project_copy("Bank blue", &d, &slots)], ..Default::default() });
    let json = p.to_json().expect("json");
    let again = Project::from_json(&json, "x").expect("reopens");
    let dash = again.dashboard.as_ref().expect("dashboard");
    assert_eq!(dash.template_library, vec![tref.clone()]);
    assert_eq!(dash.pages[0].background, Some(tref.clone()));
    assert_eq!(dash.pages[0].slots, slots);
    assert_eq!(dash.pages[0].layout, PageLayout::Free);
    // the picture in the project is the same picture: the same hash
    let from_project = resolve(&data.join("nowhere"), "Bank blue", Some(&dash.templates[0])).expect("the project copy");
    assert_eq!((from_project.from, from_project.blake3.as_str(), from_project.slots.len()), (Origin::ProjectCopy, d.blake3.as_str(), 4));
    assert_eq!(from_project.png, d.png);
    assert_eq!(again.to_json().expect("json"), json, "save, open, save is stable");
}

#[test]
fn d5_the_project_copy_is_used_when_the_data_folder_has_no_such_template_and_a_hash_mismatch_is_reported() {
    let d = design_from_bytes(&png_of(1920, 1080)).expect("design");
    let slots = four_slots();
    let copy = project_copy("T", &d, &slots);
    let data = dir("resolve");
    // no such template in the data folder: the project's copy, no warning
    let r = resolve(&data, "T", Some(&copy)).expect("project copy");
    assert_eq!((r.from, r.warning.is_none(), r.slots), (Origin::ProjectCopy, true, slots.clone()));
    // the data folder has it and it is the same picture: the folder's
    save_to_folder(&data, "T", &d, &slots).expect("saved");
    let r = resolve(&data, "T", Some(&copy)).expect("folder");
    assert_eq!((r.from, r.warning.is_none()), (Origin::DataFolder, true));
    // the data folder has another picture under the name: reported, and the project's copy is shown
    let other = design_from_bytes(&png_of(1920, 1200)).expect("other");
    save_to_folder(&data, "T", &other, &slots).expect("saved");
    let r = resolve(&data, "T", Some(&copy)).expect("project copy again");
    assert_eq!((r.from, r.blake3.as_str()), (Origin::ProjectCopy, d.blake3.as_str()));
    assert_eq!(r.warning.as_deref(), Some("The template 'T' on this computer is not the one this dashboard was made with (its hash differs). The copy kept in the project is used."));
    // in neither place
    let e = resolve(&dir("empty"), "T", None).expect_err("nowhere").to_string();
    assert_eq!(e, "The template 'T' is not in this project and not in the data folder. Import it again.");
    // a damaged project copy
    let mut bad = copy.clone();
    bad.blake3 = "0".repeat(64);
    let e = resolve(&dir("empty2"), "T", Some(&bad)).expect_err("damaged").to_string();
    assert!(e.contains("is damaged"), "{e}");
}

#[test]
fn d5_a_file_that_is_not_a_picture_a_picture_over_3_mb_and_a_slot_outside_are_refused_in_words() {
    assert_eq!(design_from_bytes(b"hello, this is a text file").expect_err("text").to_string(), "This file is not a PNG or JPEG picture.");
    let mut broken = png_of(40, 40);
    broken.truncate(60);
    assert!(design_from_bytes(&broken).expect_err("broken").to_string().starts_with("This file is not a PNG or JPEG picture."));
    // noise does not compress: 1800 x 1200 pixels of it is well over 3 MB
    let mut x: u32 = 12345;
    let noise = image::RgbaImage::from_fn(1800, 1200, |_, _| {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        image::Rgba([(x >> 24) as u8, (x >> 16) as u8, (x >> 8) as u8, 255])
    });
    let mut big = Vec::new();
    image::DynamicImage::ImageRgba8(noise).write_to(&mut std::io::Cursor::new(&mut big), image::ImageFormat::Png).expect("png");
    let e = design_from_bytes(&big).expect_err("big").to_string();
    assert!(e.contains("MB after it was made smaller; the limit is 3 MB"), "{e}");
    // slots
    let outside = vec![slot("Off", SlotKind::Any, 900.0, 20.0, 200.0, 100.0)];
    assert!(check_slots(&outside, PAGE_H).expect_err("outside").to_string().contains("The slot 'Off' is outside the picture"));
    let twice = vec![slot("A", SlotKind::Any, 0.0, 0.0, 100.0, 100.0), slot("A", SlotKind::Chart, 200.0, 0.0, 100.0, 100.0)];
    assert_eq!(check_slots(&twice, PAGE_H).expect_err("twice").to_string(), "Two slots are named 'A'. Give each slot its own name.");
    assert_eq!(check_slots(&[slot(" ", SlotKind::Any, 0.0, 0.0, 100.0, 100.0)], PAGE_H).expect_err("name").to_string(), "Every slot needs a name.");
    check_slots(&four_slots(), PAGE_H).expect("the four are fine");
    // a name that cannot be a folder
    let d = design_from_bytes(&png_of(100, 100)).expect("design");
    assert!(save_to_folder(&dir("nm"), "a/b", &d, &[]).expect_err("name").to_string().contains("cannot be used as a folder name"));
}

#[test]
fn d5_a_big_picture_is_made_smaller_to_2000_px_and_a_jpeg_is_read_and_kept_as_a_png() {
    let d = design_from_bytes(&png_of(4000, 2000)).expect("design");
    assert_eq!((d.w, d.h), (2000, 1000));
    let img = image::RgbImage::from_fn(300, 200, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
    let mut jpg = Vec::new();
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut jpg), image::ImageFormat::Jpeg).expect("jpeg");
    assert!(jpg.starts_with(&[0xFF, 0xD8]));
    let d = design_from_bytes(&jpg).expect("a jpeg");
    assert_eq!((d.w, d.h), (300, 200));
    assert!(d.png.starts_with(&[0x89, b'P', b'N', b'G']), "kept as a PNG");
    assert_eq!(d.blake3.len(), 64);
    // a 16:10 picture gives the usual 625; a 4:3 one gives 750
    assert_eq!(design_from_bytes(&png_of(1600, 1000)).expect("16:10").page_h(), 625.0);
    assert_eq!(design_from_bytes(&png_of(1600, 1200)).expect("4:3").page_h(), 750.0);
}

#[test]
fn d5_base64_round_trips_every_length() {
    for n in 0..40usize {
        let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
        assert_eq!(base64_decode(&base64_encode(&bytes)).expect("decodes"), bytes, "{n} bytes");
    }
    assert_eq!(base64_encode(b"Man"), "TWFu");
    assert_eq!(base64_encode(b"Ma"), "TWE=");
    assert_eq!(base64_encode(b"M"), "TQ==");
    assert!(base64_decode("TW*u").is_err());
}

#[test]
fn d5_a_slot_is_filled_by_an_item_of_its_kind_and_the_item_takes_its_place_and_an_emptied_slot_is_empty_again() {
    let mut page = DashPage { id: "p1".into(), title: "P".into(), ..Default::default() };
    page.apply_template(TemplateRef { name: "T".into(), blake3: "h".into() }, four_slots(), PAGE_H).expect("applied");
    assert_eq!(page.empty_slots().len(), 4);
    let chart = DashItem { id: "i1".into(), kind: DashItemKind::Chart("t2".into()), ..Default::default() };
    page.fill_slot("Deposits chart", chart.clone()).expect("filled");
    assert_eq!(page.items[0].rect, Some(DashRect { x: 20.0, y: 20.0, w: 460.0, h: 280.0 }), "the item sits where the slot is");
    assert_eq!(page.empty_slots().len(), 3);
    assert_eq!(page.fill_slot("Deposits chart", DashItem { id: "i2".into(), ..chart.clone() }).expect_err("full").to_string(), "The slot 'Deposits chart' already has an item. Remove it first.");
    assert_eq!(page.fill_slot("CASA", DashItem { id: "i3".into(), ..chart.clone() }).expect_err("kind").to_string(), "The slot 'CASA' is for a kpi item. Fill it with one, or change the slot's type in the design.");
    assert_eq!(page.fill_slot("Nope", chart.clone()).expect_err("none").to_string(), "The page has no slot named 'Nope'.");
    page.fill_slot("Notes", DashItem { id: "i4".into(), kind: DashItemKind::Pivot("t3".into()), ..Default::default() }).expect("Any takes a pivot");
    // removing the item empties the slot again
    page.items.retain(|i| i.id != "i1");
    assert_eq!(page.empty_slots().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Deposits chart", "Wing table", "CASA"]);
    // an old page without slots or a design writes nothing new
    let plain = DashPage { id: "p".into(), title: "P".into(), ..Default::default() };
    let j = serde_json::to_string(&plain).expect("json");
    assert!(!j.contains("slots") && !j.contains("templates"), "{j}");
}

#[test]
fn d5_a_page_takes_the_shape_of_its_picture_so_nothing_is_stretched_and_items_and_slots_stay_inside() {
    // 1920 x 1080 gives a 1000 x 562.5 page
    let d = design_from_bytes(&png_of(1920, 1080)).expect("design");
    let mut page = DashPage { id: "p1".into(), title: "P".into(), layout: PageLayout::Grid { per_row: 1 }, items: vec![DashItem { id: "i1".into(), ..Default::default() }, DashItem { id: "i2".into(), ..Default::default() }], ..Default::default() };
    assert_eq!(page.height(), 625.0);
    page.to_free().expect("free");
    assert_eq!(page.items[1].rect, Some(DashRect { x: 0.0, y: 317.5, w: 1000.0, h: 307.5 }), "one per row on 625: (625 - 10) / 2 = 307.5 each");
    page.apply_template(TemplateRef { name: "T".into(), blake3: "h".into() }, vec![], d.page_h()).expect("applied");
    assert_eq!(page.height(), 562.5);
    // the second item is pulled inside the shorter page: y + h = 562.5
    let r = page.items[1].rect.expect("rect");
    assert!(r.bottom() <= 562.5 + 1e-9 && r.y >= 0.0, "{r:?}");
    // a slot between 562.5 and 625 is outside this picture, and inside a 16:10 one
    let low = [slot("Low", SlotKind::Any, 20.0, 540.0, 200.0, 70.0)];
    assert!(check_slots(&low, d.page_h()).expect_err("outside").to_string().contains("outside the picture"));
    check_slots(&low, PAGE_H).expect("inside 16:10");
    // drags are held inside the page's own height
    assert_eq!(drag_rect(DashRect { x: 0.0, y: 0.0, w: 300.0, h: 200.0 }, Handle::Move, 0.0, 9000.0, 562.5).y, 362.5);
    // a 16:10 design writes nothing new; taking the design away gives 625 back
    page.set_height(PAGE_H);
    assert!(page.page_h.is_none() && !serde_json::to_string(&page).expect("json").contains("page_h"));
    page.set_height(562.5);
    assert!(serde_json::to_string(&page).expect("json").contains("\"page_h\":562.5"));
}
