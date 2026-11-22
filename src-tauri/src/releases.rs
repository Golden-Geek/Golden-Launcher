use std::{cmp::Ordering, collections::HashSet, time::Duration};

use reqwest::{Client, Response, StatusCode, Url, header, redirect};
use semver::{Prerelease, Version};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::catalog::{AppId, PRODUCTS};

const GITHUB: &str = "https://api.github.com/repos/Golden-Geek/Chataigne2/releases";
const GITHUB_DOWNLOADS: &str = "/Golden-Geek/Chataigne2/releases/download/";
const SNORING_PONY_GITHUB: &str = "https://api.github.com/repos/boherm/SnoringPony/releases";
const SNORING_PONY_DOWNLOADS: &str = "/boherm/SnoringPony/releases/download/";
const METADATA_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Stable,
    Beta,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Target {
    WinX64,
    WinArm64,
    MacIntel,
    MacSilicon,
    LinuxX64,
    LinuxArm64,
    LinuxArmhf,
    #[default]
    Unsupported,
}

impl Target {
    pub fn current() -> Self {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Self::WinX64,
            ("windows", "aarch64") => Self::WinArm64,
            ("macos", "x86_64") => Self::MacIntel,
            ("macos", "aarch64") => Self::MacSilicon,
            ("linux", "x86_64") => Self::LinuxX64,
            ("linux", "aarch64") => Self::LinuxArm64,
            ("linux", "arm") => Self::LinuxArmhf,
            _ => Self::Unsupported,
        }
    }

    fn legacy_suffix(self) -> &'static str {
        match self {
            Self::WinX64 => "win-x64",
            Self::WinArm64 => "win-arm64",
            Self::MacIntel => "osx-intel",
            Self::MacSilicon => "osx-silicon",
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-aarch64",
            Self::LinuxArmhf => "linux-armhf",
            Self::Unsupported => "",
        }
    }
}

pub fn version(raw: &str) -> Option<Version> {
    let raw = raw.trim().strip_prefix(['v', 'V']).unwrap_or(raw.trim());
    if let Ok(parsed) = Version::parse(raw) {
        return Some(parsed);
    }
    let (base, beta) = raw.rsplit_once(['b', 'B'])?;
    if beta.is_empty() || !beta.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut parsed = Version::parse(base).ok()?;
    if !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return None;
    }
    parsed.pre = Prerelease::new(&format!("beta.{}", beta.parse::<u64>().ok()?)).ok()?;
    Some(parsed)
}

pub fn newer(candidate: &str, installed: &str) -> bool {
    version(candidate)
        .zip(version(installed))
        .is_some_and(|(candidate, installed)| candidate.cmp_precedence(&installed).is_gt())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PackageKind {
    Inno,
    Nsis,
    Pkg,
    Dmg,
    AppImage,
}

impl PackageKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Inno | Self::Nsis => "exe",
            Self::Pkg => "pkg",
            Self::Dmg => "dmg",
            Self::AppImage => "AppImage",
        }
    }

    pub fn matches_prefix(self, bytes: &[u8], target: Target) -> bool {
        match self {
            Self::Inno | Self::Nsis => bytes.starts_with(b"MZ"),
            Self::Pkg => bytes.starts_with(b"xar!"),
            Self::Dmg => bytes.len() >= 16 && !bytes.trim_ascii_start().starts_with(b"<"),
            Self::AppImage => appimage_header(bytes, target),
        }
    }
}

