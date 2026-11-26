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

/// Sets the initial theme colours used before the native window is created.
pub fn set_initial_theme(accent: slint::Color, foreground: slint::Color) {
    #[cfg(windows)]
    platform::set_initial_theme(accent, foreground);
    #[cfg(not(windows))]
    let _ = (accent, foreground);
}

/// Reapplies the current theme icons onto an existing native window.
pub fn reapply_window_icon(window: &yapfast_ui::YapfastWindow) {
    #[cfg(windows)]
    platform::reapply_window_icon(window);
    #[cfg(not(windows))]
    let _ = window;
}

/// Matches the window icon, tray, toast, and Windows shortcuts to the selected theme.
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

#[cfg(test)]
fn themed_svg(accent: slint::Color, foreground: slint::Color) -> String {
    themed_svg_sized(64, accent, foreground)
}

#[cfg(any(windows, test))]
fn themed_svg_sized(size: u32, accent: slint::Color, foreground: slint::Color) -> String {
    let rgb = |c: slint::Color| format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue());
    include_str!("../packaging/icon.svg")
        .replacen(
            r#"width="64" height="64""#,
            &format!(r#"width="{size}" height="{size}""#),
            1,
        )
        .replace("#ffbf47", &rgb(accent))
        .replace("#1d1607", &rgb(foreground))
}

#[cfg(any(windows, test))]
pub(crate) fn render_rgba(
    size: u32,
    accent: slint::Color,
    foreground: slint::Color,
) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let svg = themed_svg_sized(size, accent, foreground);
    slint::Image::load_from_svg_data(svg.as_bytes())
        .ok()?
        .to_rgba8()
}

#[cfg(any(windows, test))]
pub(crate) fn render_png(
    size: u32,
    accent: slint::Color,
    foreground: slint::Color,
) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    let pixels = render_rgba(size, accent, foreground)?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(png)
}

