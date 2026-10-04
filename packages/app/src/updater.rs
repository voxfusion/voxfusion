//! Finds a newer release, downloads it, and swaps it in for the running app
//! bundle.
//!
//! What a release looks like is settled by the release pipeline and by the
//! copies already installed, which update the same way. `latest.json` names
//! a version and, per platform, an archive and its signature. The archive is
//! a gzipped tar with one top-level directory, the app bundle. The signature
//! is minisign's over the archive, base64-encoded, and is checked against
//! [`PUBLIC_KEY`] before anything is unpacked.

use base64::Engine as _;
use flate2::read::GzDecoder;
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use crate::backend::{CommandResult, UpdateDownloadEvent, UpdateInfo, Updater};
use crate::single_instance;

const ENDPOINT: &str =
    "https://github.com/voxfusion/voxfusion/releases/latest/download/latest.json";

/// The minisign public key of the release pipeline, base64-encoded as a whole
/// key file.
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEEzOTdCMUNCQjZCMDM3N0EKUldSNk43QzJ5N0dYbzhVajRWT1hkdk4rMTlqU3hJWHRhbG1xQ2xncUE3ek9iOWZTT3h6cmJIalMK";

/// A stalled connection must fail, or the interface would show a download
/// that never ends.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(30);

/// Updates the app from its GitHub releases.
pub struct AppUpdater {
    /// The release the last check offered, for the download that follows.
    offered: Mutex<Option<Release>>,
}

impl AppUpdater {
    pub fn new() -> Self {
        Self {
            offered: Mutex::new(None),
        }
    }
}

impl Updater for AppUpdater {
    fn check(&self) -> CommandResult<Option<UpdateInfo>> {
        let manifest = block_on(fetch_manifest())??;
        let current =
            Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
        let platform = platform_key(std::env::consts::OS, std::env::consts::ARCH);

        let release = manifest.release_for(&current, &platform)?;
        let info = release.as_ref().map(|release| release.info.clone());
        *self.offered.lock().unwrap_or_else(PoisonError::into_inner) = release;

        Ok(info)
    }

    fn download_and_install(
        &self,
        update: &UpdateInfo,
        on_event: &mut dyn FnMut(UpdateDownloadEvent),
    ) -> CommandResult<()> {
        if !cfg!(target_os = "macos") {
            return Err("Updates can only be installed on macOS.".into());
        }

        let release = self
            .offered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .filter(|release| release.info.version == update.version)
            .ok_or("This update is no longer on offer; check for updates again.")?;

        let executable = std::env::current_exe()
            .and_then(fs::canonicalize)
            .map_err(|error| error.to_string())?;
        let bundle = bundle_of(&executable)
            .ok_or("VoxFusion is not running from an app bundle, so there is nothing to update.")?;

        // Before the download, so that a bundle that cannot be replaced
        // where it is fails at once.
        let staging = Staging::create(&bundle)?;

        let archive = block_on(download(&release.url, on_event))??;
        verify_signature(&archive, &release.signature, PUBLIC_KEY)?;

        replace_bundle(&bundle, &staging, &archive)
    }

    /// Starts the installed copy and exits this one, without returning. What
    /// has to happen before the app exits is the caller's to do first.
    fn relaunch(&self) {
        // The new copy would otherwise find this one still running, hand over
        // to it and exit.
        single_instance::release();

        match relaunch_executable() {
            Some(executable) => {
                let spawned = Command::new(&executable)
                    .args(std::env::args_os().skip(1))
                    .spawn();

                if let Err(error) = spawned {
                    log::error!(
                        target: "updater",
                        "relaunch_failed executable={executable:?} error={error}"
                    );
                }
            }
            None => log::error!(target: "updater", "relaunch_failed error=no executable"),
        }

        std::process::exit(0);
    }
}

/// `latest.json`, as the release pipeline writes it.
#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    notes: Option<String>,
    platforms: HashMap<String, PlatformArchive>,
}

