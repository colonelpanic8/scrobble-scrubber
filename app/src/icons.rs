use dioxus::desktop::{Config, WindowBuilder};
use image::ImageReader;
use std::io::Cursor;

/// Load window icon and create desktop config
pub fn create_desktop_config_with_icon() -> Config {
    let icon_data = include_bytes!("../assets/icons/256x256.png");

    // Load and decode PNG to RGBA
    let icon = match load_window_icon(icon_data) {
        Ok(icon) => Some(icon),
        Err(e) => {
            log::warn!("Failed to load window icon: {e}");
            None
        }
    };

    // Configure the window with icon
    let mut window_builder = WindowBuilder::new()
        .with_title("Scrobble Scrubber")
        .with_resizable(true);

    if let Some(icon) = icon {
        window_builder = window_builder.with_window_icon(Some(icon));
    }

    Config::new().with_window(window_builder)
}

/// Load window icon from raw bytes
fn load_window_icon(
    icon_data: &[u8],
) -> Result<dioxus::desktop::tao::window::Icon, Box<dyn std::error::Error>> {
    use dioxus::desktop::tao::window::Icon;
    let img = ImageReader::new(Cursor::new(icon_data))
        .with_guessed_format()?
        .decode()?
        .to_rgba8();

    let (width, height) = img.dimensions();
    let rgba = img.into_raw();

    Ok(Icon::from_rgba(rgba, width, height)?)
}
