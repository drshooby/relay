use std::sync::OnceLock;

use thiserror::Error;
use tray_icon::Icon;

use crate::constants::{TRAY_ICON_RELAY, TRAY_ICON_RELAY_DISABLED, TRAY_ICON_RELAY_ERROR};
use crate::tray::TrayState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconVariant {
    Normal,
    Disabled,
    Error,
}

#[derive(Debug, Error)]
pub enum TrayIconError {
    #[error("failed to decode tray png: {0}")]
    PngDecode(#[from] png::DecodingError),
    #[error("invalid tray png layout")]
    InvalidLayout,
    #[error("failed to build tray icon: {0}")]
    BadIcon(#[from] tray_icon::BadIcon),
}

struct TrayIcons {
    normal: Icon,
    disabled: Icon,
    error: Icon,
}

static TRAY_ICONS: OnceLock<TrayIcons> = OnceLock::new();

fn decode_png(bytes: &[u8]) -> Result<Icon, TrayIconError> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let bytes = &buf[..info.buffer_size()];

    let (width, height) = (info.width, info.height);
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(bytes.len() / 3 * 4);
            for chunk in bytes.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(bytes.len() * 4);
            for g in bytes {
                rgba.extend_from_slice(&[*g, *g, *g, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(bytes.len() / 2 * 4);
            for chunk in bytes.chunks_exact(2) {
                let g = chunk[0];
                rgba.extend_from_slice(&[g, g, g, chunk[1]]);
            }
            rgba
        }
        _ => return Err(TrayIconError::InvalidLayout),
    };

    Ok(Icon::from_rgba(rgba, width, height)?)
}

fn load_icons() -> Result<TrayIcons, TrayIconError> {
    Ok(TrayIcons {
        normal: decode_png(TRAY_ICON_RELAY)?,
        disabled: decode_png(TRAY_ICON_RELAY_DISABLED)?,
        error: decode_png(TRAY_ICON_RELAY_ERROR)?,
    })
}

fn icons() -> &'static TrayIcons {
    TRAY_ICONS.get_or_init(|| load_icons().expect("embedded tray icon png assets must be valid"))
}

impl TrayState {
    pub fn icon(&self) -> Icon {
        let set = icons();
        match self.icon_variant() {
            TrayIconVariant::Normal => set.normal.clone(),
            TrayIconVariant::Disabled => set.disabled.clone(),
            TrayIconVariant::Error => set.error.clone(),
        }
    }
}

/// Default menu-bar icon used at tray construction.
pub fn default_icon() -> Icon {
    icons().normal.clone()
}
