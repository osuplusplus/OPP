//! 显示器伽马调整工具（仅 Windows）：用 GDI `SetDeviceGammaRamp` 修改主
//! 显示器红绿蓝三通道的伽马查找表；非 Windows 平台直接返回不支持。

use std::ffi::c_void;

use crate::error::{CommandError, CommandResult};

/// 伽马查找表每通道的条目数（`SetDeviceGammaRamp` 约定为 256 项）。
const RAMP_ENTRIES: usize = 256;
/// 伽马下限：更低的值会把大部分灰阶压到接近纯黑，屏幕不可用。
const MIN_GAMMA: f64 = 0.5;
/// 伽马上限：更高的值会让画面过曝泛白。
const MAX_GAMMA: f64 = 2.5;

/// 供前端调用的 Tauri 命令：把主显示器伽马设置为 `gamma`（有效范围
/// 0.50–2.50，按查表公式 gamma>1 提亮、gamma<1 变暗）。
/// 声明为 `async` 命令：部分显示驱动应用查表耗时数百毫秒，避免阻塞主线程。
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
        use windows_sys::Win32::{
            Graphics::Gdi::{GetDC, ReleaseDC},
            UI::ColorSystem::SetDeviceGammaRamp,
        };

        // 每通道查表：输出 = 输入^(1/gamma)，线性输入均匀采样后量化到 u16 全量程。
        let channel: [u16; RAMP_ENTRIES] = std::array::from_fn(|index| {
            let input = index as f64 / (RAMP_ENTRIES - 1) as f64;
            let corrected = input.powf(1.0 / gamma);
            (corrected * u16::MAX as f64).round() as u16
        });
        // 三通道共用同一条曲线；API 期望的内存布局是 R、G、B 三段各 256 项。
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
