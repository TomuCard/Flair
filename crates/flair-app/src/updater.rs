//! Updates over the network: finding the latest release on GitHub,
//! downloading and verifying it, replacing the executable, and the startup
//! health check that rolls back a version that keeps crashing.
//!
//! All blocking: run from a background thread, never on the UI thread.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use flair_core::settings::config::UpdateChannel;
use flair_core::update::{self, Manifest, UpdateError};
use semver::Version;
use serde::{Deserialize, Serialize};

/// Largest accepted download (the executable is ~15 MB).
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
/// Automatic checks happen at most this often, and only when Flair is shown.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// Where releases are looked up. Not configurable by the user (tests only).
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub api: String,
    pub repository: String,
    pub https_only: bool,
    pub public_key: String,
}

impl Endpoint {
    pub fn github() -> Self {
        Self {
            api: "https://api.github.com".into(),
            repository: update::REPOSITORY.into(),
            https_only: true,
            public_key: update::PUBLIC_KEY.into(),
        }
    }

    /// Releases page, for manual downloads.
    pub fn releases_page(&self) -> String {
        format!("https://github.com/{}/releases", self.repository)
    }

    fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .https_only(self.https_only)
            .timeout_global(Some(Duration::from_secs(60)))
            // Generic name: no user or machine information is sent.
            .user_agent("flair-updater")
            .build()
            .into()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(String),
    #[error("no release found on the {0} channel")]
    NoRelease(String),
    #[error(transparent)]
    Update(#[from] UpdateError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Self::Network(e.to_string())
    }
}

/// A published release and the URLs of its files.
#[derive(Debug, Clone)]
pub struct Release {
    pub manifest: Manifest,
    /// File name → download URL.
    pub files: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, Error> {
    let mut response = agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()?;
    Ok(response.body_mut().read_to_string()?)
}

/// Latest release of `channel`: `main` follows every release (pre-releases
/// from `main` and stable versions), `stable` only tagged versions.
pub fn latest(endpoint: &Endpoint, channel: UpdateChannel) -> Result<Release, Error> {
    let agent = endpoint.agent();
    let url = format!(
        "{}/repos/{}/releases?per_page=50",
        endpoint.api, endpoint.repository
    );
    let releases: Vec<GithubRelease> = serde_json::from_str(&get_text(&agent, &url)?)
        .map_err(|e| Error::Network(format!("unexpected GitHub answer: {e}")))?;
    let channel_name = match channel {
        UpdateChannel::Main => "main",
        UpdateChannel::Stable => "stable",
    };
    let best = releases
        .into_iter()
        .filter(|r| !r.draft && (channel == UpdateChannel::Main || !r.prerelease))
        .filter_map(|r| Some((Version::parse(r.tag_name.trim_start_matches('v')).ok()?, r)))
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, r)| r)
        .ok_or_else(|| Error::NoRelease(channel_name.into()))?;
    let files: BTreeMap<String, String> = best
        .assets
        .into_iter()
        .map(|a| (a.name, a.browser_download_url))
        .collect();
    let manifest_url = files
        .get("manifest.json")
        .ok_or_else(|| Error::NoRelease(channel_name.into()))?;
    let manifest = Manifest::parse(&get_text(&agent, manifest_url)?)?;
    Ok(Release { manifest, files })
}

/// Downloads `url`, reporting `(received, total)` bytes.
pub fn download(
    endpoint: &Endpoint,
    url: &str,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<Vec<u8>, Error> {
    let mut response = endpoint.agent().get(url).call()?;
    let total = response.body().content_length();
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .reader();
    let mut data = Vec::with_capacity(usize::try_from(total.unwrap_or(0)).unwrap_or(0));
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| Error::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..n]);
        progress(data.len() as u64, total);
    }
    if let Some(total) = total
        && data.len() as u64 != total
    {
        return Err(Error::Network("download interrupted".into()));
    }
    Ok(data)
}

