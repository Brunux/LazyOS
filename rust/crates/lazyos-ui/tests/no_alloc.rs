//! A frame of a cached list allocates (almost) nothing.
//!
//! This is the witness behind `docs/optimization.md` §13's 445 µs: the cost was
//! `format!` per row per frame, not ratatui. The rows here are built once by
//! `ListState::set_rows`, and drawing copies cells - so a hundred frames must
//! not allocate a hundred times.
//!
//! The allocator counts every allocation the test binary makes, so the
//! assertion is on a tight steady-state loop after a warm-up, and the allowance
//! covers what is outside our control (the test backend's diff buffer grows
//! once, the terminal's swap does not allocate at all).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use lazyos_ui::{
    App, BodyCtx, Card, ColorRole, Design, Header, KeyHint, Kind, ListState, Row, Screen,
    draw_badge, draw_card, draw_gauge, draw_progress,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

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

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

struct Rows {
    list: ListState,
}

impl Screen for Rows {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        // A screenful of the widgets that could format something: the gauge's
        // percentage, the progress label and the card's title are the three
        // that a first draft built with `format!` per frame.
        let line = Rect {
            y: ctx.area.y,
            height: 1,
            ..ctx.area
        };
        draw_gauge(ctx.buf, line, ctx.design, 0.42, "battery");
        let line = Rect {
            y: ctx.area.y + 1,
            height: 1,
            ..ctx.area
        };
        draw_progress(
            ctx.buf,
            line,
            ctx.design,
            0.63,
            "downloading",
            ColorRole::Accent,
        );
        let x = draw_badge(
            ctx.buf,
            ctx.area.x,
            ctx.area.y + 2,
            ctx.design,
            "connected",
            ColorRole::Ok,
        );
        let _ = x;
        let card = Rect {
            y: ctx.area.y + 3,
            height: ctx.area.height.saturating_sub(3),
            ..ctx.area
        };
        let inner = draw_card(ctx.buf, card, ctx.design, Some("rows"), true);
        self.list.render(inner, ctx.buf, ctx.design, true);
    }
}

#[test]
fn a_frame_of_a_cached_list_allocates_at_most_a_constant() {
    const FRAMES: usize = 100;
    let mut list = ListState::new();
    list.set_rows(
        (0..40)
            .map(|i| {
                Row::new(format!("row {i:02}"))
                    .detail("cached")
                    .value("Alt+1")
            })
            .collect(),
    );
    let mut screen = Rows { list };
    let mut app = App::new(Header::new("LazyOS").subtitle("no allocations"))
        .design(Design::solarized().with_reduced_motion(true))
        .keybar(vec![KeyHint::new("Esc", "close")])
        .screen(&mut screen);
    let mut terminal = Terminal::new(TestBackend::new(152, 45)).unwrap();

    for _ in 0..10 {
        terminal.draw(|frame| app.render(frame)).unwrap();
    }
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..FRAMES {
        terminal.draw(|frame| app.render(frame)).unwrap();
    }
    let after = ALLOCATIONS.load(Ordering::Relaxed);
    let per_frame = (after - before) as f64 / FRAMES as f64;

    // A per-row format! would be 40 allocations a frame; the allowance is for
    // the backend's own bookkeeping, not for ours.
    assert!(
        per_frame <= 2.0,
        "a frame allocated {per_frame} times: the rows are being rebuilt per frame \
         instead of copied (docs/optimization.md §13)"
    );
    eprintln!("allocations per frame: {per_frame}");

    // The same bound for a notification card (spec 006): its text is built when
    // it is made, so a frame copies cells and the progress label is not
    // formatted again. The glyph is the codepoint (Folder), not the fallback
    // path, which reads the environment once.
    let mut card = CardScreen {
        card: Card::new("Saved")
            .body("~/Pictures/shot.png")
            .kind(Kind::Path {
                dir: false,
                name: "shot.png",
            })
            .progress(40),
    };
    let mut app = App::new(Header::new("LazyOS").subtitle("no allocations"))
        .design(Design::solarized().with_reduced_motion(true))
        .keybar(vec![KeyHint::new("Esc", "close")])
        .screen(&mut card);
    for _ in 0..10 {
        terminal.draw(|frame| app.render(frame)).unwrap();
    }
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..FRAMES {
        terminal.draw(|frame| app.render(frame)).unwrap();
    }
    let after = ALLOCATIONS.load(Ordering::Relaxed);
    let per_frame = (after - before) as f64 / FRAMES as f64;
    assert!(
        per_frame <= 2.0,
        "a card frame allocated {per_frame} times: the card rebuilds its text per frame"
    );
    eprintln!("card allocations per frame: {per_frame}");
}

struct CardScreen {
    card: Card,
}

impl Screen for CardScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        self.card.render(ctx.area, ctx.buf, ctx.design);
    }
}
