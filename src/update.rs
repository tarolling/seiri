use self_update::backends::github::Update;
use self_update::cargo_crate_version;

const REPO_OWNER: &str = "tarolling";
const REPO_NAME: &str = "seiri";
const BIN_NAME: &str = "seiri";

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
