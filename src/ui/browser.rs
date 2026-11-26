//! Opens web pages in the browser people chose.

/// Opens `page` in the default browser if it is a safe web address, and logs
/// why if it was refused or couldn't start.
pub(super) fn open(page: &str) {
    if !can_open(page) {
        tracing::warn!("refused to open an unsafe link in the browser");
        return;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    match xdg_open(page) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return tracing::warn!(%error, "could not open a page in the browser"),
        Ok(()) => return,
    }
    if let Err(error) = webbrowser::open(page) {
        tracing::warn!(%error, "could not open a page in the browser");
    }
}

fn can_open(page: &str) -> bool {
    yapfast_core::links::openable(page)
}

/// Starts `xdg-open` without `LD_LIBRARY_PATH`: the libraries Yapfast was
/// started to find, as from its development shell or a Nix wrapper, are
/// other versions than the browser's, and Firefox exits at once on them.
/// A thread waits for `xdg-open`, which waits for a browser that wasn't
/// running yet, so a failure shows in the log.
#[cfg(all(unix, not(target_os = "macos")))]
fn xdg_open(page: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("xdg-open")
        .arg(page)
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::Builder::new()
        .name("xdg-open".into())
        .spawn(move || match child.wait() {
            Ok(status) if !status.success() => {
                tracing::warn!(%status, "the browser did not open a page");
            }
            Err(error) => tracing::warn!(%error, "could not wait for the browser"),
            Ok(_) => {}
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::can_open;

    #[test]
    fn allows_only_clean_http_links() {
        assert!(can_open("https://discord.com/invite/abc"));
        assert!(can_open("http://localhost:8080/callback"));
        assert!(!can_open("file:///C:/Windows/System32/calc.exe"));
        assert!(!can_open("ms-msdt:/id"));
        assert!(!can_open("javascript:alert(1)"));
        assert!(!can_open(r"\\attacker\share\payload.exe"));
        assert!(!can_open("https://discord.com@evil.example/"));
        assert!(!can_open("https://example.com/\u{202e}gnp.exe"));
        assert!(!can_open("https://example.com/\n"));
    }
}