/// Downloads the binary of the running platform and its signature, then
/// verifies both before returning the binary. Nothing is written to disk.
pub fn fetch_verified(
    endpoint: &Endpoint,
    release: &Release,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<Vec<u8>, Error> {
    let asset = release.manifest.asset().ok_or(UpdateError::NoAsset)?;
    let url_of = |name: &str| {
        release
            .files
            .get(name)
            .cloned()
            .ok_or(Error::Update(UpdateError::NoAsset))
    };
    let signature = String::from_utf8(download(endpoint, &url_of(&asset.signature)?, &|_, _| {})?)
        .map_err(|_| UpdateError::BadSignature)?;
    let binary = download(endpoint, &url_of(&asset.file)?, progress)?;
    update::verify(&binary, &signature, &release.manifest, &endpoint.public_key)?;
    Ok(binary)
}

// ------------------------------------------------------- install, restart

/// Files of the update machinery, in the data folder.
#[derive(Debug, Clone)]
pub struct Files {
    /// Temporary downloads.
    pub updates_dir: PathBuf,
    /// Startup health of a freshly installed version.
    pub health: PathBuf,
    /// Written when an automatic rollback happened, read once to tell the user.
    pub rolled_back: PathBuf,
    /// Time of the last automatic check.
    pub last_check: PathBuf,
}

impl Files {
    pub fn new(data_dir: &Path) -> Self {
        let dir = data_dir.join("updates");
        Self {
            health: dir.join("health.json"),
            rolled_back: dir.join("rolled-back.json"),
            last_check: dir.join("last-check"),
            updates_dir: dir,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Health {
    version: String,
    previous_version: String,
    failed_starts: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RolledBack {
    pub from: String,
    pub to: String,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    fs::write(path, json)
}

/// Installs a verified binary in place of `exe` (the old one is kept as
/// `flair.previous`) and arms the startup health check.
pub fn install(
    binary: &[u8],
    exe: &Path,
    files: &Files,
    from_version: &str,
    to_version: &str,
) -> Result<(), Error> {
    fs::create_dir_all(&files.updates_dir)?;
    let download = files
        .updates_dir
        .join(format!("flair-{to_version}.download"));
    fs::write(&download, binary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&download, fs::Permissions::from_mode(0o755))?;
    }
    let result = flair_platform::install::replace_file(&download, exe);
    let _ = fs::remove_file(&download);
    result?;
    write_json(
        &files.health,
        &Health {
            version: to_version.into(),
            previous_version: from_version.into(),
            failed_starts: 0,
        },
    )?;
    Ok(())
}

/// What the startup health check decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guard {
    /// Start normally.
    Continue,
    /// The previous version was restored and started: exit now.
    RolledBack,
}

/// The very first thing `main` runs. A freshly installed version that
/// crashed twice before confirming its startup is replaced by the previous
/// one, which is started instead.
pub fn startup_guard(exe: &Path, files: &Files) -> Guard {
    let Some(mut health) = read_json::<Health>(&files.health) else {
        return Guard::Continue;
    };
    if health.failed_starts >= 2 {
        let previous = flair_platform::install::previous_exe(exe);
        match swap_with_previous(exe) {
            Ok(()) => {
                let _ = write_json(
                    &files.rolled_back,
                    &RolledBack {
                        from: health.version.clone(),
                        to: health.previous_version.clone(),
                    },
                );
                let _ = fs::remove_file(&files.health);
                let _ = previous; // now holds the failed version
                if flair_platform::process::spawn_detached(exe, &[]).is_ok() {
                    return Guard::RolledBack;
                }
            }
            Err(e) => eprintln!("flair: automatic rollback failed: {e}"),
        }
        let _ = fs::remove_file(&files.health);
        return Guard::Continue;
    }
    health.failed_starts += 1;
    let _ = write_json(&files.health, &health);
    Guard::Continue
}

/// The new version started correctly (window ready, shortcut registered).
pub fn confirm_started(files: &Files) {
    if fs::remove_file(&files.health).is_ok() {
        tracing::info!("update confirmed: startup succeeded");
    }
}

/// Reads (once) the notice of an automatic rollback.
pub fn take_rollback_notice(files: &Files) -> Option<RolledBack> {
    let notice = read_json(&files.rolled_back);
    let _ = fs::remove_file(&files.rolled_back);
    notice
}

/// Exchanges `exe` and `flair.previous` (`> update rollback`, automatic rollback).
pub fn swap_with_previous(exe: &Path) -> std::io::Result<()> {
    let previous = flair_platform::install::previous_exe(exe);
    if !previous.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no previous version kept",
        ));
    }
    let parking = exe.with_file_name(format!("{}.swap", flair_platform::install::exe_name()));
    let _ = fs::remove_file(&parking);
    fs::rename(exe, &parking)?;
    if let Err(e) = fs::rename(&previous, exe) {
        let _ = fs::rename(&parking, exe);
        return Err(e);
    }
    fs::rename(&parking, &previous)
}

