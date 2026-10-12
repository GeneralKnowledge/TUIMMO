//! The delisted-floor evidence has to fit the noticed panel on an 80x24 terminal.
//! The feed and the finding string can hold more than the panel shows.

use coffee_world::{starter_operator, WorldState};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use terminus_core::{Game, Permissions, PlayerId, SessionIdentity};
use terminus_tui::{App, Theme};

fn render(view: terminus_core::GameView) -> String {
    render_size(view, 80, 24)
}

fn render_size(view: terminus_core::GameView, width: u16, height: u16) -> String {
    let app = App::play(Theme::neutral(), view);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut lines = Vec::new();
    for y in 0..buffer.area().height {
        let mut line = String::new();
        for x in 0..buffer.area().width {
            line.push_str(buffer[(x, y)].symbol());
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Left-hand pane body under a panel title. The right-hand feed is not included.
fn left_panel(screen: &str, title: &str) -> String {
    let mut started = false;
    let mut text = String::new();
    for line in screen.lines() {
        let left = line.split("││").next().unwrap_or(line);
        if !started {
            if left.contains(title) {
                started = true;
            }
            continue;
        }
        if left.contains('╰') {
            break;
        }
        let body = left.trim_matches(|ch: char| "│╭╮╯ ".contains(ch));
        if !body.is_empty() {
            text.push_str(body);
            text.push(' ');
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn delisted_floor_evidence_fits_an_eighty_by_twenty_four_terminal() {
    let mut world = WorldState::for_tests();
    let id = PlayerId::new();
    world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "ada", "ada"));
    let identity = SessionIdentity::new(id, "ada", Permissions::player());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();

    for step in ["alley", "exchange", "service"] {
        world.apply(session, step).unwrap();
    }
    world.apply(session, "inspect panel").unwrap();
    let panel = left_panel(&render(world.view(session).unwrap()), "noticed");
    assert!(panel.contains("B-4"), "{panel}");
    assert!(!panel.contains("public list"), "{panel}");

    world.apply(session, "down").unwrap();
    world.apply(session, "inspect terminal").unwrap();
    let page = left_panel(&render(world.view(session).unwrap()), "noticed");
    assert!(page.contains("B-4"), "{page}");
    assert!(page.contains("public list"), "{page}");
    assert!(page.contains("relay"), "{page}");
    assert!(page.contains("journal record"), "{page}");
    assert!(!page.contains("still mounted"), "{page}");

    world.apply(session, "inspect terminal").unwrap();
    let again = left_panel(&render(world.view(session).unwrap()), "noticed");
    assert!(again.contains("nothing new"), "{again}");
    assert!(again.contains("inspect log"), "{again}");
    assert!(!again.contains("public list"), "{again}");

    world.apply(session, "inspect log").unwrap();
    let scrap = left_panel(&render(world.view(session).unwrap()), "noticed");
    assert!(scrap.contains("B-4"), "{scrap}");
    assert!(scrap.contains("public list"), "{scrap}");
    assert!(scrap.contains("relay"), "{scrap}");

    world.apply(session, "up").unwrap();
    world.apply(session, "inspect panel").unwrap();
    let linked = left_panel(&render(world.view(session).unwrap()), "noticed");
    assert!(linked.contains("B-4"), "{linked}");
    assert!(linked.contains("public list"), "{linked}");
    assert!(linked.contains("journal record"), "{linked}");

    world.apply(session, "journal record").unwrap();
    world.apply(session, "journal read delisted-floor").unwrap();
    let notes = left_panel(&render(world.view(session).unwrap()), "notes");
    assert!(notes.contains("delisted-floor"), "{notes}");
    assert!(notes.contains("public list"), "{notes}");
    assert!(notes.contains("relay"), "{notes}");
    assert!(!world.operator(id).unwrap().flags.contains("cipher-plate"));
}

#[test]
fn a_newcomer_can_read_the_shop_on_a_narrow_terminal() {
    let mut world = WorldState::for_tests();
    let id = PlayerId::new();
    world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "ada", "Ada"));
    let identity = SessionIdentity::new(id, "Ada", Permissions::player());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    let view = world.view(session).unwrap();

    let wide = render(view.clone());
    assert!(wide.contains("Coffee Shop"), "{wide}");
    assert!(wide.contains("alley"), "{wide}");
    assert!(wide.contains("arrived") || wide.contains("help"), "{wide}");

    let narrow = render_size(view.clone(), 60, 20);
    assert!(narrow.contains("You arrived") || narrow.contains("help"), "{narrow}");
    assert!(narrow.contains("Coffee") || narrow.contains("Undernet"), "{narrow}");

    let tiny = render_size(view, 40, 12);
    assert!(tiny.contains("Coffee") || tiny.contains("help") || tiny.contains("arrived"), "{tiny}");
    assert!(!tiny.contains("Terminal too small"), "{tiny}");
}