pub fn appimage_header(bytes: &[u8], target: Target) -> bool {
    let (class, machine) = match target {
        Target::LinuxX64 => (2, 62),
        Target::LinuxArm64 => (2, 183),
        Target::LinuxArmhf => (1, 40),
        _ => return false,
    };
    let arm_float_abi = bytes
        .get(36..40)
        .map(|flags| u32::from_le_bytes(flags.try_into().unwrap()) & 0x600);
    bytes.starts_with(b"\x7fELF")
        && bytes.get(4..7) == Some(&[class, 1, 1])
        && bytes.get(8..11) == Some(b"AI\x02")
        && bytes.get(18..20) == Some(&[machine, 0])
        && !(target == Target::LinuxArmhf && arm_float_abi == Some(0x200))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    #[serde(alias = "preview")]
    pub beta: bool,
    pub notes: Vec<String>,
    pub(crate) package: Package,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Package {
    pub url: String,
    pub kind: PackageKind,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Resolved {
    pub published: bool,
    pub stable: Option<Release>,
    #[serde(alias = "preview")]
    pub beta: Option<Release>,
    #[serde(default)]
    candidates: Vec<Release>,
}

impl Resolved {
    pub fn for_channel(&self, channel: Channel) -> Option<&Release> {
        match channel {
            Channel::Stable => self.stable.as_ref(),
            // Older caches could mirror the stable release in this slot.
            Channel::Beta => self.beta.as_ref().filter(|release| release.beta),
        }
    }

    pub fn versions(&self) -> Vec<(Channel, &str)> {
        let mut builds: Vec<_> = self
            .candidates
            .iter()
            .chain(self.for_channel(Channel::Stable))
            .chain(self.for_channel(Channel::Beta))
            .collect();
        builds.sort_by(|left, right| order(left, right));
        let mut seen = HashSet::new();
        builds
            .into_iter()
            .map(|release| {
                (
                    if release.beta {
                        Channel::Beta
                    } else {
                        Channel::Stable
                    },
                    release.version.as_str(),
                )
            })
            .filter(|key| seen.insert(*key))
            .collect()
    }
}

#[derive(Clone, Copy)]
struct Publisher {
    directory: &'static str,
    feed: &'static str,
    prefix: &'static str,
}

fn publisher(id: AppId) -> Option<Publisher> {
    let (directory, feed, prefix) = match id {
        AppId::Chataigne => (
            "https://benjamin.kuperberg.fr/chataigne/user/data/",
            "https://benjamin.kuperberg.fr/chataigne/releases/update.json",
            "Chataigne",
        ),
        AppId::MapGyver => (
            "https://www.goldengeek.org/mapgyver/download/app/",
            "https://www.goldengeek.org/mapgyver/releases/update.json",
            "MapGyver",
        ),
        AppId::Blux => (
            "https://benjamin.kuperberg.fr/blux/download/app/",
            "https://benjamin.kuperberg.fr/blux/releases/update.json",
            "Blux",
        ),
        AppId::BlinderKitten => (
            "https://blinderkitten.lighting/installs/",
            "https://blinderkitten.lighting/update.json",
            "BlinderKitten",
        ),
        AppId::BenTo => (
            "https://benjamin.kuperberg.fr/bento/download/",
            "https://benjamin.kuperberg.fr/bento/update.json",
            "BenTo",
        ),
        AppId::Lgml => (
            "https://benjamin.kuperberg.fr/lgml/download/app/",
            "https://benjamin.kuperberg.fr/lgml/releases/update.json",
            "LGML",
        ),
        AppId::Chataigne2 | AppId::MapGyver2 | AppId::SnoringPony => return None,
    };
    Some(Publisher {
        directory,
        feed,
        prefix,
    })
}

fn allowed_url(url: &Url, hosts: &[&str]) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| hosts.contains(&host))
}

const PUBLISHER_HOSTS: &[&str] = &[
    "benjamin.kuperberg.fr",
    "www.goldengeek.org",
    "goldengeek.org",
    "www.blinderkitten.lighting",
    "blinderkitten.lighting",
];
const GITHUB_HOSTS: &[&str] = &[
    "api.github.com",
    "github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
];