#[derive(Debug, Deserialize)]
struct PlatformArchive {
    url: String,
    signature: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Release {
    info: UpdateInfo,
    url: String,
    signature: String,
}

impl Manifest {
    /// The release to offer to a copy at version `current` on `platform`:
    /// none unless the manifest's version is newer.
    fn release_for(&self, current: &Version, platform: &str) -> Result<Option<Release>, String> {
        let version = Version::parse(self.version.trim_start_matches('v')).map_err(|error| {
            format!(
                "invalid version `{}` in the update manifest: {error}",
                self.version
            )
        })?;
        if version <= *current {
            return Ok(None);
        }

        let archive = self.platforms.get(platform).ok_or_else(|| {
            format!("the platform `{platform}` was not found on the response `platforms` object")
        })?;

        Ok(Some(Release {
            info: UpdateInfo {
                version: version.to_string(),
                body: self.notes.clone(),
            },
            url: archive.url.clone(),
            signature: archive.signature.clone(),
        }))
    }
}

/// The name of a build in the manifest's `platforms`, such as
/// `darwin-aarch64`, from Rust's names for its system and processor.
fn platform_key(os: &str, arch: &str) -> String {
    // The manifest calls macOS by the name of its kernel.
    let os = if os == "macos" { "darwin" } else { os };

    format!("{os}-{arch}")
}

/// Runs a request to its end. The updater's methods block and are called off
/// the main thread, each on a thread of its own.
fn block_on<F: Future>(future: F) -> Result<F::Output, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;

    Ok(runtime.block_on(future))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("VoxFusion/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(NETWORK_TIMEOUT)
        .read_timeout(NETWORK_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())
}

async fn fetch_manifest() -> Result<Manifest, String> {
    let response = http_client()?
        .get(ENDPOINT)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "The update manifest request failed with status: {}",
            response.status()
        ));
    }

    let body = response.bytes().await.map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| format!("invalid update manifest: {error}"))
}

/// Downloads the archive at `url` into memory, reporting each chunk.
async fn download(
    url: &str,
    on_event: &mut dyn FnMut(UpdateDownloadEvent),
) -> Result<Vec<u8>, String> {
    let mut response = http_client()?
        .get(url)
        .header(reqwest::header::ACCEPT, "application/octet-stream")
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "Download request failed with status: {}",
            response.status()
        ));
    }

    let content_length = response.content_length();
    let mut archive = Vec::new();
    let mut started = false;

    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if !started {
            started = true;
            on_event(UpdateDownloadEvent::Started { content_length });
        }

        on_event(UpdateDownloadEvent::Progress {
            chunk_length: chunk.len() as u64,
        });
        archive.extend_from_slice(&chunk);
    }
    on_event(UpdateDownloadEvent::Finished);

    Ok(archive)
}

/// Checks `signature`, the base64 of a minisign signature file, against
/// `public_key`, the base64 of a minisign public key file.
fn verify_signature(data: &[u8], signature: &str, public_key: &str) -> Result<(), String> {
    let public_key = PublicKey::decode(&decode_base64_text(public_key)?)
        .map_err(|error| format!("invalid update public key: {error}"))?;
    let signature = Signature::decode(&decode_base64_text(signature)?)
        .map_err(|error| format!("invalid update signature: {error}"))?;

    public_key
        .verify(data, &signature, true)
        .map_err(|error| format!("the update's signature does not match: {error}"))
}

fn decode_base64_text(encoded: &str) -> Result<String, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("invalid base64 in the update signature: {error}"))?;

    String::from_utf8(decoded).map_err(|error| format!("invalid update signature: {error}"))
}

/// The app bundle that contains `executable`, when it is the bundle's
/// `Contents/MacOS/<name>`.
fn bundle_of(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;

    if macos.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
        return None;
    }

    contents.parent().map(Path::to_path_buf)
}

/// The executable `bundle` starts, as its `Info.plist` names it.
fn bundle_executable(bundle: &Path) -> Option<PathBuf> {
    let contents = bundle.join("Contents");
    let info: plist::Dictionary = plist::from_file(contents.join("Info.plist")).ok()?;
    let executable = contents
        .join("MacOS")
        .join(info.get("CFBundleExecutable")?.as_string()?);

    executable.is_file().then_some(executable)
}

/// The executable to start in place of this process. In a bundle it is read
/// from `Info.plist`, since an update may have given it another name than
/// the one this process was started under.
fn relaunch_executable() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;

    match bundle_of(&executable) {
        Some(bundle) => bundle_executable(&bundle),
        None => Some(executable),
    }
}

/// A directory to prepare the new bundle in. Dropping it deletes whatever is
/// left in it: after a swap, the old bundle.
struct Staging {
    path: PathBuf,
    /// Set when the user may not change the folder the app is in, as with a
    /// standard account and the app in `/Applications`. The staging is then
    /// in the temporary directory, and it takes an administrator to move the
    /// new bundle into place.
    needs_administrator: bool,
}

