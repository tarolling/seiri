use self_update::backends::github::{ReleaseList, Update};
use self_update::cargo_crate_version;
use std::sync::mpsc;
use std::time::Duration;

const REPO_OWNER: &str = "tarolling";
const REPO_NAME: &str = "seiri";
const BIN_NAME: &str = "seiri";
/// How long to wait for the background update check before giving up silently,
/// so a slow or unreachable network never holds up an otherwise-finished command.
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// Downloads and installs the latest seiri release from GitHub, replacing the
/// currently running binary if a newer version is available.
pub fn run_self_update(verbose: bool) -> Result<(), String> {
    // Release archives are laid out differently per OS (see .github/workflows/release.yml):
    // Windows zips hold the exe at the archive root, while macOS/Linux tarballs wrap the
    // binary in a `<bin>-<target>/` directory.
    let bin_path_in_archive = if cfg!(target_os = "windows") {
        "seiri.exe".to_string()
    } else {
        "{{ bin }}-{{ target }}/{{ bin }}".to_string()
    };

    let status = Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .bin_path_in_archive(&bin_path_in_archive)
        .current_version(cargo_crate_version!())
        .show_download_progress(verbose)
        .build()
        .map_err(|e| format!("Failed to configure updater: {e}"))?
        .update()
        .map_err(|e| format!("Failed to update: {e}"))?;

    if status.updated() {
        println!("Updated seiri to version {}.", status.version());
    } else {
        println!(
            "seiri is already up to date (version {}).",
            status.version()
        );
    }

    Ok(())
}

/// Checks GitHub for a newer release and, if one is available, prints a short
/// notice telling the user how to install it. Stays silent when already up to
/// date or when the check fails for any reason (offline, rate limited, etc.).
pub fn notify_if_update_available() {
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let _ = tx.send(latest_available_version());
    });

    if let Ok(Some(version)) = rx.recv_timeout(UPDATE_CHECK_TIMEOUT) {
        println!(
            "A new version of seiri is available (v{version}). Run `seiri --update` to install it."
        );
    }
}

/// Fetches the newest GitHub release with an asset for this platform and
/// returns its version if it's newer than the version currently running.
fn latest_available_version() -> Option<String> {
    let releases = ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .with_target(self_update::get_target())
        .build()
        .ok()?
        .fetch()
        .ok()?;

    let latest = releases.first()?;

    is_update_available(cargo_crate_version!(), &latest.version).then(|| latest.version.clone())
}

fn is_update_available(current_version: &str, candidate_version: &str) -> bool {
    self_update::version::bump_is_greater(current_version, candidate_version).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_update_available_detects_newer_version() {
        assert!(is_update_available("0.3.1", "0.4.0"));
    }

    #[test]
    fn test_is_update_available_rejects_same_or_older_version() {
        assert!(!is_update_available("0.3.1", "0.3.1"));
        assert!(!is_update_available("0.3.1", "0.2.9"));
    }

    #[test]
    fn test_is_update_available_handles_invalid_version_gracefully() {
        assert!(!is_update_available("0.3.1", "not-a-version"));
    }
}