pub fn client() -> reqwest::Result<Client> {
    Client::builder()
        .https_only(true)
        .user_agent(concat!("GoldenLauncher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(8))
        .read_timeout(Duration::from_secs(20))
        .redirect(redirect::Policy::custom(|attempt| {
            let Some(first) = attempt.previous().first() else {
                return attempt.stop();
            };
            let hosts = if allowed_url(first, GITHUB_HOSTS) {
                GITHUB_HOSTS
            } else {
                PUBLISHER_HOSTS
            };
            if attempt.previous().len() >= 5 || !allowed_url(attempt.url(), hosts) {
                return attempt.error("unapproved publisher redirect");
            }
            attempt.follow()
        }))
        .build()
}

pub fn check_package_source(id: AppId, package: &Package) -> Result<Url, String> {
    let url = Url::parse(&package.url).map_err(|_| "Invalid publisher package URL.")?;
    let approved = match publisher(id) {
        Some(source) => {
            allowed_url(&url, PUBLISHER_HOSTS) && url.as_str().starts_with(source.directory)
        }
        None => {
            let directory = match id {
                AppId::SnoringPony => SNORING_PONY_DOWNLOADS,
                _ => GITHUB_DOWNLOADS,
            };
            allowed_url(&url, &["github.com"]) && url.path().starts_with(directory)
        }
    };
    if !approved || url.query().is_some() || url.fragment().is_some() {
        return Err("The package is outside its approved publisher download directory.".into());
    }
    Ok(url)
}

#[derive(Deserialize)]
struct LegacyFeed {
    #[serde(rename = "winExtension", default)]
    win: String,
    #[serde(rename = "osxExtension", default)]
    mac: String,
    #[serde(rename = "linuxExtension", default)]
    linux: String,
    stableversion: Option<LegacyVersion>,
    betaversion: Option<LegacyVersion>,
    #[serde(default)]
    archives: Vec<LegacyVersion>,
}

#[derive(Deserialize)]
struct LegacyVersion {
    #[serde(default)]
    version: String,
    #[serde(default)]
    beta: serde_json::Value,
    #[serde(default)]
    changelog: Vec<String>,
    #[serde(default)]
    sha256: std::collections::HashMap<String, String>,
}

fn legacy_candidates(feed: LegacyFeed, source: Publisher, target: Target) -> (bool, Vec<Release>) {
    let kind = match target {
        Target::WinX64 | Target::WinArm64 if feed.win.eq_ignore_ascii_case("exe") => {
            Some(PackageKind::Inno)
        }
        Target::MacIntel | Target::MacSilicon if feed.mac.eq_ignore_ascii_case("pkg") => {
            Some(PackageKind::Pkg)
        }
        Target::MacIntel | Target::MacSilicon if feed.mac.eq_ignore_ascii_case("dmg") => {
            Some(PackageKind::Dmg)
        }
        Target::LinuxX64 | Target::LinuxArm64 | Target::LinuxArmhf
            if feed.linux.eq_ignore_ascii_case("appimage") =>
        {
            Some(PackageKind::AppImage)
        }
        _ => None,
    };
    let versions = feed
        .stableversion
        .into_iter()
        .map(|entry| (entry, false))
        .chain(feed.betaversion.into_iter().map(|entry| (entry, true)))
        .chain(feed.archives.into_iter().map(|entry| {
            let beta = entry.beta.as_bool() == Some(true) || entry.beta.as_str() == Some("true");
            (entry, beta)
        }));
    let windows_arm_targets = [Target::WinArm64, Target::WinX64];
    let package_targets = if target == Target::WinArm64 {
        &windows_arm_targets[..]
    } else {
        std::slice::from_ref(&target)
    };
    let mut published = false;
    let mut candidates = Vec::new();
    for (entry, beta) in versions {
        let Some(parsed) =
            version(&entry.version).filter(|parsed| *parsed != Version::new(0, 0, 0))
        else {
            continue;
        };
        published = true;
        let Some(kind) = kind else { continue };
        let notes: Vec<_> = entry
            .changelog
            .into_iter()
            .filter(|note| !note.trim().is_empty())
            .collect();
        for package_target in package_targets {
            let suffix = package_target.legacy_suffix();
            let filename = format!(
                "{}-{suffix}-{}.{}",
                source.prefix,
                entry.version,
                kind.extension()
            );
            let sha256 = entry
                .sha256
                .get(&filename)
                .or_else(|| entry.sha256.get(suffix))
                .cloned();
            candidates.push(Release {
                version: entry.version.clone(),
                beta: beta || !parsed.pre.is_empty(),
                notes: notes.clone(),
                package: Package {
                    url: format!("{}{filename}", source.directory),
                    kind,
                    sha256,
                    size: None,
                },
            });
        }
    }
    (published, candidates)
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
    state: String,
}

fn v2_stem(id: AppId, filename: &str) -> Option<String> {
    let name = filename.to_ascii_lowercase().replace(['-', ' '], "_");
    let product = match id {
        AppId::Chataigne2 => "chataigne",
        AppId::MapGyver2 => "mapgyver",
        _ => return None,
    };
    let suffix = name
        .strip_prefix(product)?
        .trim_start_matches('_')
        .strip_prefix("2_")?;
    Some(suffix.into())
}

fn github_kind(filename: &str, expected: &Version, target: Target) -> Option<PackageKind> {
    let (stem, extension) = filename.rsplit_once('.')?;
    // Tauri names assets Product_Version_Arch[-setup].ext.
    let expected_name = expected.to_string().to_ascii_lowercase().replace('-', "_");
    let arch = stem.strip_prefix(&expected_name)?.strip_prefix('_')?;
    match (extension, arch, target) {
        ("exe", "x64_setup" | "x86_64_setup", Target::WinX64) => Some(PackageKind::Nsis),
        ("exe", "arm64_setup" | "aarch64_setup", Target::WinArm64) => Some(PackageKind::Nsis),
        ("dmg", "x64" | "x86_64" | "intel" | "universal", Target::MacIntel) => {
            Some(PackageKind::Dmg)
        }
        ("dmg", "aarch64" | "arm64" | "silicon" | "universal", Target::MacSilicon) => {
            Some(PackageKind::Dmg)
        }
        ("appimage", "x64" | "x86_64" | "amd64", Target::LinuxX64) => Some(PackageKind::AppImage),
        ("appimage", "aarch64" | "arm64", Target::LinuxArm64) => Some(PackageKind::AppImage),
        ("appimage", "armhf" | "armv7" | "armv7l", Target::LinuxArmhf) => {
            Some(PackageKind::AppImage)
        }
        _ => None,
    }
}

fn snoring_pony_kind(filename: &str, tag: &str, target: Target) -> Option<PackageKind> {
    let kind = match target {
        Target::WinX64 => PackageKind::Inno,
        Target::MacIntel | Target::MacSilicon => PackageKind::Pkg,
        Target::LinuxX64 => PackageKind::AppImage,
        _ => return None,
    };
    (filename == format!("{}-{tag}.{}", target.legacy_suffix(), kind.extension())).then_some(kind)
}

fn github_candidates(feed: &[GithubRelease], id: AppId, target: Target) -> (bool, Vec<Release>) {
    let (published, mut candidates) = github_target_candidates(feed, id, target);
    if target == Target::WinArm64 {
        candidates.extend(github_target_candidates(feed, id, Target::WinX64).1);
    }
    (published, candidates)
}

fn github_target_candidates(
    feed: &[GithubRelease],
    id: AppId,
    target: Target,
) -> (bool, Vec<Release>) {
    let snoring_pony = id == AppId::SnoringPony;
    let mut published = false;
    let mut candidates = Vec::new();
    for build in feed.iter().filter(|build| !build.draft) {
        // Shared releases can use independent product versions in their tags.
        let product_tag = v2_stem(id, &build.tag_name).map(|tag| tag.replace('_', "-"));
        let tag = product_tag.as_deref().unwrap_or(&build.tag_name);
        let parsed = version(tag).filter(|parsed| *parsed != Version::new(0, 0, 0));
        for asset in build
            .assets
            .iter()
            .filter(|asset| asset.state == "uploaded" && asset.size > 0)
        {
            let filename = if snoring_pony {
                asset.name.strip_prefix("SnoringPony-").map(str::to_owned)
            } else {
                v2_stem(id, &asset.name)
            };
            let Some(filename) = filename else {
                continue;
            };
            published = true;
            let Some(parsed) = parsed.as_ref() else {
                continue;
            };
            let kind = if snoring_pony {
                snoring_pony_kind(&filename, tag, target)
            } else {
                github_kind(&filename, parsed, target)
            };
            let Some(kind) = kind else {
                continue;
            };
            candidates.push(Release {
                version: if snoring_pony {
                    tag.to_owned()
                } else {
                    parsed.to_string()
                },
                beta: build.prerelease || !parsed.pre.is_empty(),
                notes: build
                    .body
                    .as_ref()
                    .filter(|note| !note.trim().is_empty())
                    .cloned()
                    .into_iter()
                    .collect(),
                package: Package {
                    url: asset.browser_download_url.clone(),
                    kind,
                    sha256: asset
                        .digest
                        .as_ref()
                        .map(|digest| digest.strip_prefix("sha256:").unwrap_or(digest).to_owned()),
                    size: Some(asset.size),
                },
            });
        }
    }
    (published, candidates)
}

async fn json<T: DeserializeOwned>(client: &Client, url: &str) -> Result<T, String> {
    let mut response = client
        .get(url)
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .and_then(Response::error_for_status)
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len() + chunk.len() > METADATA_LIMIT {
            return Err("Publisher metadata exceeds the size limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("Invalid publisher metadata ({error})."))
}

async fn github(client: &Client, url: &str) -> Result<Vec<GithubRelease>, String> {
    let mut all = Vec::new();
    for page in 1..=20 {
        let batch: Vec<_> = json(client, &format!("{url}?per_page=100&page={page}")).await?;
        let last = batch.len() < 100;
        all.extend(batch);
        if last {
            return Ok(all);
        }
    }
    Err("GitHub release pagination exceeds the supported limit.".into())
}

async fn probe(
    client: &Client,
    id: AppId,
    package: &Package,
    target: Target,
) -> Result<bool, String> {
    let url = check_package_source(id, package)?;
    let mut response = client
        .get(url)
        .header(header::RANGE, "bytes=0-63")
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    match response.status() {
        StatusCode::NOT_FOUND | StatusCode::GONE => return Ok(false),
        StatusCode::OK | StatusCode::PARTIAL_CONTENT => {}
        status => return Err(format!("Publisher package check returned HTTP {status}.")),
    }
    if response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/") || value.starts_with("application/json"))
    {
        return Ok(false);
    }
    let mut prefix = Vec::with_capacity(64);
    while prefix.len() < 64 {
        let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? else {
            break;
        };
        prefix.extend_from_slice(&chunk[..chunk.len().min(64 - prefix.len())]);
    }
    Ok(package.kind.matches_prefix(&prefix, target))
}

fn order(left: &Release, right: &Release) -> Ordering {
    let left_version = version(&left.version).expect("parsed release");
    let right_version = version(&right.version).expect("parsed release");
    right_version
        .cmp_precedence(&left_version)
        .then(left.beta.cmp(&right.beta))
}

pub async fn compatible_version(
    client: &Client,
    id: AppId,
    target: Target,
    resolved: &Resolved,
    channel: Channel,
    version: &str,
) -> Result<Option<Release>, String> {
    if let Some(release) = resolved
        .for_channel(channel)
        .filter(|release| release.version == version)
    {
        return Ok(Some(release.clone()));
    }
    let mut error = None;
    for release in resolved
        .candidates
        .iter()
        .filter(|release| release.version == version && release.beta == (channel == Channel::Beta))
    {
        match probe(client, id, &release.package, target).await {
            Ok(true) => return Ok(Some(release.clone())),
            Ok(false) => (),
            Err(reason) => error = Some(reason),
        }
    }
    error.map_or(Ok(None), Err)
}

async fn resolve<Probe>(
    published: bool,
    mut candidates: Vec<Release>,
    check: impl Fn(Package) -> Probe,
) -> Result<Resolved, String>
where
    Probe: Future<Output = Result<bool, String>> + Send,
{
    candidates.sort_by(order);
    candidates
        .dedup_by(|right, left| right.beta == left.beta && right.package.url == left.package.url);
    let mut result = Resolved {
        published,
        ..Resolved::default()
    };
    // Stop probing a channel once its newest compatible package is found.
    'probes: for batch in candidates.chunks(8) {
        let probes = batch.iter().filter(|build| {
            if build.beta {
                result.beta.is_none()
            } else {
                result.stable.is_none()
            }
        });
        let outcomes = futures_util::future::join_all(probes.map(|build| {
            let probe = check(build.package.clone());
            async move { (build, probe.await) }
        }))
        .await;
        for (build, exists) in outcomes {
            let channel = if build.beta {
                &mut result.beta
            } else {
                &mut result.stable
            };
            if channel.is_none() && exists? {
                *channel = Some(build.clone());
            }
            if result.stable.is_some() && result.beta.is_some() {
                break 'probes;
            }
        }
    }
    result.candidates = candidates;
    Ok(result)
}

#[cfg(test)]
pub fn chataigne_smoke_release(version: &str, target: Target) -> Release {
    // The pinned Windows smoke releases ship x64 installers.
    let target = if target == Target::WinArm64 {
        Target::WinX64
    } else {
        target
    };
    let source = publisher(AppId::Chataigne).expect("Chataigne publisher");
    let kind = match target {
        Target::WinX64 => PackageKind::Inno,
        Target::MacIntel | Target::MacSilicon => PackageKind::Pkg,
        Target::LinuxX64 | Target::LinuxArm64 | Target::LinuxArmhf => PackageKind::AppImage,
        _ => panic!("unsupported smoke-test target"),
    };
    Release {
        version: version.into(),
        beta: version.contains('b'),
        notes: Vec::new(),
        package: Package {
            url: format!(
                "{}Chataigne-{}-{version}.{}",
                source.directory,
                target.legacy_suffix(),
                kind.extension()
            ),
            kind,
            sha256: None,
            size: None,
        },
    }
}

async fn refresh_github(
    client: &Client,
    target: Target,
    url: &str,
    apps: &[AppId],
) -> Vec<(AppId, Result<Resolved, String>)> {
    let feed = github(client, url).await;
    futures_util::future::join_all(apps.iter().map(|&id| {
        let feed = &feed;
        async move {
            let result = match feed {
                Ok(feed) => {
                    let (published, candidates) = github_candidates(feed, id, target);
                    resolve(published, candidates, |package| async move {
                        probe(client, id, &package, target).await
                    })
                    .await
                }
                Err(error) => Err(error.clone()),
            };
            (id, result)
        }
    }))
    .await
}

pub async fn refresh_all(
    client: &Client,
    target: Target,
) -> Vec<(AppId, Result<Resolved, String>)> {
    let legacy = async {
        futures_util::future::join_all(PRODUCTS.iter().filter_map(|product| {
            let source = publisher(product.id)?;
            Some(async move {
                let id = product.id;
                let result = match json(client, source.feed).await {
                    Ok(feed) => {
                        let (published, candidates) = legacy_candidates(feed, source, target);
                        resolve(published, candidates, |package| async move {
                            probe(client, id, &package, target).await
                        })
                        .await
                    }
                    Err(error) => Err(error),
                };
                (id, result)
            })
        }))
        .await
    };
    let (mut legacy, modern, snoring_pony) = futures_util::future::join3(
        legacy,
        refresh_github(
            client,
            target,
            GITHUB,
            &[AppId::Chataigne2, AppId::MapGyver2],
        ),
        refresh_github(client, target, SNORING_PONY_GITHUB, &[AppId::SnoringPony]),
    )
    .await;
    legacy.extend(modern);
    legacy.extend(snoring_pony);
    legacy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_semantically_and_ignore_build_metadata() {
        assert!(newer("1.10.5b14", "1.10.5b9"));
        assert!(newer("1.10.5", "1.10.5b14"));
        assert!(newer("1.10.5-beta.15", "1.10.5b14"));
        assert!(!newer("1.1.0b5", "1.1.0"));
        assert!(!newer("1.0.0+2", "1.0.0+1"));
        assert!(!newer("1.0.0", "unknown"));
    }

    #[test]
    fn legacy_feed_excludes_placeholder_and_recognizes_beta_versions() {
        let feed = serde_json::from_str(
            r#"{
          "winExtension":"exe","osxExtension":"pkg","linuxExtension":"AppImage",
          "stableversion":{"version":"1.0.1b98","changelog":["A fix"]},
          "betaversion":{"version":"0.0.0"},
          "archives":[{},{"version":"1.0.0"},{"version":"0.9.0","beta":"true"}]
        }"#,
        )
        .unwrap();
        let (published, builds) = legacy_candidates(
            feed,
            publisher(AppId::BlinderKitten).unwrap(),
            Target::LinuxArmhf,
        );
        assert!(published);
        assert_eq!(builds.len(), 3);
        assert!(builds[0].beta && builds[2].beta);
        assert!(!builds[1].beta);
        assert!(
            builds[0]
                .package
                .url
                .ends_with("BlinderKitten-linux-armhf-1.0.1b98.AppImage")
        );
        let channels = Resolved {
            published,
            stable: Some(builds[1].clone()),
            beta: Some(builds[0].clone()),
            ..Resolved::default()
        };
        assert_eq!(
            channels.for_channel(Channel::Stable).unwrap().version,
            "1.0.0"
        );
        assert_eq!(
            channels.for_channel(Channel::Beta).unwrap().version,
            "1.0.1b98"
        );
    }

    #[test]
    fn resolves_an_older_beta_even_when_a_newer_stable_is_available() {
        let builds = ["1.10.5b14", "1.10.6", "1.10.5b15"]
            .map(|version| chataigne_smoke_release(version, Target::WinX64));
        let resolved =
            tauri::async_runtime::block_on(resolve(true, builds.to_vec(), async |package| {
                Ok(!package.url.contains("1.10.5b15"))
            }))
            .unwrap();
        assert_eq!(
            resolved.for_channel(Channel::Stable).unwrap().version,
            "1.10.6"
        );
        assert_eq!(
            resolved.for_channel(Channel::Beta).unwrap().version,
            "1.10.5b14"
        );
        assert_eq!(
            resolved.versions(),
            vec![
                (Channel::Stable, "1.10.6"),
                (Channel::Beta, "1.10.5b15"),
                (Channel::Beta, "1.10.5b14")
            ]
        );
    }

    #[test]
    fn old_stable_only_cache_does_not_duplicate_stable_as_beta() {
        let mut release =
            serde_json::to_value(chataigne_smoke_release("1.10.6", Target::WinX64)).unwrap();
        let fields = release.as_object_mut().unwrap();
        let beta = fields.remove("beta").unwrap();
        fields.insert("preview".into(), beta);
        let cached: Resolved = serde_json::from_value(serde_json::json!({
            "published": true,
            "stable": release,
            "preview": release,
        }))
        .unwrap();
        assert_eq!(
            cached.for_channel(Channel::Stable).unwrap().version,
            "1.10.6"
        );
        assert!(cached.for_channel(Channel::Beta).is_none());
    }

    #[test]
    fn github_assets_match_product_version_and_architecture_independently() {
        let assets = [
            "Chataigne2_2.0.0-beta.3_x64-setup.exe",
            "Chataigne2_2.0.0-beta.3_arm64-setup.exe",
            "MapGyver2_2.0.0-beta.3_x64-setup.exe",
            "MapGyver_2.0.0-beta.3_x64-setup.exe",
            "MapGyver2_2.0.0-beta.3_aarch64.AppImage",
            "MapGyver2_1.9.0_armhf.AppImage",
        ]
        .map(|name| GithubAsset {
            name: name.into(),
            browser_download_url: name.into(),
            state: "uploaded".into(),
            digest: Some(format!("sha256:{}", "0".repeat(64))),
            size: 500,
        });
        let feed = [GithubRelease {
            tag_name: "v2.0.0-beta.3".into(),
            body: None,
            draft: false,
            prerelease: false,
            assets: assets.into(),
        }];
        assert_eq!(
            github_candidates(&feed, AppId::Chataigne2, Target::WinX64)
                .1
                .len(),
            1
        );
        let (_, mut arm_builds) = github_candidates(&feed, AppId::Chataigne2, Target::WinArm64);
        arm_builds.sort_by(order);
        assert_eq!(arm_builds.len(), 2);
        assert!(arm_builds[0].package.url.ends_with("_arm64-setup.exe"));
        assert!(arm_builds[1].package.url.ends_with("_x64-setup.exe"));
        let (_, emulated) = github_candidates(&feed, AppId::MapGyver2, Target::WinArm64);
        assert_eq!(emulated.len(), 1);
        assert!(emulated[0].package.url.ends_with("_x64-setup.exe"));
        assert!(
            github_candidates(&feed, AppId::MapGyver2, Target::LinuxArmhf)
                .1
                .is_empty()
        );
        let (published, releases) = github_candidates(&feed, AppId::MapGyver2, Target::LinuxArm64);
        assert!(published && releases[0].beta);
        assert_eq!(releases[0].package.sha256.as_ref().unwrap().len(), 64);
        assert!(!github_candidates(&[], AppId::Chataigne2, Target::WinX64).0);
    }

    #[test]
    fn snoring_pony_matches_published_packages_and_recognizes_beta_tags() {
        let id = AppId::SnoringPony;
        let tag = "1.0.0b19";
        let packages = [
            ("win-x64", Target::WinX64, PackageKind::Inno),
            ("osx-intel", Target::MacIntel, PackageKind::Pkg),
            ("osx-silicon", Target::MacSilicon, PackageKind::Pkg),
            ("linux-x64", Target::LinuxX64, PackageKind::AppImage),
        ];
        let assets = packages.map(|(platform, _, kind)| {
            let name = format!("SnoringPony-{platform}-{tag}.{}", kind.extension());
            GithubAsset {
                browser_download_url: format!(
                    "https://github.com{SNORING_PONY_DOWNLOADS}{tag}/{name}"
                ),
                name,
                state: "uploaded".into(),
                digest: None,
                size: 500,
            }
        });
        let feed = [GithubRelease {
            tag_name: tag.into(),
            body: None,
            draft: false,
            prerelease: false,
            assets: assets.into(),
        }];
        for (platform, target, kind) in packages {
            let (published, builds) = github_candidates(&feed, id, target);
            assert!(published);
            let [build] = builds.as_slice() else {
                panic!("expected one build for {target:?}");
            };
            assert!(build.beta);
            assert_eq!(build.version, tag);
            assert_eq!(build.package.kind, kind);
            let url = check_package_source(id, &build.package).unwrap();
            assert!(url.path().ends_with(&format!(
                "SnoringPony-{platform}-{tag}.{}",
                kind.extension()
            )));
            assert!(check_package_source(AppId::Chataigne2, &build.package).is_err());
        }
        let (published, builds) = github_candidates(&feed, id, Target::WinArm64);
        assert!(published);
        assert_eq!(builds.len(), 1);
        assert!(
            builds[0]
                .package
                .url
                .ends_with(&format!("SnoringPony-win-x64-{tag}.exe"))
        );
        let (published, builds) = github_candidates(&feed, id, Target::LinuxArm64);
        assert!(published && builds.is_empty());
    }

    #[test]
    fn appimage_probe_checks_elf_architecture_and_rejects_html() {
        let mut header = [0; 64];
        header[..7].copy_from_slice(b"\x7fELF\x01\x01\x01");
        header[8..11].copy_from_slice(b"AI\x02");
        header[18] = 40;
        assert!(PackageKind::AppImage.matches_prefix(&header, Target::LinuxArmhf));
        assert!(!PackageKind::AppImage.matches_prefix(&header, Target::LinuxArm64));
        header[36..40].copy_from_slice(&0x0500_0200u32.to_le_bytes());
        assert!(!PackageKind::AppImage.matches_prefix(&header, Target::LinuxArmhf));
        assert!(!PackageKind::Inno.matches_prefix(b"<html>", Target::WinX64));
    }

    #[test]
    #[ignore = "contacts live publisher services"]
    fn publisher_feeds_resolve() {
        let client = client().unwrap();
        let results = tauri::async_runtime::block_on(refresh_all(&client, Target::current()));
        assert_eq!(results.len(), PRODUCTS.len());
        for (id, result) in results {
            let resolved = result.unwrap_or_else(|error| panic!("{id:?}: {error}"));
            if id == AppId::Chataigne && Target::current() == Target::WinX64 {
                let archive = tauri::async_runtime::block_on(async {
                    for (channel, version) in resolved.versions().into_iter().take(12) {
                        if resolved
                            .for_channel(channel)
                            .is_none_or(|latest| latest.version != version)
                            && let Ok(Some(release)) = compatible_version(
                                &client,
                                id,
                                Target::current(),
                                &resolved,
                                channel,
                                version,
                            )
                            .await
                        {
                            return Some(release.version);
                        }
                    }
                    None
                });
                println!("Chataigne archived Windows build: {archive:?}");
            }
            println!(
                "{id:?}  stable={:?}  beta={:?}",
                resolved.stable.map(|build| build.version),
                resolved.beta.map(|build| build.version),
            );
        }
    }
}