impl Staging {
    /// Prefers a directory beside the bundle: that is on the same volume, so
    /// swapping the bundles is two renames.
    fn create(bundle: &Path) -> Result<Self, String> {
        let beside = beside(bundle)?;
        let failed = |path: &Path, error: io::Error| {
            format!(
                "Could not prepare the update in {}: {error}",
                path.display()
            )
        };

        // Left over if an earlier update was interrupted.
        let _ = fs::remove_dir_all(&beside);

        match fs::create_dir(&beside) {
            Ok(()) => Ok(Self {
                path: beside,
                needs_administrator: false,
            }),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                let path =
                    std::env::temp_dir().join(format!("voxfusion-update-{}", uuid::Uuid::new_v4()));
                fs::create_dir(&path).map_err(|error| failed(&path, error))?;

                Ok(Self {
                    path,
                    needs_administrator: true,
                })
            }
            // A read-only volume, such as a disk image, among others: an
            // administrator could not replace the bundle there either.
            Err(error) => Err(failed(&beside, error)),
        }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The hidden path beside `bundle` that an update uses while it replaces it.
fn beside(bundle: &Path) -> Result<PathBuf, String> {
    let (Some(folder), Some(name)) = (bundle.parent(), bundle.file_name()) else {
        return Err(format!("{} cannot be updated in place.", bundle.display()));
    };

    Ok(folder.join(format!(".{}.update", name.to_string_lossy())))
}

/// Unpacks the bundle in `archive` and puts it in the place of `bundle`.
fn replace_bundle(bundle: &Path, staging: &Staging, archive: &[u8]) -> Result<(), String> {
    let new = staging.path.join("new");

    unpack_bundle(archive, &new)
        .map_err(|error| format!("Could not unpack the update: {error}"))?;
    if bundle_executable(&new).is_none() {
        return Err("The update does not contain an app bundle.".into());
    }

    if staging.needs_administrator {
        run_as_administrator(&swap_command(bundle, &new, &beside(bundle)?)?)?;
    } else {
        swap_bundles(bundle, &new, &staging.path.join("old"))
            .map_err(|error| format!("Could not replace {}: {error}", bundle.display()))?;
    }

    // The files keep the dates they had when the release was built. A fresh
    // date on the bundle makes macOS read its `Info.plist` and icon again.
    let _ = File::open(bundle).and_then(|directory| directory.set_modified(SystemTime::now()));

    Ok(())
}

/// Moves `bundle` to `old` and `new` to where `bundle` was. The old bundle
/// is put back if the new one cannot take its place.
fn swap_bundles(bundle: &Path, new: &Path, old: &Path) -> io::Result<()> {
    fs::rename(bundle, old)?;

    if let Err(error) = fs::rename(new, bundle) {
        fs::rename(old, bundle)?;
        return Err(error);
    }

    Ok(())
}

/// The shell command that does what [`swap_bundles`] does, for an
/// administrator to run: `bundle` moves to `old`, `new` takes its place, and
/// `old` is deleted. If `new` cannot be moved, `old` is moved back instead.
fn swap_command(bundle: &Path, new: &Path, old: &Path) -> Result<String, String> {
    let quoted = |path: &Path| {
        path.to_str()
            .filter(|path| !path.chars().any(char::is_control))
            .map(shell_quoted)
            .ok_or_else(|| format!("{path:?} cannot be named in a shell command."))
    };
    let (bundle, new, old) = (quoted(bundle)?, quoted(new)?, quoted(old)?);

    Ok(format!(
        "/bin/rm -rf {old} && /bin/mv -f {bundle} {old} || exit 1; \
         if /bin/mv -f {new} {bundle}; \
         then /bin/rm -rf {old}; \
         else /bin/rm -rf {bundle}; /bin/mv -f {old} {bundle}; exit 1; fi"
    ))
}

/// `text` as one word of a shell command. Nothing has a meaning between
/// single quotes, so only the single quote itself needs a way around them.
fn shell_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The AppleScript that runs `command` in a shell as an administrator.
fn administrator_script(command: &str) -> String {
    // The two characters that have a meaning in an AppleScript string.
    let command = command.replace('\\', r"\\").replace('"', r#"\""#);

    format!(r#"do shell script "{command}" with administrator privileges"#)
}

/// Runs `command` as an administrator. macOS asks for an administrator's
/// name and password first, and nothing runs if the user cancels that.
fn run_as_administrator(command: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(administrator_script(command))
        .output()
        .map_err(|error| format!("Could not ask for an administrator's permission: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let reason = String::from_utf8_lossy(&output.stderr);
    // AppleScript's error number for a cancelled dialog.
    if reason.contains("-128") {
        Err(
            "The update was not installed: VoxFusion is in a folder that only an \
             administrator may change, and the request for a password was cancelled."
                .into(),
        )
    } else {
        Err(format!(
            "The update could not be installed as an administrator: {}",
            reason.trim()
        ))
    }
}

/// Unpacks a gzipped tar into `destination`, without the top-level directory
/// that holds everything in it: the bundle, whatever it is called.
fn unpack_bundle(archive: &[u8], destination: &Path) -> io::Result<()> {
    let mut archive = tar::Archive::new(GzDecoder::new(archive));

    for entry in archive.entries()? {
        let mut entry = entry?;
        let inside_bundle: PathBuf = entry.path()?.iter().skip(1).collect();
        let target = destination.join(inside_bundle);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read as _};
    use std::os::unix::fs::PermissionsExt as _;

    const MANIFEST: &str = r#"{
        "version": "0.9.18",
        "notes": "See release notes on GitHub",
        "pub_date": "2026-10-03T19:05:56Z",
        "platforms": {
            "darwin-aarch64": {
                "signature": "arm-signature",
                "url": "https://github.com/voxfusion/voxfusion/releases/download/v0.9.18/VoxFusion.app.tar.gz"
            },
            "darwin-x86_64": {
                "signature": "intel-signature",
                "url": "https://github.com/voxfusion/voxfusion/releases/download/v0.9.18/VoxFusion-intel.app.tar.gz"
            }
        }
    }"#;

    fn manifest(version: &str) -> Manifest {
        let mut manifest: Manifest = serde_json::from_str(MANIFEST).unwrap();
        manifest.version = version.into();
        manifest
    }

    fn version(version: &str) -> Version {
        Version::parse(version).unwrap()
    }

    #[test]
    fn reads_the_manifest_the_release_pipeline_publishes() {
        let manifest: Manifest = serde_json::from_str(MANIFEST).unwrap();
        let release = manifest
            .release_for(&version("0.9.17"), "darwin-aarch64")
            .unwrap()
            .unwrap();

        assert_eq!(
            release,
            Release {
                info: UpdateInfo {
                    version: "0.9.18".into(),
                    body: Some("See release notes on GitHub".into()),
                },
                url: "https://github.com/voxfusion/voxfusion/releases/download/v0.9.18/VoxFusion.app.tar.gz".into(),
                signature: "arm-signature".into(),
            }
        );

        let intel = manifest
            .release_for(&version("0.9.17"), "darwin-x86_64")
            .unwrap()
            .unwrap();
        assert_eq!(intel.signature, "intel-signature");
        assert!(intel.url.ends_with("/VoxFusion-intel.app.tar.gz"));
    }

    #[test]
    fn release_notes_are_optional() {
        let manifest: Manifest = serde_json::from_str(
            r#"{ "version": "1.0.0", "platforms": { "darwin-aarch64": { "url": "u", "signature": "s" } } }"#,
        )
        .unwrap();
        let release = manifest
            .release_for(&version("0.9.0"), "darwin-aarch64")
            .unwrap()
            .unwrap();

        assert_eq!(release.info.body, None);
    }

    #[test]
    fn rejects_a_manifest_without_archives() {
        assert!(serde_json::from_str::<Manifest>(r#"{ "version": "1.0.0" }"#).is_err());
        assert!(serde_json::from_str::<Manifest>(r#"{ "platforms": {} }"#).is_err());
        assert!(
            serde_json::from_str::<Manifest>(
                r#"{ "version": "1.0.0", "platforms": { "darwin-aarch64": { "url": "u" } } }"#
            )
            .is_err()
        );
    }

    #[test]
    fn offers_only_newer_versions() {
        let offered = |published: &str, current: &str| {
            manifest(published)
                .release_for(&version(current), "darwin-aarch64")
                .unwrap()
                .map(|release| release.info.version)
        };

        assert_eq!(offered("0.9.18", "0.9.17"), Some("0.9.18".into()));
        assert_eq!(offered("0.10.0", "0.9.18"), Some("0.10.0".into()));
        assert_eq!(offered("1.0.0", "0.9.18"), Some("1.0.0".into()));
        assert_eq!(offered("v0.9.18", "0.9.17"), Some("0.9.18".into()));

        assert_eq!(offered("0.9.18", "0.9.18"), None);
        assert_eq!(offered("0.9.17", "0.9.18"), None);
        assert_eq!(offered("0.9.9", "0.9.18"), None);

        // A pre-release comes before its release.
        assert_eq!(offered("1.0.0-beta.1", "1.0.0"), None);
        assert_eq!(offered("1.0.0", "1.0.0-beta.1"), Some("1.0.0".into()));
    }

    #[test]
    fn a_malformed_version_is_an_error() {
        let result = manifest("latest").release_for(&version("0.9.18"), "darwin-aarch64");

        assert!(result.unwrap_err().contains("latest"));
    }

    #[test]
    fn a_newer_release_without_this_platform_is_an_error() {
        let result = manifest("0.9.18").release_for(&version("0.9.17"), "linux-x86_64");
        assert_eq!(
            result.unwrap_err(),
            "the platform `linux-x86_64` was not found on the response `platforms` object"
        );

        let up_to_date = manifest("0.9.18").release_for(&version("0.9.18"), "linux-x86_64");
        assert_eq!(up_to_date, Ok(None));
    }

    #[test]
    fn platform_keys_are_the_ones_the_manifest_uses() {
        assert_eq!(platform_key("macos", "aarch64"), "darwin-aarch64");
        assert_eq!(platform_key("macos", "x86_64"), "darwin-x86_64");
        assert_eq!(platform_key("linux", "x86_64"), "linux-x86_64");
    }

    #[test]
    fn the_pipelines_public_key_is_a_minisign_key() {
        let key = decode_base64_text(PUBLIC_KEY).unwrap();

        assert!(key.starts_with("untrusted comment: minisign public key"));
        assert!(PublicKey::decode(&key).is_ok());
    }

    /// A key pair and a way to sign with it the way the release pipeline
    /// does: base64 of the key file, base64 of the signature file.
    struct TestKey {
        pair: minisign::KeyPair,
        public: String,
    }

    impl TestKey {
        fn generate() -> Self {
            let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
            let public = encode(&pair.pk.to_box().unwrap().to_string());

            Self { pair, public }
        }

        fn sign(&self, data: &[u8]) -> String {
            let signature = minisign::sign(
                None,
                &self.pair.sk,
                Cursor::new(data),
                Some("timestamp:1791053939\tfile:VoxFusion.app.tar.gz"),
                Some("signature from tauri secret key"),
            )
            .unwrap();

            encode(&signature.to_string())
        }
    }

    fn encode(text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    #[test]
    fn accepts_an_archive_signed_with_the_key() {
        let key = TestKey::generate();
        let archive = b"the bytes of an archive";

        assert_eq!(
            verify_signature(archive, &key.sign(archive), &key.public),
            Ok(())
        );
    }

    #[test]
    fn rejects_an_archive_that_was_changed() {
        let key = TestKey::generate();
        let signature = key.sign(b"the bytes of an archive");

        let result = verify_signature(b"the bytes of another archive", &signature, &key.public);
        assert!(result.unwrap_err().contains("does not match"));
    }

    #[test]
    fn rejects_a_signature_made_with_another_key() {
        let key = TestKey::generate();
        let other = TestKey::generate();
        let archive = b"the bytes of an archive";

        assert!(verify_signature(archive, &other.sign(archive), &key.public).is_err());
        assert!(verify_signature(archive, &other.sign(archive), PUBLIC_KEY).is_err());
    }

    #[test]
    fn rejects_signatures_that_are_not_signatures() {
        let key = TestKey::generate();
        let archive = b"the bytes of an archive";

        assert!(verify_signature(archive, "", &key.public).is_err());
        assert!(verify_signature(archive, "not base64!", &key.public).is_err());
        assert!(verify_signature(archive, &encode("not a signature"), &key.public).is_err());
        assert!(verify_signature(archive, &key.sign(archive), "not a key").is_err());
    }

    #[test]
    fn finds_the_bundle_around_the_executable() {
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/VoxFusion.app/Contents/MacOS/voxfusion"
            )),
            Some(PathBuf::from("/Applications/VoxFusion.app"))
        );
        assert_eq!(
            bundle_of(Path::new(
                "/Users/me/Applications/Vox Fusion.app/Contents/MacOS/voxfusion-app"
            )),
            Some(PathBuf::from("/Users/me/Applications/Vox Fusion.app"))
        );
    }

    #[test]
    fn an_executable_outside_a_bundle_has_none() {
        assert_eq!(
            bundle_of(Path::new("/code/voxfusion/target/debug/voxfusion")),
            None
        );
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/VoxFusion.app/Contents/Resources/bin/crispasr"
            )),
            None
        );
        assert_eq!(bundle_of(Path::new("/opt/MacOS/voxfusion")), None);
        assert_eq!(bundle_of(Path::new("voxfusion")), None);
    }

    const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>EXECUTABLE</string>
</dict>
</plist>
"#;

    /// Writes an app bundle whose executable is `executable` and contains
    /// `marker`.
    fn write_bundle(bundle: &Path, executable: &str, marker: &str) {
        let macos = bundle.join("Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        fs::create_dir_all(bundle.join("Contents/Resources/bin")).unwrap();

        fs::write(
            bundle.join("Contents/Info.plist"),
            INFO_PLIST.replace("EXECUTABLE", executable),
        )
        .unwrap();
        fs::write(macos.join(executable), marker).unwrap();
        fs::set_permissions(macos.join(executable), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(bundle.join("Contents/Resources/bin/engine"), marker).unwrap();
    }

    /// Packs the bundle the way the release pipeline does: a gzipped tar
    /// with the bundle as its only top-level entry.
    fn pack_bundle(bundle: &Path) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        archive
            .append_dir_all(bundle.file_name().unwrap(), bundle)
            .unwrap();

        archive.into_inner().unwrap().finish().unwrap()
    }

    fn read(path: PathBuf) -> String {
        let mut contents = String::new();
        File::open(path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        contents
    }

    fn entries(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn reads_the_executable_from_info_plist() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion", "binary");

        assert_eq!(
            bundle_executable(&bundle),
            Some(bundle.join("Contents/MacOS/voxfusion"))
        );

        fs::remove_file(bundle.join("Contents/MacOS/voxfusion")).unwrap();
        assert_eq!(bundle_executable(&bundle), None);
        assert_eq!(bundle_executable(folder.path()), None);
    }

    #[test]
    fn replaces_the_bundle_with_the_one_in_the_archive() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion-app", "old");
        fs::write(bundle.join("Contents/Resources/removed-in-new-version"), "").unwrap();

        // The new version is built elsewhere, under another name, and its
        // executable is called differently.
        let build = tempfile::tempdir().unwrap();
        write_bundle(&build.path().join("Built.app"), "voxfusion", "new");
        let archive = pack_bundle(&build.path().join("Built.app"));

        let staging = Staging::create(&bundle).unwrap();
        replace_bundle(&bundle, &staging, &archive).unwrap();

        let executable = bundle.join("Contents/MacOS/voxfusion");
        assert_eq!(bundle_executable(&bundle), Some(executable.clone()));
        assert_eq!(read(executable.clone()), "new");
        assert_eq!(read(bundle.join("Contents/Resources/bin/engine")), "new");
        assert_eq!(
            executable.metadata().unwrap().permissions().mode() & 0o777,
            0o755
        );

        // Nothing of the old bundle is left inside the new one.
        assert_eq!(entries(&bundle.join("Contents/MacOS")), ["voxfusion"]);
        assert_eq!(entries(&bundle.join("Contents/Resources")), ["bin"]);

        // The old bundle is around until the staging goes.
        assert_eq!(
            read(staging.path.join("old/Contents/MacOS/voxfusion-app")),
            "old"
        );
        drop(staging);
        assert_eq!(entries(folder.path()), ["VoxFusion.app"]);
    }

    #[test]
    fn the_replaced_bundle_gets_a_fresh_date() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion", "old");

        let build = tempfile::tempdir().unwrap();
        let built = build.path().join("Built.app");
        write_bundle(&built, "voxfusion", "new");
        let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        File::open(&built).unwrap().set_modified(long_ago).unwrap();

        let before = SystemTime::now() - Duration::from_secs(5);
        let staging = Staging::create(&bundle).unwrap();
        replace_bundle(&bundle, &staging, &pack_bundle(&built)).unwrap();

        assert!(bundle.metadata().unwrap().modified().unwrap() > before);
    }

    #[test]
    fn keeps_the_bundle_when_the_archive_has_no_bundle() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion", "old");

        let build = tempfile::tempdir().unwrap();
        let not_a_bundle = build.path().join("Built.app");
        fs::create_dir_all(not_a_bundle.join("Contents")).unwrap();
        fs::write(not_a_bundle.join("Contents/readme"), "").unwrap();

        let staging = Staging::create(&bundle).unwrap();
        let error = replace_bundle(&bundle, &staging, &pack_bundle(&not_a_bundle)).unwrap_err();

        assert_eq!(error, "The update does not contain an app bundle.");
        assert_eq!(read(bundle.join("Contents/MacOS/voxfusion")), "old");
    }

    #[test]
    fn keeps_the_bundle_when_the_archive_is_damaged() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion", "old");

        let build = tempfile::tempdir().unwrap();
        write_bundle(&build.path().join("Built.app"), "voxfusion", "new");
        let mut archive = pack_bundle(&build.path().join("Built.app"));
        archive.truncate(archive.len() / 2);

        let staging = Staging::create(&bundle).unwrap();

        assert!(replace_bundle(&bundle, &staging, &archive).is_err());
        assert!(replace_bundle(&bundle, &staging, b"not an archive").is_err());
        assert_eq!(read(bundle.join("Contents/MacOS/voxfusion")), "old");
    }

    #[test]
    fn puts_the_old_bundle_back_when_the_new_one_cannot_take_its_place() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion", "old");
        let staging = Staging::create(&bundle).unwrap();

        let missing = staging.path.join("new");
        let old = staging.path.join("old");

        assert!(swap_bundles(&bundle, &missing, &old).is_err());
        assert_eq!(read(bundle.join("Contents/MacOS/voxfusion")), "old");
        assert!(!old.exists());
    }