#[cfg(any(windows, test))]
fn build_multi_ico(entries: &[(u32, &[u8])]) -> Vec<u8> {
    let count = u16::try_from(entries.len()).unwrap_or(0);
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6usize + entries.len() * 16;
    for &(size, png) in entries {
        let dim = if size >= 256 { 0u8 } else { size as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for &(_, png) in entries {
        ico.extend_from_slice(png);
    }
    ico
}

#[cfg(any(windows, test))]
fn should_update_shortcuts(
    applied_path: &std::path::Path,
    exe: &std::path::Path,
    ico_path: &std::path::Path,
) -> bool {
    let expected = format!("{}\n{}", exe.display(), ico_path.display());
    std::fs::read_to_string(applied_path).ok().as_deref() != Some(expected.as_str())
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
    static THEME_COLORS: Mutex<(u32, u32)> = Mutex::new((0xffff_bf47, 0xff1d_1607));

    fn theme_colors() -> (slint::Color, slint::Color) {
        let (accent, foreground) = *THEME_COLORS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            slint::Color::from_argb_encoded(accent),
            slint::Color::from_argb_encoded(foreground),
        )
    }

    pub fn set_initial_theme(accent: slint::Color, foreground: slint::Color) {
        *THEME_COLORS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            (accent.as_argb_encoded(), foreground.as_argb_encoded());
        crate::tray::set_icon_theme(accent, foreground);
        crate::notify::set_icon_theme(accent, foreground);
    }

    pub fn reapply_window_icon(window: &yapfast_ui::YapfastWindow) {
        let (accent, foreground) = theme_colors();
        let small = icon_at(SM_CXSMICON, accent, foreground);
        let large = icon_at(SM_CXICON, accent, foreground);
        let _ = window.window().with_winit_window(|winit| {
            if let Some(small) = small {
                winit.set_window_icon(Some(small));
            }
            if let Some(large) = large {
                winit.set_taskbar_icon(Some(large));
            }
        });
    }

    fn icon_at(
        metric: SYSTEM_METRICS_INDEX,
        accent: slint::Color,
        foreground: slint::Color,
    ) -> Option<Icon> {
        // SAFETY: GetSystemMetrics only reads a setting.
        let side = (unsafe { GetSystemMetrics(metric) } as u32).clamp(16, 256);
        super::render_rgba(side, accent, foreground)
            .and_then(|pixels| {
                Icon::from_rgba(pixels.as_bytes().to_vec(), pixels.width(), pixels.height()).ok()
            })
            .or_else(|| Icon::from_resource(ICON, Some(PhysicalSize::new(side, side))).ok())
    }

    pub fn apply_theme(
        window: &yapfast_ui::YapfastWindow,
        accent: slint::Color,
        foreground: slint::Color,
    ) {
        use std::os::windows::process::CommandExt;

        set_initial_theme(accent, foreground);
        reapply_window_icon(window);

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
            let (Some(png32), Some(png64), Some(png256)) = (
                super::render_png(32, accent, foreground),
                super::render_png(64, accent, foreground),
                super::render_png(256, accent, foreground),
            ) else {
                return;
            };
            let ico = super::build_multi_ico(&[(32, &png32), (64, &png64), (256, &png256)]);
            if std::fs::write(&path, ico).is_err() {
                return;
            }
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let applied_path = directory.join("applied.txt");
        if !super::should_update_shortcuts(&applied_path, &exe, &path) {
            return;
        }
        let expected_applied = format!("{}\n{}", exe.display(), path.display());
        let script = r#"[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $w=New-Object -ComObject WScript.Shell; $paths=@((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Yapfast.lnk'),(Join-Path ([Environment]::GetFolderPath('Desktop')) 'Yapfast.lnk')); $pins=Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'; if(Test-Path -LiteralPath $pins){$paths+=@(Get-ChildItem -LiteralPath $pins -Filter '*.lnk' | ForEach-Object FullName)}; foreach($p in $paths){if(Test-Path -LiteralPath $p){$s=$w.CreateShortcut($p); if([string]::Equals($s.TargetPath,$env:YAPFAST_ICON_EXE,[StringComparison]::OrdinalIgnoreCase)){$s.IconLocation=$env:YAPFAST_ICON_PATH+',0';$s.Save(); Write-Output $p}}}"#;
        let version = ICON_VERSION.fetch_add(1, Ordering::Relaxed) + 1;
        std::thread::spawn(move || {
            let Ok(_guard) = SHORTCUT_UPDATE.lock() else {
                return;
            };
            if ICON_VERSION.load(Ordering::Relaxed) != version {
                return;
            }
            if !super::should_update_shortcuts(&applied_path, &exe, &path) {
                return;
            }
            let updated = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("YAPFAST_ICON_EXE", &exe)
                .env("YAPFAST_ICON_PATH", &path)
                .creation_flags(0x08000000)
                .output();
            if let Ok(output) = updated
                && output.status.success()
            {
                let _ = std::fs::write(&applied_path, expected_applied);
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

    /// The title bar's small icon and the taskbar's large one, each rendered at
    /// the size Windows draws it for the saved theme.
    pub fn attributes(attributes: WindowAttributes) -> WindowAttributes {
        let (accent, foreground) = theme_colors();
        attributes
            .with_window_icon(icon_at(SM_CXSMICON, accent, foreground))
            .with_taskbar_icon(icon_at(SM_CXICON, accent, foreground))
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

    #[test]
    fn themed_multi_size_ico_has_32_64_and_256_entries() {
        let png32 = super::render_png(32, slint::Color::from_rgb_u8(255, 255, 255), slint::Color::from_rgb_u8(0, 0, 0)).unwrap();
        let png64 = super::render_png(64, slint::Color::from_rgb_u8(255, 255, 255), slint::Color::from_rgb_u8(0, 0, 0)).unwrap();
        let png256 = super::render_png(256, slint::Color::from_rgb_u8(255, 255, 255), slint::Color::from_rgb_u8(0, 0, 0)).unwrap();
        let ico = super::build_multi_ico(&[(32, &png32), (64, &png64), (256, &png256)]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 3);
        assert_eq!(ico[6], 32);
        assert_eq!(ico[6 + 16], 64);
        assert_eq!(ico[6 + 32], 0, "256px width is encoded as 0 in ICO");
    }

    #[test]
    fn shortcut_cache_skips_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let applied = dir.path().join("applied.txt");
        let exe = std::path::Path::new("C:\\Yapfast\\yapfast.exe");
        let ico = std::path::Path::new("C:\\Icons\\a.ico");
        assert!(super::should_update_shortcuts(&applied, exe, ico));
        std::fs::write(&applied, format!("{}\n{}", exe.display(), ico.display())).unwrap();
        assert!(!super::should_update_shortcuts(&applied, exe, ico));
        let ico2 = std::path::Path::new("C:\\Icons\\b.ico");
        assert!(super::should_update_shortcuts(&applied, exe, ico2));
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
