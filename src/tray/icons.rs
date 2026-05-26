use std::sync::OnceLock;

use thiserror::Error;
use tray_icon::Icon;

use crate::constants::{TRAY_ICON_ERROR_ALPHA, TRAY_ICON_RELAY};
use crate::tray::TrayStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconVariant {
    Normal,
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
    error: Icon,
}

static TRAY_ICONS: OnceLock<TrayIcons> = OnceLock::new();

fn decode_png_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), TrayIconError> {
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

    Ok((rgba, width, height))
}

fn dim_alpha(rgba: &mut [u8], factor: u8) {
    for chunk in rgba.chunks_exact_mut(4) {
        chunk[3] = ((chunk[3] as u16 * factor as u16) / 255) as u8;
    }
}

fn load_icons() -> Result<TrayIcons, TrayIconError> {
    let (rgba, width, height) = decode_png_rgba(TRAY_ICON_RELAY)?;
    let normal = Icon::from_rgba(rgba, width, height)?;
    let (mut error_rgba, width, height) = decode_png_rgba(TRAY_ICON_RELAY)?;
    dim_alpha(&mut error_rgba, TRAY_ICON_ERROR_ALPHA);
    let error = Icon::from_rgba(error_rgba, width, height)?;
    Ok(TrayIcons { normal, error })
}

fn icons() -> &'static TrayIcons {
    // Embedded compile-time asset — failure means a corrupt build artifact, not a runtime condition.
    TRAY_ICONS.get_or_init(|| load_icons().expect("embedded relay.png must decode at startup"))
}

impl TrayStatus {
    pub fn icon(&self) -> Icon {
        let set = icons();
        match self.icon_variant() {
            TrayIconVariant::Normal => set.normal.clone(),
            TrayIconVariant::Error => set.error.clone(),
        }
    }
}

/// Default menu-bar icon used at tray construction.
pub fn default_icon() -> Icon {
    icons().normal.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_alpha_scales_opacity() {
        let mut rgba = vec![255, 255, 255, 200, 0, 0, 0, 100];
        dim_alpha(&mut rgba, 128);
        assert_eq!(rgba[3], 100);
        assert_eq!(rgba[7], 50);
    }

    #[test]
    fn embedded_relay_icon_decodes() {
        let (rgba, width, height) = decode_png_rgba(TRAY_ICON_RELAY).unwrap();
        assert!(width > 0);
        assert!(height > 0);
        assert_eq!(rgba.len(), (width * height * 4) as usize);
    }

    #[test]
    fn error_variant_has_lower_alpha_than_normal() {
        let (rgba, _, _) = decode_png_rgba(TRAY_ICON_RELAY).unwrap();
        let normal_alpha: u32 = rgba.chunks_exact(4).map(|px| px[3] as u32).sum();

        let (mut error_rgba, width, height) = decode_png_rgba(TRAY_ICON_RELAY).unwrap();
        dim_alpha(&mut error_rgba, TRAY_ICON_ERROR_ALPHA);
        let error_alpha: u32 = error_rgba.chunks_exact(4).map(|px| px[3] as u32).sum();

        assert!(error_alpha < normal_alpha);
        assert!(Icon::from_rgba(error_rgba, width, height).is_ok());
    }
}
