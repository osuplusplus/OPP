use crate::error::{CommandError, CommandResult};

const MIN_GAMMA: f64 = 0.5;
const MAX_GAMMA: f64 = 2.5;

/// Applies a gamma correction ramp to the primary display.
///
/// `SetDeviceGammaRamp` owns the display state; the setting is not persisted by OPP and may be
/// reset when the graphics driver is restarted or the user signs out.
// Some display drivers take hundreds of milliseconds to apply a ramp.
#[tauri::command(async)]
pub fn set_display_gamma(gamma: f64) -> CommandResult<()> {
    if !gamma.is_finite() || !(MIN_GAMMA..=MAX_GAMMA).contains(&gamma) {
        return Err(CommandError::new(
            "INVALID_GAMMA",
            "伽马值必须在 0.50 到 2.50 之间",
        ));
    }

    #[cfg(windows)]
    {
        use std::ffi::c_void;
        const RAMP_ENTRIES: usize = 256;
        use windows_sys::Win32::{
            Graphics::Gdi::{GetDC, ReleaseDC},
            UI::ColorSystem::SetDeviceGammaRamp,
        };

        let channel: [u16; RAMP_ENTRIES] = std::array::from_fn(|index| {
            let input = index as f64 / (RAMP_ENTRIES - 1) as f64;
            let corrected = input.powf(1.0 / gamma);
            (corrected * u16::MAX as f64).round() as u16
        });
        let ramp = [channel; 3];

        // A null HWND retrieves the desktop DC, which targets the primary display.
        let dc = unsafe { GetDC(std::ptr::null_mut()) };
        if dc.is_null() {
            return Err(CommandError::new(
                "DISPLAY_GAMMA_FAILED",
                format!("无法访问主显示器：{}", std::io::Error::last_os_error()),
            ));
        }
        let result = unsafe { SetDeviceGammaRamp(dc, ramp.as_ptr().cast::<c_void>()) };
        unsafe { ReleaseDC(std::ptr::null_mut(), dc) };
        if result == 0 {
            return Err(CommandError::new(
                "DISPLAY_GAMMA_FAILED",
                format!(
                    "Windows 未能应用显示器伽马：{}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = gamma;
        Err(CommandError::new(
            "DISPLAY_GAMMA_UNSUPPORTED",
            "显示器伽马调整仅支持 Windows",
        ))
    }
}
