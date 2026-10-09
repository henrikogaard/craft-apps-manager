use anyhow::{bail, Context, Result};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub fn hidden(command: &mut Command) -> &mut Command {
    command.process_group(0)
}
pub fn output(command: &mut Command) -> Result<Output> {
    Ok(hidden(command).output()?)
}
pub fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)?;
    File::open(to.parent().context("Missing parent directory")?)?.sync_all()?;
    Ok(())
}
pub struct Lock(File);
impl Lock {
    pub fn take(name: &str) -> Result<Self> {
        // TMPDIR is a private per-user folder on macOS.
        let base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches"))
            })
            .context("No per-user lock directory available")?
            .join("craft-apps-manager");
        fs::create_dir_all(&base)?;
        let filename: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(base.join(format!("{filename}.lock")))?;
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            bail!(
                "Another operation is already running: {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(Self(file))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
pub struct ProcessGroup(i32);
impl ProcessGroup {
    pub fn attach(child: &std::process::Child) -> Result<Self> {
        let pid = i32::try_from(child.id())?;
        if unsafe { libc::getpgid(pid) } != pid {
            bail!("Child process was not started in its own process group");
        }
        Ok(Self(pid))
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}
/// Removes extended attributes from a copied bundle. `ditto --noextattr` keeps
/// non-empty Finder info, which strict signature checks reject.
pub fn clear_attributes(path: &Path) -> Result<()> {
    let out = Command::new("/usr/bin/xattr")
        .arg("-cr")
        .arg(path)
        .output()?;
    if !out.status.success() {
        bail!(
            "Could not clear file attributes: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}
pub fn open(path: &Path) -> Result<()> {
    let status = Command::new("/usr/bin/open").arg(path).status()?;
    if !status.success() {
        bail!("Could not open {}", path.display());
    }
    Ok(())
}
/// Reads a string value from an app bundle's Info.plist.
pub fn bundle_value(bundle: &Path, key: &str) -> Option<String> {
    let out = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
pub fn executable_version(bundle: &Path) -> Option<String> {
    bundle_value(bundle, "CFBundleShortVersionString")
}
fn processes() -> Result<String> {
    let out = Command::new("/bin/ps")
        .args(["-axww", "-o", "comm="])
        .output()?;
    if !out.status.success() {
        bail!("Could not list running processes");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_lowercase())
}
fn running_in(processes: &str, name: &str) -> bool {
    let bundles: Vec<_> = crate::model::executable_names(name)
        .into_iter()
        .map(|bundle| format!("/{}/contents/macos/", bundle.to_lowercase()))
        .collect();
    processes
        .lines()
        .any(|line| bundles.iter().any(|bundle| line.contains(bundle)))
}
pub fn running_app(name: &str) -> Result<bool> {
    Ok(running_in(&processes()?, name))
}
/// The apps among `names` that have a running process, from a single process listing.
pub fn running_apps<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>> {
    let processes = processes()?;
    Ok(names
        .into_iter()
        .filter(|name| running_in(&processes, name))
        .map(str::to_owned)
        .collect())
}
pub fn notify(_: &Path, message: &str) -> Result<()> {
    // Pass the message as an argument so it is never parsed as AppleScript.
    let status = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 1 of argv) with title \"Craft Library\"",
            "-e",
            "end run",
            message,
        ])
        .status()?;
    if !status.success() {
        bail!("Desktop notification could not be delivered");
    }
    Ok(())
}
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod running_tests {
    use super::*;
    #[test]
    fn running_detection_matches_bundle_paths_case_insensitively() {
        let listing = "/Applications/LightCraft.app/Contents/MacOS/LightCraft\n/Applications/PrintCraft.app/Contents/MacOS/PrintCraft\n/usr/sbin/cfprefsd\n".to_lowercase();
        assert!(running_in(&listing, "lightcraft"));
        // The PdfCraft entry is also found under its former bundle name.
        assert!(running_in(&listing, "printcraft"));
        assert!(!running_in(&listing, "photocraft"));
        // A document named like an app is not a running bundle.
        assert!(!running_in("/users/me/lightcraft.app.txt\n", "lightcraft"));
    }
}
