//! Update logic without I/O: the release manifest, the decision to update,
//! and the verification of a downloaded binary (minisign signature + SHA-256).
//! Downloading and replacing the executable live in the app.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Repository publishing the releases. Fixed at compile time, never read from
/// the configuration: a modified config cannot redirect updates.
pub const REPOSITORY: &str = "TomuCard/Flair";

/// Public key of the release signatures (minisign, key id CC6274C21E2B9C7A).
/// The private key only exists as the `FLAIR_SIGNING_KEY` GitHub secret.
pub const PUBLIC_KEY: &str = "RWR6nCsewnRizOVuyjFmqcV6a9xSKxILcviPum5G36Dazz8LgnZlptY4";

/// Manifest schema written by this version of the release workflow.
pub const MANIFEST_SCHEMA: u32 = 1;

/// One downloadable binary of a release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Asset {
    pub file: String,
    pub sha256: String,
    pub signature: String,
}

/// `manifest.json` published with every release. Read tolerantly: unknown
/// fields are ignored and missing ones take default values, so an old Flair
/// can still read the manifest of a much newer release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub channel: String,
    pub commit: String,
    pub date: String,
    pub notes: Vec<String>,
    pub min_updater_version: String,
    pub config_version: u32,
    pub requires_reset: bool,
    pub assets: BTreeMap<String, Asset>,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, UpdateError> {
        serde_json::from_str(json).map_err(|e| UpdateError::Manifest(e.to_string()))
    }

    /// The asset for the running platform.
    pub fn asset(&self) -> Option<&Asset> {
        self.assets.get(platform_key())
    }
}

/// Key of the running platform in `Manifest::assets`.
pub fn platform_key() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("invalid release manifest: {0}")]
    Manifest(String),
    #[error("invalid version number: {0}")]
    Version(String),
    #[error("no download for this platform in the release")]
    NoAsset,
    #[error("the signature is invalid")]
    BadSignature,
    #[error("the signature does not match this release ({0})")]
    WrongRelease(String),
    #[error("the file is corrupted (SHA-256 mismatch)")]
    BadChecksum,
}

/// What to do with a published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Already on this version or a newer one (never downgrade implicitly).
    UpToDate,
    /// A newer version can be installed by `> update`.
    Available,
    /// The running Flair is older than `min_updater_version`: it must be
    /// downloaded manually from the releases page.
    ManualDownload,
}

fn version(text: &str) -> Result<Version, UpdateError> {
    Version::parse(text.trim().trim_start_matches('v'))
        .map_err(|_| UpdateError::Version(text.to_owned()))
}

/// Compares the running version with a release.
pub fn decide(current: &str, manifest: &Manifest) -> Result<Decision, UpdateError> {
    let current = version(current)?;
    let latest = version(&manifest.version)?;
    if latest <= current {
        return Ok(Decision::UpToDate);
    }
    if manifest.asset().is_none() {
        return Err(UpdateError::NoAsset);
    }
    if !manifest.min_updater_version.trim().is_empty() {
        let min = version(&manifest.min_updater_version)?;
        if current < min {
            return Ok(Decision::ManualDownload);
        }
    }
    Ok(Decision::Available)
}

