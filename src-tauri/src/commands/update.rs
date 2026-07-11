#![cfg_attr(not(any(test, target_os = "android")), allow(dead_code))]

use std::cmp::Ordering;
#[cfg(target_os = "android")]
use std::time::Duration;

use reqwest::Url;
use semver::Version;
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const ANDROID_UPDATE_METADATA_URL: &str =
    "https://github.com/zhuxbo/ai-email/releases/latest/download/android-latest.json";
const ANDROID_RELEASE_PATH_PREFIX: &str = "/zhuxbo/ai-email/releases/download/";
const ANDROID_VERSION_CODE_MAJOR_FACTOR: u64 = 1_000_000;
const ANDROID_VERSION_CODE_MINOR_FACTOR: u64 = 1_000;
const ANDROID_VERSION_CODE_MAX: u64 = 2_100_000_000;
#[cfg(target_os = "android")]
const ANDROID_UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const MACOS_LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/zhuxbo/ai-email/releases/latest";
const MACOS_RELEASE_PATH_PREFIX: &str = "/zhuxbo/ai-email/releases/download/";
const MACOS_UPDATE_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MACOS_UPDATE_USER_AGENT: &str = concat!("ai-email/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidUpdateInfo {
    pub version: String,
    pub version_code: i64,
    pub notes: String,
    pub pub_date: String,
    pub apk_url: String,
    pub apk_size: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosUpdateInfo {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
    pub dmg_url: String,
}

#[derive(serde::Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    body: Option<String>,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(serde::Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[cfg(test)]
pub(crate) fn is_allowed_android_apk_url(raw: &str) -> bool {
    validate_allowed_android_apk_url(raw).is_ok()
}

pub(crate) fn compare_version_code(remote: i64, current: i64) -> Ordering {
    remote.cmp(&current)
}

pub(crate) fn parse_android_metadata(raw: &str) -> AppResult<AndroidUpdateInfo> {
    let info: AndroidUpdateInfo = serde_json::from_str(raw)?;
    if info.version_code <= 0 {
        return Err(AppError::Config("Android versionCode 必须大于 0".into()));
    }
    if info.apk_size <= 0 {
        return Err(AppError::Config("Android APK 大小必须大于 0".into()));
    }
    validate_allowed_android_apk_url(&info.apk_url)?;
    Ok(info)
}

pub(crate) fn current_android_version_code() -> AppResult<i64> {
    let version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| AppError::Config("当前 Android 版本非法".into()))?;
    android_version_code_from_semver(&version)
}

fn android_version_code_from_semver(version: &Version) -> AppResult<i64> {
    // Tauri defaults to major * 1_000_000 + minor * 1_000 + patch.
    let code = version
        .major
        .checked_mul(ANDROID_VERSION_CODE_MAJOR_FACTOR)
        .and_then(|code| {
            version
                .minor
                .checked_mul(ANDROID_VERSION_CODE_MINOR_FACTOR)
                .and_then(|minor| code.checked_add(minor))
        })
        .and_then(|code| code.checked_add(version.patch))
        .filter(|code| *code <= ANDROID_VERSION_CODE_MAX)
        .ok_or_else(|| AppError::Config("Android versionCode 超出允许范围".into()))?;

    Ok(code as i64)
}

fn validate_allowed_android_apk_url(raw: &str) -> AppResult<Url> {
    let url = Url::parse(raw).map_err(|_| AppError::Config("更新下载地址非法".into()))?;
    if url.scheme() != "https" {
        return Err(AppError::Config("更新下载地址不可信".into()));
    }
    if url.host_str() != Some("github.com") {
        return Err(AppError::Config("更新下载地址不可信".into()));
    }
    if !url.path().starts_with(ANDROID_RELEASE_PATH_PREFIX) || !url.path().ends_with(".apk") {
        return Err(AppError::Config("更新下载地址不可信".into()));
    }
    Ok(url)
}

pub(crate) fn parse_macos_release(
    raw: &str,
    current_version: &str,
) -> AppResult<Option<MacosUpdateInfo>> {
    let release: GithubLatestRelease = serde_json::from_str(raw)?;
    if release.draft || release.prerelease {
        return Err(AppError::Config("macOS 更新发布不可用".into()));
    }

    let version_text = release
        .tag_name
        .strip_prefix('v')
        .ok_or_else(|| AppError::Config("macOS 更新版本标签非法".into()))?;
    let remote_version = Version::parse(version_text)
        .map_err(|_| AppError::Config("macOS 更新版本标签非法".into()))?;
    if !remote_version.pre.is_empty() {
        return Err(AppError::Config("macOS 更新版本标签非法".into()));
    }

    let current_version = Version::parse(current_version)
        .map_err(|_| AppError::Config("当前 macOS 版本非法".into()))?;
    if remote_version <= current_version {
        return Ok(None);
    }

    let expected_name = format!("ai-email_{remote_version}_aarch64.dmg");
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == expected_name)
        .ok_or_else(|| AppError::Config("macOS 更新包不存在或名称非法".into()))?;
    let dmg_url = validate_allowed_macos_dmg_url(&asset.browser_download_url, &current_version)?;
    let expected_path = format!("{MACOS_RELEASE_PATH_PREFIX}v{remote_version}/{expected_name}");
    if dmg_url.path() != expected_path {
        return Err(AppError::Config(
            "macOS 更新下载地址与发布版本不一致".into(),
        ));
    }

    Ok(Some(MacosUpdateInfo {
        version: remote_version.to_string(),
        notes: release.body.unwrap_or_default(),
        pub_date: release
            .published_at
            .ok_or_else(|| AppError::Config("macOS 更新发布日期缺失".into()))?,
        dmg_url: asset.browser_download_url.clone(),
    }))
}