/// Starts the new executable, which waits for this process to exit.
pub fn restart(exe: &Path, flag: &str) -> std::io::Result<()> {
    let pid = std::process::id().to_string();
    flair_platform::process::spawn_detached(exe, &[flag, "--wait-pid", &pid])
}

// --------------------------------------------------------- periodic check

/// True when the last automatic check is older than [`CHECK_INTERVAL`].
pub fn check_due(files: &Files) -> bool {
    let Ok(modified) = fs::metadata(&files.last_check).and_then(|m| m.modified()) else {
        return true;
    };
    SystemTime::now()
        .duration_since(modified)
        .map_or(true, |age| age >= CHECK_INTERVAL)
}

pub fn record_check(files: &Files) {
    let _ = fs::create_dir_all(&files.updates_dir);
    let _ = fs::write(&files.last_check, b"");
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Minimal local HTTP server serving canned responses (no external crate).
    pub struct FakeServer {
        pub base: String,
        routes: Arc<Mutex<BTreeMap<String, Route>>>,
    }

    #[derive(Clone)]
    enum Route {
        Ok(Vec<u8>),
        /// Announces `declared` bytes but sends only the given ones, then closes.
        Truncated {
            body: Vec<u8>,
            declared: usize,
        },
    }

    impl FakeServer {
        pub fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let base = format!("http://{}", listener.local_addr().expect("addr"));
            let routes: Arc<Mutex<BTreeMap<String, Route>>> = Arc::default();
            let served = Arc::clone(&routes);
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let routes = Arc::clone(&served);
                    std::thread::spawn(move || {
                        let mut reader = BufReader::new(&stream);
                        let mut request = String::new();
                        if reader.read_line(&mut request).is_err() {
                            return;
                        }
                        // Skip headers.
                        let mut line = String::new();
                        while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                            line.clear();
                        }
                        let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                        let route = routes.lock().expect("routes").get(&path).cloned();
                        let mut out = &stream;
                        let _ = match route {
                            Some(Route::Ok(body)) => out
                                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes())
                                .and_then(|()| out.write_all(&body)),
                            Some(Route::Truncated { body, declared }) => out
                                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n").as_bytes())
                                .and_then(|()| out.write_all(&body)),
                            None => out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
                        };
                    });
                }
            });
            Self { base, routes }
        }

        pub fn serve(&self, path: &str, body: impl Into<Vec<u8>>) {
            self.routes
                .lock()
                .expect("routes")
                .insert(path.into(), Route::Ok(body.into()));
        }

        fn serve_truncated(&self, path: &str, body: &[u8], declared: usize) {
            self.routes.lock().expect("routes").insert(
                path.into(),
                Route::Truncated {
                    body: body.to_vec(),
                    declared,
                },
            );
        }

        pub fn endpoint(&self, public_key: &str) -> Endpoint {
            Endpoint {
                api: self.base.clone(),
                repository: "owner/flair".into(),
                https_only: false,
                public_key: public_key.into(),
            }
        }

        /// Publishes a release (GitHub API + manifest + binary + signature).
        pub fn publish(
            &self,
            signed: &flair_core::update::tests_support::Signed,
            prerelease: bool,
        ) {
            let m = &signed.manifest;
            let asset = m.asset().expect("asset");
            let url = |name: &str| format!("{}/download/{}/{name}", self.base, m.version);
            let releases = serde_json::json!([{
                "tag_name": format!("v{}", m.version),
                "draft": false,
                "prerelease": prerelease,
                "assets": [
                    {"name": "manifest.json", "browser_download_url": url("manifest.json")},
                    {"name": asset.file, "browser_download_url": url(&asset.file)},
                    {"name": asset.signature, "browser_download_url": url(&asset.signature)},
                ]
            }]);
            self.serve(
                "/repos/owner/flair/releases?per_page=50",
                releases.to_string(),
            );
            self.serve(
                &format!("/download/{}/manifest.json", m.version),
                serde_json::to_string(m).expect("json"),
            );
            self.serve(
                &format!("/download/{}/{}", m.version, asset.file),
                signed.binary.clone(),
            );
            self.serve(
                &format!("/download/{}/{}", m.version, asset.signature),
                signed.signature.clone(),
            );
        }
    }

    use flair_core::update::tests_support::{signed_release, signed_release_with_comment};
    use flair_core::update::{Decision, decide};

    fn sandbox(name: &str) -> (PathBuf, PathBuf, Files) {
        let dir = std::env::temp_dir().join(format!("flair-upd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let exe = dir.join(flair_platform::install::exe_name());
        fs::write(&exe, b"old version").expect("exe");
        let files = Files::new(&dir.join("data"));
        (dir, exe, files)
    }

    #[test]
    fn up_to_date() {
        let server = FakeServer::start();
        let signed = signed_release("0.4.0-main.57", b"new");
        server.publish(&signed, true);
        let release =
            latest(&server.endpoint(&signed.public_key), UpdateChannel::Main).expect("found");
        assert_eq!(
            decide("0.4.0-main.57", &release.manifest),
            Ok(Decision::UpToDate)
        );
    }

    #[test]
    fn available_downloaded_verified_installed() {
        let server = FakeServer::start();
        let signed = signed_release("0.4.0-main.57", b"brand new binary");
        server.publish(&signed, true);
        let endpoint = server.endpoint(&signed.public_key);
        let release = latest(&endpoint, UpdateChannel::Main).expect("found");
        assert_eq!(
            decide("0.4.0-main.52", &release.manifest),
            Ok(Decision::Available)
        );
        let seen = Mutex::new(Vec::new());
        let binary = fetch_verified(&endpoint, &release, &|got, total| {
            seen.lock().expect("lock").push((got, total));
        })
        .expect("verified");
        assert_eq!(seen.lock().expect("lock").last(), Some(&(16, Some(16))));

        let (dir, exe, files) = sandbox("install");
        install(&binary, &exe, &files, "0.4.0-main.52", "0.4.0-main.57").expect("installed");
        assert_eq!(fs::read(&exe).expect("exe"), b"brand new binary");
        assert_eq!(
            fs::read(flair_platform::install::previous_exe(&exe)).expect("previous"),
            b"old version"
        );
        assert!(files.health.exists(), "health check armed");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stable_channel_ignores_prereleases() {
        let server = FakeServer::start();
        let signed = signed_release("0.4.0-main.57", b"x");
        server.publish(&signed, true);
        assert!(matches!(
            latest(&server.endpoint(&signed.public_key), UpdateChannel::Stable),
            Err(Error::NoRelease(_))
        ));
    }

    #[test]
    fn invalid_signature_is_refused() {
        let server = FakeServer::start();
        let signed = signed_release("0.5.0", b"genuine");
        server.publish(&signed, false);
        let other_key = signed_release("0.5.0", b"x").public_key;
        let endpoint = server.endpoint(&other_key);
        let release = latest(&endpoint, UpdateChannel::Stable).expect("found");
        assert!(matches!(
            fetch_verified(&endpoint, &release, &|_, _| {}),
            Err(Error::Update(UpdateError::BadSignature))
        ));
    }

    #[test]
    fn invalid_sha256_is_refused_and_nothing_replaced() {
        let server = FakeServer::start();
        let signed = signed_release("0.5.0", b"genuine");
        server.publish(&signed, false);
        // The binary is swapped on the server after the manifest was made.
        server.serve("/download/0.5.0/flair-test", b"tampered".to_vec());
        let endpoint = server.endpoint(&signed.public_key);
        let release = latest(&endpoint, UpdateChannel::Stable).expect("found");
        let (dir, exe, _) = sandbox("badsha");
        let result = fetch_verified(&endpoint, &release, &|_, _| {});
        assert!(matches!(
            result,
            Err(Error::Update(UpdateError::BadChecksum))
        ));
        assert_eq!(fs::read(&exe).expect("exe"), b"old version");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn signature_of_another_version_is_refused() {
        let server = FakeServer::start();
        let signed = signed_release_with_comment("0.5.0", b"bin", "flair 0.3.0 linux-x86_64");
        server.publish(&signed, false);
        let endpoint = server.endpoint(&signed.public_key);
        let release = latest(&endpoint, UpdateChannel::Stable).expect("found");
        assert!(matches!(
            fetch_verified(&endpoint, &release, &|_, _| {}),
            Err(Error::Update(UpdateError::WrongRelease(_)))
        ));
    }

    #[test]
    fn interrupted_download_replaces_nothing() {
        let server = FakeServer::start();
        let signed = signed_release("0.5.0", &[7_u8; 5000]);
        server.publish(&signed, false);
        server.serve_truncated("/download/0.5.0/flair-test", &[7_u8; 1000], 5000);
        let endpoint = server.endpoint(&signed.public_key);
        let release = latest(&endpoint, UpdateChannel::Stable).expect("found");
        let (dir, exe, files) = sandbox("interrupted");
        let result = fetch_verified(&endpoint, &release, &|_, _| {});
        assert!(result.is_err(), "{result:?}");
        assert_eq!(fs::read(&exe).expect("exe"), b"old version");
        assert!(!files.health.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn min_updater_version_requires_a_manual_download() {
        let server = FakeServer::start();
        let mut signed = signed_release("0.9.0", b"x");
        signed.manifest.min_updater_version = "0.5.0".into();
        server.publish(&signed, false);
        let release =
            latest(&server.endpoint(&signed.public_key), UpdateChannel::Stable).expect("found");
        assert_eq!(
            decide("0.4.0", &release.manifest),
            Ok(Decision::ManualDownload)
        );
    }

    #[test]
    fn requires_reset_is_read_from_the_manifest() {
        let server = FakeServer::start();
        let mut signed = signed_release("0.9.0", b"x");
        signed.manifest.requires_reset = true;
        server.publish(&signed, false);
        let release =
            latest(&server.endpoint(&signed.public_key), UpdateChannel::Stable).expect("found");
        assert!(release.manifest.requires_reset);
    }

    #[test]
    fn https_is_mandatory_outside_tests() {
        let server = FakeServer::start();
        let mut endpoint = server.endpoint("key");
        endpoint.https_only = true;
        assert!(matches!(
            latest(&endpoint, UpdateChannel::Main),
            Err(Error::Network(_))
        ));
    }

    #[test]
    #[cfg(unix)] // starts the restored executable (a shell script here)
    fn two_failed_starts_roll_back_automatically() {
        let (dir, exe, files) = sandbox("rollback");
        fs::write(&exe, b"#!/bin/sh\nexit 0\n").expect("bad new");
        install(b"#!/bin/sh\nexit 0\n", &exe, &files, "0.4.0", "0.5.0").expect("installed");
        fs::write(
            flair_platform::install::previous_exe(&exe),
            b"#!/bin/sh\nexit 0\n# good old\n",
        )
        .expect("old");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for f in [&exe, &flair_platform::install::previous_exe(&exe)] {
                fs::set_permissions(f, fs::Permissions::from_mode(0o755)).expect("chmod");
            }
        }
        // 1st and 2nd starts of the new version crash before confirming.
        assert_eq!(startup_guard(&exe, &files), Guard::Continue);
        assert_eq!(startup_guard(&exe, &files), Guard::Continue);
        // 3rd start: the previous version is restored and started.
        assert_eq!(startup_guard(&exe, &files), Guard::RolledBack);
        assert!(fs::read_to_string(&exe).expect("exe").contains("good old"));
        let notice = take_rollback_notice(&files).expect("notice");
        assert_eq!(
            (notice.from.as_str(), notice.to.as_str()),
            ("0.5.0", "0.4.0")
        );
        assert!(take_rollback_notice(&files).is_none(), "told once");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_confirmed_start_disarms_the_health_check() {
        let (dir, exe, files) = sandbox("confirm");
        install(b"new", &exe, &files, "0.4.0", "0.5.0").expect("installed");
        assert_eq!(startup_guard(&exe, &files), Guard::Continue);
        confirm_started(&files);
        for _ in 0..3 {
            assert_eq!(startup_guard(&exe, &files), Guard::Continue);
        }
        assert_eq!(fs::read(&exe).expect("exe"), b"new");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn manual_rollback_swaps_versions() {
        let (dir, exe, files) = sandbox("manual");
        install(b"new", &exe, &files, "0.4.0", "0.5.0").expect("installed");
        swap_with_previous(&exe).expect("swapped");
        assert_eq!(fs::read(&exe).expect("exe"), b"old version");
        assert_eq!(
            fs::read(flair_platform::install::previous_exe(&exe)).expect("prev"),
            b"new"
        );
        let _ = fs::remove_dir_all(dir);
    }
}
