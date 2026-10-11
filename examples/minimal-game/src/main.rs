#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Demonstrates Terminus without Coffee.shop.
//!
//! A terminal runs the TUI. Anything else prints a scripted walk from the
//! lobby into the garden, so CI can execute this binary.

mod game;

use std::io::{self, IsTerminal};

use game::{scripted_demo, MiniGame};
use terminus_core::{Permissions, SessionIdentity};
use terminus_tui::{run_local, Theme};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let script = flag.as_deref() == Some("--script") || !io::stdout().is_terminal();
    let force_tui = flag.as_deref() == Some("--tui");
    if force_tui || !script {
        let identity =
            SessionIdentity::new(terminus_core::PlayerId::new(), "Guest", Permissions::player());
        run_local(MiniGame::new(), identity, Theme::neutral()).await?;
        return Ok(());
    }
    let rooms = scripted_demo();
    println!("minimal-game");
    println!("visited: {}", rooms.join(" -> "));
    println!("The engine ran look, go east, say, status, and quit.");
    Ok(())
}
