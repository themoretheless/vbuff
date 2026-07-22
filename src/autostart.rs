//! Per-OS launch-at-login registration.
//!
//! This is the MVP `AutostartBackend`: it registers the current executable with
//! the user's login/session startup mechanism. Packaging can later replace this
//! with SMAppService, installer-managed Run keys, or systemd units, but the
//! app already has a working toggle.

use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;

const BACKGROUND_ARG: &str = "--background";

/// True when the process was launched by the native login registration.
pub fn background_requested() -> bool {
    std::env::args_os().any(|argument| argument == BACKGROUND_ARG)
}

/// Register or unregister vbuff for launch at login.
pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    platform_set_enabled(enabled, &exe)
}

#[cfg(target_os = "macos")]
fn platform_set_enabled(enabled: bool, exe: &Path) -> anyhow::Result<()> {
    let path = macos_launch_agent_path()?;
    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, macos_launch_agent_plist(exe))?;
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_set_enabled(enabled: bool, exe: &Path) -> anyhow::Result<()> {
    let path = linux_desktop_entry_path()?;
    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, linux_desktop_entry(exe))?;
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn platform_set_enabled(enabled: bool, exe: &Path) -> anyhow::Result<()> {
    let exe = exe
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("vbuff executable path is not valid UTF-8"))?;
    let run_value = windows_run_value(exe);
    let status = if enabled {
        std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "vbuff",
                "/t",
                "REG_SZ",
                "/d",
                &run_value,
                "/f",
            ])
            .status()?
    } else {
        let exists = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "vbuff",
            ])
            .status()?;
        if !exists.success() {
            return Ok(());
        }
        std::process::Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "vbuff",
                "/f",
            ])
            .status()?
    };
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("registry autostart command failed"))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform_set_enabled(_enabled: bool, _exe: &Path) -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "launch-at-login is not implemented for this platform"
    ))
}

#[cfg(target_os = "macos")]
fn macos_launch_agent_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not find home directory"))?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join("com.vbuff.vbuff.plist"))
}

#[cfg(target_os = "macos")]
fn macos_launch_agent_plist(exe: &Path) -> String {
    let exe = xml_escape(&exe.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.vbuff.vbuff</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>{BACKGROUND_ARG}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "linux")]
fn linux_desktop_entry_path() -> anyhow::Result<PathBuf> {
    let dir =
        dirs::config_dir().ok_or_else(|| anyhow::anyhow!("could not find config directory"))?;
    Ok(dir.join("autostart").join("vbuff.desktop"))
}

#[cfg(target_os = "linux")]
fn linux_desktop_entry(exe: &Path) -> String {
    let exe = desktop_exec_escape(&exe.to_string_lossy());
    format!(
        "[Desktop Entry]\nType=Application\nName=vbuff\nComment=Private clipboard manager\nExec={exe} {BACKGROUND_ARG}\nTryExec={exe}\nTerminal=false\nDBusActivatable=false\nX-GNOME-Autostart-enabled=true\nX-GNOME-Autostart-Delay=2\n"
    )
}

#[cfg(any(target_os = "windows", test))]
fn windows_run_value(exe: &str) -> String {
    format!("\"{exe}\" {BACKGROUND_ARG}")
}

#[cfg(target_os = "linux")]
fn desktop_exec_escape(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace(' ', "\\ ")
        .replace('\t', "\\\t")
        .replace('\n', "")
}

#[cfg(target_os = "macos")]
fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_exec_escapes_spaces() {
        assert_eq!(
            super::desktop_exec_escape("/tmp/vbuff app/bin"),
            "/tmp/vbuff\\ app/bin"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_plist_escapes_xml() {
        let escaped = super::xml_escape("/tmp/a&b<vbuff>");
        assert_eq!(escaped, "/tmp/a&amp;b&lt;vbuff&gt;");
    }

    #[test]
    fn startup_commands_are_explicitly_backgrounded() {
        assert_eq!(
            super::windows_run_value(r"C:\\Program Files\\vbuff.exe"),
            r#""C:\\Program Files\\vbuff.exe" --background"#
        );
        #[cfg(target_os = "macos")]
        assert!(
            super::macos_launch_agent_plist(std::path::Path::new("/Applications/vbuff"))
                .contains("<string>--background</string>")
        );
        #[cfg(target_os = "linux")]
        assert!(
            super::linux_desktop_entry(std::path::Path::new("/opt/vbuff"))
                .contains("Exec=/opt/vbuff --background")
        );
    }
}
