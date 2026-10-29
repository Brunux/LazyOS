//! The ranked list (spec 007): one ordering for the root menu and the palette.
//!
//! The order is the contract a person learns with their fingers - type `th`,
//! press Enter, get the theme - so it is pinned here, tier by tier, and so is
//! the cost: a ranked keystroke allocates no more per row than a plain one
//! (`tests/filter.rs` is the plain half).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use lazyos_ui::{ListState, Row, Tier};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn ranked(rows: Vec<Row>) -> ListState {
    let mut list = ListState::new().ranked();
    list.set_rows(rows);
    list
}

fn order(list: &mut ListState) -> Vec<String> {
    (0..list.visible_len())
        .map(|i| {
            list.set_cursor(i);
            list.selected().unwrap().label.clone()
        })
        .collect()
}

#[test]
fn ranking_prefers_prefix() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    // The spec's own example: `th` must rank `theme` above `themes install`
    // (both prefixes; the shorter label is the closer match) and above a row
    // that only holds the letters in order.
    let mut list = ranked(vec![
        Row::new("Bluetooth").detail("pick a device"),
        Row::new("themes install"),
        Row::new("Time zone").detail("and network time"),
        Row::new("theme"),
    ]);
    list.set_filter("th");
    assert_eq!(
        order(&mut list),
        vec!["theme", "themes install", "Bluetooth"],
        "prefix before substring, shorter prefix first; `Time zone` has no `h` after its `t`"
    );
    list.set_filter("tz");
    assert_eq!(
        order(&mut list),
        vec!["Time zone"],
        "a subsequence still matches"
    );
}

#[test]
fn every_tier_lands_in_its_place() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let row = |label: &str| Row::new(label);
    assert_eq!(Tier::of(&row("set"), "set"), Some(Tier::Exact));
    assert_eq!(Tier::of(&row("settings"), "set"), Some(Tier::Prefix));
    assert_eq!(Tier::of(&row("theme set"), "set"), Some(Tier::WordPrefix));
    assert_eq!(Tier::of(&row("theme-set"), "set"), Some(Tier::WordPrefix));
    assert_eq!(Tier::of(&row("reset"), "set"), Some(Tier::Substring));
    assert_eq!(
        Tier::of(&row("system settings"), "sst"),
        Some(Tier::Subsequence)
    );
    assert_eq!(
        Tier::of(&Row::new("Wi-Fi").detail("pick a network"), "network"),
        Some(Tier::Detail),
        "a ranked list finds what a plain one finds, below every label match"
    );
    assert_eq!(Tier::of(&row("Power"), "xyz"), None);

    // Case and accents fold the same way the plain filter folds them.
    assert_eq!(Tier::of(&row("README.md"), "readme"), Some(Tier::Prefix));
    assert_eq!(Tier::of(&row("Año nuevo"), "año"), Some(Tier::Prefix));
    assert_eq!(Tier::of(&row("feliz Año"), "año"), Some(Tier::WordPrefix));
    assert_eq!(Tier::of(&row("ÜNÏCODE"), "üc"), Some(Tier::Subsequence));
}

#[test]
fn a_boost_orders_equal_matches_and_never_beats_a_better_tier() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut list = ranked(vec![
        Row::new("theme"),
        Row::new("themes install").boost(30),
        Row::new("north").boost(1_000),
    ]);
    list.set_filter("th");
    assert_eq!(
        order(&mut list),
        vec!["themes install", "theme", "north"],
        "the boost decides between two prefixes; `north` (a substring) stays below both"
    );
}

#[test]
fn an_empty_needle_keeps_the_rows_own_order() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    // The root menu reads in the order its groups were written, whatever the
    // boosts say: ranking is an answer to a needle, not a reshuffle.
    let mut list = ranked(vec![
        Row::new("Terminal"),
        Row::new("Files").boost(99),
        Row::new("Power"),
    ]);
    assert_eq!(order(&mut list), vec!["Terminal", "Files", "Power"]);
    list.set_filter("p");
    list.set_filter("");
    assert_eq!(order(&mut list), vec!["Terminal", "Files", "Power"]);
}

