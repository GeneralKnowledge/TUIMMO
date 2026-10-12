//! Coffee.shop command catalogue (registry metadata for docs and hints).

use terminus_core::CommandRegistry;

use crate::state::commands;

pub fn coffee_registry() -> CommandRegistry {
    commands::coffee_registry(&commands::coffee_commands())
}