/// Trusted comment signed with each binary. It binds the signature to one
/// version and platform: an old signed binary cannot be passed off as a new one.
pub fn trusted_comment(version: &str, asset_key: &str) -> String {
    format!("flair {version} {asset_key}")
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Verifies a downloaded binary before anything is touched: SHA-256 from the
/// manifest, minisign signature with `public_key`, and the signed trusted
/// comment naming this exact version and platform.
pub fn verify(
    data: &[u8],
    signature: &str,
    manifest: &Manifest,
    public_key: &str,
) -> Result<(), UpdateError> {
    let asset = manifest.asset().ok_or(UpdateError::NoAsset)?;
    if !sha256_hex(data).eq_ignore_ascii_case(asset.sha256.trim()) {
        return Err(UpdateError::BadChecksum);
    }
    let key = minisign_verify::PublicKey::from_base64(public_key)
        .map_err(|_| UpdateError::BadSignature)?;
    let sig =
        minisign_verify::Signature::decode(signature).map_err(|_| UpdateError::BadSignature)?;
    key.verify(data, &sig, false)
        .map_err(|_| UpdateError::BadSignature)?;
    let expected = trusted_comment(&manifest.version, platform_key());
    let found = sig.trusted_comment().trim();
    if found != expected {
        return Err(UpdateError::WrongRelease(found.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A signed fake release, made with a throwaway key.
    pub struct Signed {
        pub public_key: String,
        pub manifest: Manifest,
        pub binary: Vec<u8>,
        pub signature: String,
    }

    pub fn signed_release(version: &str, binary: &[u8]) -> Signed {
        signed_release_with_comment(version, binary, &trusted_comment(version, platform_key()))
    }

    pub fn signed_release_with_comment(version: &str, binary: &[u8], comment: &str) -> Signed {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
        let signature = minisign::sign(
            Some(&pair.pk),
            &pair.sk,
            std::io::Cursor::new(binary),
            Some(comment),
            Some("test signature"),
        )
        .expect("signed")
        .into_string();
        let mut assets = BTreeMap::new();
        assets.insert(
            platform_key().to_owned(),
            Asset {
                file: "flair-test".into(),
                sha256: sha256_hex(binary),
                signature: "flair-test.minisig".into(),
            },
        );
        Signed {
            public_key: pair.pk.to_base64(),
            manifest: Manifest {
                schema: 1,
                version: version.into(),
                channel: "main".into(),
                min_updater_version: "0.1.0".into(),
                config_version: 2,
                assets,
                ..Manifest::default()
            },
            binary: binary.to_vec(),
            signature,
        }
    }

    #[test]
    fn embedded_public_key_is_valid() {
        assert!(minisign_verify::PublicKey::from_base64(PUBLIC_KEY).is_ok());
    }

    #[test]
    fn manifest_is_read_tolerantly() {
        let json = r#"{
            "schema": 7, "version": "0.4.0-main.57", "future_field": {"x": 1},
            "assets": {"windows-x86_64": {"file": "flair.exe", "sha256": "ab", "new": 1}}
        }"#;
        let m = Manifest::parse(json).expect("tolerant");
        assert_eq!(m.version, "0.4.0-main.57");
        assert!(!m.requires_reset);
        assert_eq!(m.assets["windows-x86_64"].file, "flair.exe");
        assert!(Manifest::parse("not json").is_err());
    }

    #[test]
    fn spec_example_manifest_parses() {
        let json = r#"{
          "schema": 1, "version": "0.4.0-main.57", "channel": "main", "commit": "a1b2c3d",
          "date": "2026-12-03T14:12:00Z", "notes": ["Calcul : fuseaux horaires"],
          "min_updater_version": "0.2.0", "config_version": 3, "requires_reset": false,
          "assets": {
            "windows-x86_64": { "file": "flair-windows-x86_64.exe", "sha256": "x", "signature": "flair-windows-x86_64.exe.minisig" },
            "linux-x86_64":   { "file": "flair-linux-x86_64", "sha256": "y", "signature": "flair-linux-x86_64.minisig" }
          }
        }"#;
        let m = Manifest::parse(json).expect("valid");
        assert_eq!(m.config_version, 3);
        assert!(m.asset().is_some());
    }

    fn manifest(version: &str, min: &str) -> Manifest {
        signed_release(version, b"x").manifest.clone_with_min(min)
    }

    impl Manifest {
        fn clone_with_min(mut self, min: &str) -> Self {
            self.min_updater_version = min.into();
            self
        }
    }

    #[test]
    fn decisions() {
        let m = manifest("0.4.0-main.57", "0.2.0");
        assert_eq!(decide("0.4.0-main.52", &m), Ok(Decision::Available));
        assert_eq!(decide("0.4.0-main.57", &m), Ok(Decision::UpToDate));
        assert_eq!(
            decide("0.4.0-main.60", &m),
            Ok(Decision::UpToDate),
            "never downgrade"
        );
        // A stable release is newer than its pre-releases.
        assert_eq!(
            decide("0.4.0-main.99", &manifest("0.4.0", "")),
            Ok(Decision::Available)
        );
        assert_eq!(decide("0.1.0", &m), Ok(Decision::ManualDownload));
        assert!(decide("garbage", &m).is_err());
        let mut no_asset = m.clone();
        no_asset.assets.clear();
        assert_eq!(decide("0.4.0-main.1", &no_asset), Err(UpdateError::NoAsset));
    }

    #[test]
    fn verification_accepts_a_genuine_release() {
        let r = signed_release("0.5.0", b"genuine binary");
        assert_eq!(
            verify(&r.binary, &r.signature, &r.manifest, &r.public_key),
            Ok(())
        );
    }

    #[test]
    fn verification_rejects_tampering() {
        let r = signed_release("0.5.0", b"genuine binary");
        // Modified binary: checksum fails first.
        assert_eq!(
            verify(b"evil binary!!!", &r.signature, &r.manifest, &r.public_key),
            Err(UpdateError::BadChecksum)
        );
        // Modified binary with a matching (attacker-written) manifest checksum.
        let mut m = r.manifest.clone();
        m.assets.get_mut(platform_key()).expect("asset").sha256 = sha256_hex(b"evil");
        assert_eq!(
            verify(b"evil", &r.signature, &m, &r.public_key),
            Err(UpdateError::BadSignature)
        );
        // Signed by another key.
        let other = signed_release("0.5.0", b"genuine binary");
        assert_eq!(
            verify(&r.binary, &r.signature, &r.manifest, &other.public_key),
            Err(UpdateError::BadSignature)
        );
        // Garbage signature.
        assert_eq!(
            verify(&r.binary, "nope", &r.manifest, &r.public_key),
            Err(UpdateError::BadSignature)
        );
    }

    #[test]
    fn an_old_signed_binary_cannot_pose_as_a_new_version() {
        // Genuinely signed, but for 0.3.0: replayed in a manifest claiming 0.9.0.
        let old = signed_release("0.3.0", b"old binary");
        let mut m = old.manifest.clone();
        m.version = "0.9.0".into();
        assert!(matches!(
            verify(&old.binary, &old.signature, &m, &old.public_key),
            Err(UpdateError::WrongRelease(_))
        ));
    }
}