#[test]
fn typing_puts_the_cursor_on_the_best_match() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut list = ranked(vec![
        Row::new("Wi-Fi"),
        Row::new("Bluetooth"),
        Row::new("Audio output"),
    ]);
    list.set_cursor(2);
    list.set_filter("blue");
    assert_eq!(list.cursor(), 0);
    assert_eq!(list.selected().unwrap().label, "Bluetooth");
}

#[test]
fn narrowing_a_ranked_list_gives_the_same_answer_as_a_fresh_one() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let rows = || {
        vec![
            Row::new("theme set"),
            Row::new("themes install"),
            Row::new("Time zone"),
            Row::new("the manual").detail("Learn"),
            Row::new("north"),
            Row::new("Notifications").detail("history and do-not-disturb"),
        ]
    };
    let mut incremental = ranked(rows());
    for needle in ["t", "th", "the", "them", "the", "t", "", "n", "no"] {
        incremental.set_filter(needle);
        let mut fresh = ranked(rows());
        fresh.set_filter(needle);
        assert_eq!(
            order(&mut incremental),
            order(&mut fresh),
            "after {needle:?} the narrowed, re-sorted set must equal a fresh one"
        );
    }
}

#[test]
fn ranking_does_not_allocate_per_row() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());

    fn rows(count: usize) -> Vec<Row> {
        (0..count)
            .map(|i| {
                Row::new(format!("report-{i:05}-Quarterly.txt"))
                    .detail("a detail")
                    .boost((i % 7) as u32)
            })
            .collect()
    }

    fn keystroke(list: &mut ListState, needle: &str) -> usize {
        (0..5)
            .map(|_| {
                let before = ALLOCATIONS.load(Ordering::Relaxed);
                list.set_filter(needle);
                ALLOCATIONS.load(Ordering::Relaxed) - before
            })
            .min()
            .expect("five passes")
    }

    let mut small = ranked(rows(200));
    small.set_filter("rep");
    let small_typing = keystroke(&mut small, "repo");
    let small_back = keystroke(&mut small, "rep");

    let mut large = ranked(rows(5_000));
    large.set_filter("rep");
    let large_typing = keystroke(&mut large, "repo");
    let large_back = keystroke(&mut large, "rep");

    assert_eq!(
        (large_typing, large_back),
        (small_typing, small_back),
        "a ranked keystroke's allocations must not grow with the rows"
    );
    assert!(
        large_typing <= 2 && large_back <= 2,
        "a ranked keystroke over 5,000 rows allocated {large_typing}/{large_back} times"
    );
}

/// Rows that stream in (spec 009's walk) are filtered as they arrive, and the
/// row under the cursor stays under it: a batch must not move a person's place.
#[test]
fn rows_that_arrive_later_are_filtered_and_keep_the_cursor() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let design = lazyos_ui::Design::solarized();
    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

    let mut plain = ListState::new();
    plain.set_rows(vec![Row::new("a.txt"), Row::new("b.rs"), Row::new("c.txt")]);
    plain.handle(down, 10, &design);
    plain.set_filter("txt");
    assert_eq!(plain.selected().unwrap().label, "c.txt");
    plain.extend_rows(vec![Row::new("d.rs"), Row::new("e.txt")]);
    assert_eq!(
        plain.visible_len(),
        3,
        "only the new row that matches joins"
    );
    assert_eq!(plain.selected().unwrap().label, "c.txt");

    let mut ranked = ListState::new().ranked();
    ranked.set_rows(vec![Row::new("src/notes.md"), Row::new("a/notes-old.md")]);
    ranked.set_filter("notes");
    ranked.handle(down, 10, &design);
    let before = ranked.selected().unwrap().label.clone();
    // A better match arrives: it ranks first, and the cursor follows its row.
    ranked.extend_rows(vec![Row::new("notes"), Row::new("unrelated")]);
    assert_eq!(ranked.visible_len(), 3);
    assert_eq!(ranked.selected().unwrap().label, before);
    ranked.set_cursor(0);
    assert_eq!(
        ranked.selected().unwrap().label,
        "notes",
        "the exact match is first"
    );
}
