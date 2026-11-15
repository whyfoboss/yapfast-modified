//! Yapfast's icon on its window. Linux desktops find it from the window's app ID,
//! which names the desktop entry in packaging/linux, which an AppImage installs
//! for itself (`crate::desktop_entry`); Windows shows the executable's icon
//! (build.rs) in Explorer, and the window loads the same one.

use slint::winit_030::winit::window::WindowAttributes;

/// The window's app ID on Linux, and the name of its desktop entry and icon.
pub const APP_ID: &str = "io.github.yapfast.Yapfast";

/// Gives each window Slint makes the icon.
pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    platform::attributes(attributes)
}

/// Matches the window icon and Windows shortcuts to the selected theme.
pub fn apply_theme(
    window: &yapfast_ui::YapfastWindow,
    accent: slint::Color,
    foreground: slint::Color,
) {
    #[cfg(windows)]
    platform::apply_theme(window, accent, foreground);
    #[cfg(not(windows))]
    let _ = (window, accent, foreground);
}

#[cfg(any(windows, test))]
fn themed_svg(accent: slint::Color, foreground: slint::Color) -> String {
    let rgb = |c: slint::Color| format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue());
    include_str!("../packaging/icon.svg")
        .replace("#ffbf47", &rgb(accent))
        .replace("#1d1607", &rgb(foreground))
}

#[cfg(target_os = "linux")]
mod platform {
    use slint::winit_030::winit::window::{Icon, WindowAttributes};

    /// X11 shows the window's own icon where the desktop entry isn't
    /// installed; Wayland takes the icon only from the entry, and ignores it.
    pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
        let icon = slint::Image::load_from_svg_data(include_bytes!("../packaging/icon.svg"))
            .ok()
            .and_then(|image| image.to_rgba8())
            .and_then(|pixels| {
                Icon::from_rgba(pixels.as_bytes().to_vec(), pixels.width(), pixels.height()).ok()
            });
        attributes.with_window_icon(icon)
    }
}

