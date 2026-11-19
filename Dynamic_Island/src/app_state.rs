//! Shared constants. They must match the numbers in ui/main.slint and tokens.slint.

/// Port of the local notification webhook (127.0.0.1 only).
pub const WEBHOOK_PORT: u16 = 47823;

/// City shown in the weather widget when your location cannot be found.
pub const WEATHER_CITY: &str = "Delhi";

/// Gap between the screen top and the island (logical px).
pub const ISLAND_TOP: f32 = 6.0;

/// Extra clickable margin around the island (logical px).
pub const REGION_PAD: f32 = 10.0;

/// How many clipboard entries are remembered.
pub const CLIPBOARD_MAX: usize = 20;