    #[test]
    fn staging_starts_empty_beside_the_bundle() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion", "old");

        // What an interrupted update left behind is cleared.
        let leftover = folder.path().join(".VoxFusion.app.update/new/Contents");
        fs::create_dir_all(&leftover).unwrap();

        let staging = Staging::create(&bundle).unwrap();

        assert_eq!(staging.path, folder.path().join(".VoxFusion.app.update"));
        assert!(!staging.needs_administrator);
        assert!(entries(&staging.path).is_empty());
    }

    #[test]
    fn a_folder_that_cannot_be_changed_takes_an_administrator() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("VoxFusion.app");
        write_bundle(&bundle, "voxfusion", "old");
        fs::set_permissions(folder.path(), fs::Permissions::from_mode(0o555)).unwrap();

        let staging = Staging::create(&bundle);

        fs::set_permissions(folder.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let staging = staging.unwrap();
        assert!(staging.needs_administrator);

        // The new bundle is prepared elsewhere; the folder is left alone.
        assert!(staging.path.starts_with(std::env::temp_dir()));
        assert!(!staging.path.starts_with(folder.path()));
        assert!(entries(&staging.path).is_empty());
        assert_eq!(entries(folder.path()), ["VoxFusion.app"]);

        let path = staging.path.clone();
        drop(staging);
        assert!(!path.exists());
    }

    #[test]
    fn a_folder_that_is_missing_is_an_error() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("gone/VoxFusion.app");

        assert!(Staging::create(&bundle).is_err());
        assert!(Staging::create(Path::new("/")).is_err());
    }

    /// Names with everything a shell or AppleScript could trip over.
    const AWKWARD_FOLDER: &str = r#"Apps & "Tools" it's $HOME `id` \back\slash"#;
    const AWKWARD_BUNDLE: &str = r#"Vox "Fusion" (it's new).app"#;

    fn run_in_shell(command: &str) -> bool {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .status()
            .unwrap()
            .success()
    }

    #[test]
    fn the_administrators_command_swaps_the_bundles() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join(AWKWARD_FOLDER);
        let bundle = folder.join(AWKWARD_BUNDLE);
        let new = root.path().join("staging 'elsewhere'").join("new");
        let old = beside(&bundle).unwrap();

        write_bundle(&bundle, "voxfusion", "old");
        write_bundle(&new, "voxfusion", "new");
        // What an interrupted update left behind is cleared.
        fs::create_dir_all(old.join("leftover")).unwrap();

        assert!(run_in_shell(&swap_command(&bundle, &new, &old).unwrap()));

        assert_eq!(read(bundle.join("Contents/MacOS/voxfusion")), "new");
        assert_eq!(entries(&folder), [AWKWARD_BUNDLE]);
        assert!(!new.exists());
        // Nothing else was touched or created by a name read wrongly.
        assert_eq!(
            entries(root.path()),
            [AWKWARD_FOLDER, "staging 'elsewhere'"]
        );
    }

    #[test]
    fn the_administrators_command_puts_the_old_bundle_back() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join(AWKWARD_FOLDER);
        let bundle = folder.join(AWKWARD_BUNDLE);
        let missing = root.path().join("staging").join("new");
        let old = beside(&bundle).unwrap();
        write_bundle(&bundle, "voxfusion", "old");

        assert!(!run_in_shell(
            &swap_command(&bundle, &missing, &old).unwrap()
        ));

        assert_eq!(read(bundle.join("Contents/MacOS/voxfusion")), "old");
        assert_eq!(entries(&folder), [AWKWARD_BUNDLE]);
    }

    #[test]
    fn the_administrators_command_changes_nothing_without_a_bundle() {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("VoxFusion.app");
        let new = root.path().join("staging").join("new");
        write_bundle(&new, "voxfusion", "new");

        assert!(!run_in_shell(
            &swap_command(&bundle, &new, &beside(&bundle).unwrap()).unwrap()
        ));

        assert_eq!(read(new.join("Contents/MacOS/voxfusion")), "new");
        assert_eq!(entries(root.path()), ["staging"]);
    }

    #[test]
    fn the_administrators_command_names_its_paths_literally() {
        let command = swap_command(
            Path::new("/Applications/Vox Fusion's.app"),
            Path::new("/tmp/staging/new"),
            Path::new("/Applications/.Vox Fusion's.app.update"),
        )
        .unwrap();

        assert_eq!(
            command,
            r"/bin/rm -rf '/Applications/.Vox Fusion'\''s.app.update' && /bin/mv -f '/Applications/Vox Fusion'\''s.app' '/Applications/.Vox Fusion'\''s.app.update' || exit 1; if /bin/mv -f '/tmp/staging/new' '/Applications/Vox Fusion'\''s.app'; then /bin/rm -rf '/Applications/.Vox Fusion'\''s.app.update'; else /bin/rm -rf '/Applications/Vox Fusion'\''s.app'; /bin/mv -f '/Applications/.Vox Fusion'\''s.app.update' '/Applications/Vox Fusion'\''s.app'; exit 1; fi"
        );
    }

    #[test]
    fn paths_a_command_cannot_name_are_refused() {
        use std::os::unix::ffi::OsStrExt as _;

        let fine = Path::new("/Applications/VoxFusion.app");
        let with_newline = Path::new("/Applications/Vox\nFusion.app");
        let not_unicode = Path::new(std::ffi::OsStr::from_bytes(
            b"/Applications/Vox\xffFusion.app",
        ));

        assert!(swap_command(with_newline, fine, fine).is_err());
        assert!(swap_command(fine, not_unicode, fine).is_err());
        assert!(swap_command(fine, fine, with_newline).is_err());
    }

    /// Reads an AppleScript string literal the way AppleScript does.
    fn applescript_string(literal: &str) -> String {
        let mut text = String::new();
        let mut characters = literal.chars();

        while let Some(character) = characters.next() {
            match character {
                '\\' => text.push(characters.next().expect("a dangling backslash")),
                '"' => panic!("the string ends early: {literal}"),
                other => text.push(other),
            }
        }

        text
    }

    #[test]
    fn the_administrators_script_carries_the_command_unchanged() {
        assert_eq!(
            administrator_script(r#"/bin/mv -f '/a "b" \c' '/d'"#),
            r#"do shell script "/bin/mv -f '/a \"b\" \\c' '/d'" with administrator privileges"#
        );

        let bundle = Path::new("/").join(AWKWARD_FOLDER).join(AWKWARD_BUNDLE);
        let command =
            swap_command(&bundle, Path::new("/tmp/new"), &beside(&bundle).unwrap()).unwrap();
        let script = administrator_script(&command);

        let literal = script
            .strip_prefix("do shell script \"")
            .and_then(|rest| rest.strip_suffix("\" with administrator privileges"))
            .unwrap();
        assert_eq!(applescript_string(literal), command);
    }
}