#[cfg(windows)]
mod platform {
    use slint::winit_030::winit::platform::windows::WindowExtWindows;
    use slint::winit_030::winit::{
        dpi::PhysicalSize,
        platform::windows::{IconExtWindows, WindowAttributesExtWindows},
        window::{Icon, WindowAttributes},
    };
    use slint::{ComponentHandle, winit_030::WinitWindowAccessor};
    use std::sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    };
    use windows_sys::Win32::UI::Shell::{SHCNE_UPDATEITEM, SHCNF_PATHW, SHChangeNotify};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXICON, SM_CXSMICON, SYSTEM_METRICS_INDEX,
    };

    /// The icon in packaging/windows/yapfast.rc.
    const ICON: u16 = 1;
    static SHORTCUT_UPDATE: Mutex<()> = Mutex::new(());
    static ICON_VERSION: AtomicU64 = AtomicU64::new(0);

    pub fn apply_theme(
        window: &yapfast_ui::YapfastWindow,
        accent: slint::Color,
        foreground: slint::Color,
    ) {
        use image::ImageEncoder;
        use std::os::windows::process::CommandExt;

        crate::tray::set_icon_theme(accent);

        let svg = super::themed_svg(accent, foreground);
        let Some(pixels) = slint::Image::load_from_svg_data(svg.as_bytes())
            .ok()
            .and_then(|image| image.to_rgba8())
        else {
            return;
        };
        let Ok(icon) = Icon::from_rgba(pixels.as_bytes().to_vec(), pixels.width(), pixels.height())
        else {
            return;
        };
        let _ = window.window().with_winit_window(|winit| {
            winit.set_window_icon(Some(icon.clone()));
            winit.set_taskbar_icon(Some(icon));
        });

        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let directory = std::path::PathBuf::from(local)
            .join("Yapfast")
            .join("theme-icons");
        if std::fs::create_dir_all(&directory).is_err() {
            return;
        }
        let path = directory.join(format!(
            "{:06x}-{:06x}.ico",
            accent.as_argb_encoded() & 0xffffff,
            foreground.as_argb_encoded() & 0xffffff
        ));
        if !path.exists() {
            let mut png = Vec::new();
            if image::codecs::png::PngEncoder::new(&mut png)
                .write_image(
                    pixels.as_bytes(),
                    pixels.width(),
                    pixels.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .is_err()
            {
                return;
            }
            let mut ico = vec![
                0,
                0,
                1,
                0,
                1,
                0,
                pixels.width() as u8,
                pixels.height() as u8,
                0,
                0,
                1,
                0,
                32,
                0,
            ];
            ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
            ico.extend_from_slice(&22u32.to_le_bytes());
            ico.extend_from_slice(&png);
            if std::fs::write(&path, ico).is_err() {
                return;
            }
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let script = r#"[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $w=New-Object -ComObject WScript.Shell; $paths=@((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Yapfast.lnk'),(Join-Path ([Environment]::GetFolderPath('Desktop')) 'Yapfast.lnk')); $pins=Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'; if(Test-Path -LiteralPath $pins){$paths+=@(Get-ChildItem -LiteralPath $pins -Filter '*.lnk' | ForEach-Object FullName)}; foreach($p in $paths){if(Test-Path -LiteralPath $p){$s=$w.CreateShortcut($p); if([string]::Equals($s.TargetPath,$env:YAPFAST_ICON_EXE,[StringComparison]::OrdinalIgnoreCase)){$s.IconLocation=$env:YAPFAST_ICON_PATH+',0';$s.Save(); Write-Output $p}}}"#;
        let version = ICON_VERSION.fetch_add(1, Ordering::Relaxed) + 1;
        std::thread::spawn(move || {
            let Ok(_guard) = SHORTCUT_UPDATE.lock() else {
                return;
            };
            if ICON_VERSION.load(Ordering::Relaxed) != version {
                return;
            }
            let updated = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("YAPFAST_ICON_EXE", exe)
                .env("YAPFAST_ICON_PATH", path)
                .creation_flags(0x08000000)
                .output();
            if let Ok(output) = updated
                && output.status.success()
            {
                use std::os::windows::ffi::OsStrExt;
                for shortcut in String::from_utf8_lossy(&output.stdout).lines() {
                    let path: Vec<u16> = std::ffi::OsStr::new(shortcut)
                        .encode_wide()
                        .chain(Some(0))
                        .collect();
                    // SAFETY: the path is null-terminated and lives through the call.
                    unsafe {
                        SHChangeNotify(
                            SHCNE_UPDATEITEM as i32,
                            SHCNF_PATHW,
                            path.as_ptr().cast(),
                            std::ptr::null(),
                        )
                    };
                }
            }
        });
    }

    /// The title bar's small icon and the taskbar's large one, each loaded at
    /// the size Windows draws it rather than scaled from one picture.
    pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
        let icon = |metric: SYSTEM_METRICS_INDEX| {
            // SAFETY: GetSystemMetrics only reads a setting.
            let side = unsafe { GetSystemMetrics(metric) } as u32;
            Icon::from_resource(ICON, Some(PhysicalSize::new(side, side))).ok()
        };
        attributes
            .with_window_icon(icon(SM_CXSMICON))
            .with_taskbar_icon(icon(SM_CXICON))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn theme_colours_replace_both_parts_of_the_icon() {
        let svg = super::themed_svg(
            slint::Color::from_rgb_u8(255, 255, 255),
            slint::Color::from_rgb_u8(0, 0, 0),
        );
        assert!(svg.contains("#ffffff"));
        assert!(svg.contains("#000000"));
        assert!(!svg.contains("#ffbf47"));
        assert!(!svg.contains("#1d1607"));
        assert!(slint::Image::load_from_svg_data(svg.as_bytes()).is_ok());
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    pub fn attributes(
        attributes: slint::winit_030::winit::window::WindowAttributes,
    ) -> slint::winit_030::winit::window::WindowAttributes {
        attributes
    }
}