fn validate_macos_update_download_url(raw: &str, current_version: &str) -> AppResult<Url> {
    let current_version = Version::parse(current_version)
        .map_err(|_| AppError::Config("当前 macOS 版本非法".into()))?;
    validate_allowed_macos_dmg_url(raw, &current_version)
}

fn validate_allowed_macos_dmg_url(raw: &str, current_version: &Version) -> AppResult<Url> {
    let url = Url::parse(raw).map_err(|_| AppError::Config("更新下载地址非法".into()))?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Config("更新下载地址不可信".into()));
    }

    let Some((tag, asset_name)) = url
        .path()
        .strip_prefix(MACOS_RELEASE_PATH_PREFIX)
        .and_then(|path| path.split_once('/'))
    else {
        return Err(AppError::Config("更新下载地址不可信".into()));
    };
    let Some(version_text) = tag.strip_prefix('v') else {
        return Err(AppError::Config("更新下载地址不可信".into()));
    };
    let version =
        Version::parse(version_text).map_err(|_| AppError::Config("更新下载地址不可信".into()))?;
    let expected_name = format!("ai-email_{version}_aarch64.dmg");
    let expected_path = format!("{MACOS_RELEASE_PATH_PREFIX}v{version}/{expected_name}");
    if !version.pre.is_empty()
        || version <= *current_version
        || asset_name != expected_name
        || url.path() != expected_path
    {
        return Err(AppError::Config("更新下载地址不可信".into()));
    }
    Ok(url)
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn android_update_check() -> AppResult<Option<AndroidUpdateInfo>> {
    let client = reqwest::Client::builder()
        .timeout(ANDROID_UPDATE_CHECK_TIMEOUT)
        .build()?;
    let raw = client
        .get(ANDROID_UPDATE_METADATA_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let info = parse_android_metadata(&raw)?;
    let current = current_android_version_code()?;
    if compare_version_code(info.version_code, current).is_gt() {
        Ok(Some(info))
    } else {
        Ok(None)
    }
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn android_update_check() -> AppResult<Option<AndroidUpdateInfo>> {
    Ok(None)
}

#[tauri::command]
pub async fn android_update_open_download(app: tauri::AppHandle, url: String) -> AppResult<()> {
    validate_allowed_android_apk_url(&url)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::Other(anyhow::anyhow!("open update url failed: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn macos_update_check() -> AppResult<Option<MacosUpdateInfo>> {
    let client = reqwest::Client::builder()
        .timeout(MACOS_UPDATE_CHECK_TIMEOUT)
        .user_agent(MACOS_UPDATE_USER_AGENT)
        .build()?;
    let raw = client
        .get(MACOS_LATEST_RELEASE_API_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse_macos_release(&raw, env!("CARGO_PKG_VERSION"))
}

#[tauri::command]
pub async fn macos_update_open_download(app: tauri::AppHandle, url: String) -> AppResult<()> {
    validate_macos_update_download_url(&url, env!("CARGO_PKG_VERSION"))?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::Other(anyhow::anyhow!("open update url failed: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{
        android_version_code_from_semver, compare_version_code, current_android_version_code,
        is_allowed_android_apk_url, parse_android_metadata, parse_macos_release,
        validate_macos_update_download_url,
    };
    use semver::Version;

    fn macos_release_json(
        tag_name: &str,
        draft: bool,
        prerelease: bool,
        asset_name: &str,
        download_url: &str,
    ) -> String {
        serde_json::json!({
            "tag_name": tag_name,
            "body": "更新说明",
            "published_at": "2026-07-10T00:00:00Z",
            "draft": draft,
            "prerelease": prerelease,
            "assets": [{
                "name": asset_name,
                "browser_download_url": download_url,
            }],
        })
        .to_string()
    }

    #[test]
    fn allowed_android_apk_url_accepts_repo_release_apk() {
        assert!(is_allowed_android_apk_url(
            "https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_arm64-v8a.apk"
        ));
    }

    #[test]
    fn allowed_android_apk_url_rejects_http_and_other_hosts() {
        assert!(!is_allowed_android_apk_url(
            "http://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_arm64-v8a.apk"
        ));
        assert!(!is_allowed_android_apk_url(
            "https://evil.example/releases/download/v0.2.0/ai-email_0.2.0_arm64-v8a.apk"
        ));
    }

    #[test]
    fn version_code_comparison_requires_remote_to_be_newer() {
        assert_eq!(compare_version_code(2000, 1000), Ordering::Greater);
        assert_eq!(compare_version_code(1000, 1000), Ordering::Equal);
        assert_eq!(compare_version_code(999, 1000), Ordering::Less);
    }

    #[test]
    fn current_android_version_code_uses_package_version() {
        assert_eq!(current_android_version_code().unwrap(), 1000);
    }

    #[test]
    fn android_version_code_matches_tauri_semver_default() {
        assert_eq!(
            android_version_code_from_semver(&Version::parse("0.1.0").unwrap()).unwrap(),
            1000
        );
        assert_eq!(
            android_version_code_from_semver(&Version::parse("1.2.3").unwrap()).unwrap(),
            1_002_003
        );
        assert!(android_version_code_from_semver(&Version::parse("2101.0.0").unwrap()).is_err());
    }

    #[test]
    fn parse_android_metadata_rejects_invalid_apk_url() {
        let raw = r#"{
          "version":"0.2.0",
          "versionCode":2000,
          "notes":"更新说明",
          "pubDate":"2026-07-10T00:00:00Z",
          "apkUrl":"https://evil.example/app.apk",
          "apkSize":123,
          "sha256":"abc"
        }"#;

        assert!(parse_android_metadata(raw).is_err());
    }

    #[test]
    fn macos_release_accepts_newer_stable_arm64_dmg() {
        let raw = macos_release_json(
            "v0.2.0",
            false,
            false,
            "ai-email_0.2.0_aarch64.dmg",
            "https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg",
        );

        let info = parse_macos_release(&raw, "0.1.0").unwrap().unwrap();

        assert_eq!(info.version, "0.2.0");
        assert_eq!(info.notes, "更新说明");
        assert_eq!(info.pub_date, "2026-07-10T00:00:00Z");
        assert_eq!(
            info.dmg_url,
            "https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg"
        );
        assert_eq!(
            serde_json::to_value(info).unwrap(),
            serde_json::json!({
                "version": "0.2.0",
                "notes": "更新说明",
                "pubDate": "2026-07-10T00:00:00Z",
                "dmgUrl": "https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg",
            })
        );
    }

    #[test]
    fn macos_release_rejects_drafts_prereleases_and_non_newer_versions() {
        let expected_name = "ai-email_0.2.0_aarch64.dmg";
        let expected_url = "https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg";

        for raw in [
            macos_release_json("v0.2.0", true, false, expected_name, expected_url),
            macos_release_json("v0.2.0", false, true, expected_name, expected_url),
        ] {
            assert!(parse_macos_release(&raw, "0.1.0").is_err());
        }

        for tag_name in ["v0.1.0", "v0.0.9"] {
            let raw = macos_release_json(
                tag_name,
                false,
                false,
                "ai-email_0.1.0_aarch64.dmg",
                "https://github.com/zhuxbo/ai-email/releases/download/v0.1.0/ai-email_0.1.0_aarch64.dmg",
            );
            assert!(parse_macos_release(&raw, "0.1.0").unwrap().is_none());
        }
    }

    #[test]
    fn macos_release_rejects_invalid_tag_assets_and_download_urls() {
        let expected_name = "ai-email_0.2.0_aarch64.dmg";
        let expected_url = "https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg";

        for (tag_name, asset_name, download_url) in [
            ("release-0.2.0", expected_name, expected_url),
            ("v0.2.0", "ai-email_0.2.0_x86_64.dmg", expected_url),
            (
                "v0.2.0",
                expected_name,
                "http://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg",
            ),
            (
                "v0.2.0",
                expected_name,
                "https://github.com/zhuxbo/other/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg",
            ),
            (
                "v0.2.0",
                expected_name,
                "https://evil.example/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg",
            ),
            (
                "v0.2.0",
                expected_name,
                "https://github.com/zhuxbo/ai-email/releases/download/v0.3.0/ai-email_0.3.0_aarch64.dmg",
            ),
        ] {
            let raw = macos_release_json(tag_name, false, false, asset_name, download_url);
            assert!(parse_macos_release(&raw, "0.1.0").is_err());
        }
    }

    #[test]
    fn macos_open_download_validation_boundary_requires_a_newer_dmg() {
        let current_version = "0.1.0";
        let url_for = |version: &str| {
            format!(
                "https://github.com/zhuxbo/ai-email/releases/download/v{version}/ai-email_{version}_aarch64.dmg"
            )
        };

        assert!(validate_macos_update_download_url(&url_for("0.1.0"), current_version).is_err());
        assert!(validate_macos_update_download_url(&url_for("0.0.9"), current_version).is_err());
        assert!(validate_macos_update_download_url(&url_for("0.2.0"), current_version).is_ok());
    }
}
