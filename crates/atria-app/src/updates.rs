//! Signed self-update manifest verification and safe Windows binary replacement.

use ring::{digest, signature};
use serde::Deserialize;
use std::time::Duration;

const REPOSITORY: &str = "Manixpcshot/atria-ai";
const UPDATE_PUBLIC_KEY_HEX: &str = "5d5295efd759e0d49cb4fb1ed673c04e6dff7db1b0cd3e078c03d36284b7a2da";
const MAX_UPDATE_BYTES: usize = 160 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct ManifestAsset {
    name: String,
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct SignedManifest {
    version: String,
    tag: String,
    release_url: String,
    assets: Vec<ManifestAsset>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub release_url: String,
    pub download_size: u64,
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    if value.len() % 2 != 0 {
        return Err("invalid hexadecimal signature".into());
    }
    value.as_bytes().chunks_exact(2).map(|pair| {
        let high = (pair[0] as char).to_digit(16).ok_or_else(|| "invalid hexadecimal signature".to_string())?;
        let low = (pair[1] as char).to_digit(16).ok_or_else(|| "invalid hexadecimal signature".to_string())?;
        Ok(((high << 4) | low) as u8)
    }).collect()
}

fn version_tuple(value: &str) -> Result<(u64, u64, u64), String> {
    let clean = value.trim().trim_start_matches('v');
    let mut parts = clean.split('.');
    let major = parts.next().ok_or_else(|| "invalid version".to_string())?.parse().map_err(|_| "invalid version".to_string())?;
    let minor = parts.next().ok_or_else(|| "invalid version".to_string())?.parse().map_err(|_| "invalid version".to_string())?;
    let patch = parts.next().ok_or_else(|| "invalid version".to_string())?.parse().map_err(|_| "invalid version".to_string())?;
    if parts.next().is_some() { return Err("unsupported version format".into()); }
    Ok((major, minor, patch))
}

fn verify_manifest(bytes: &[u8], signature_hex: &str, release_tag: &str) -> Result<SignedManifest, String> {
    let key = decode_hex(UPDATE_PUBLIC_KEY_HEX)?;
    let sig = decode_hex(signature_hex)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(bytes, &sig)
        .map_err(|_| "update manifest signature is invalid".to_string())?;
    let manifest: SignedManifest = serde_json::from_slice(bytes).map_err(|e| format!("invalid update manifest: {e}"))?;
    if manifest.tag != release_tag || manifest.version != release_tag.trim_start_matches('v') {
        return Err("signed update manifest does not match the GitHub release tag".into());
    }
    let expected_release = format!("https://github.com/{REPOSITORY}/releases/tag/{release_tag}");
    if manifest.release_url != expected_release {
        return Err("unexpected release URL in signed update manifest".into());
    }
    for asset in &manifest.assets {
        let expected = format!("https://github.com/{REPOSITORY}/releases/download/{release_tag}/{}", asset.name);
        if asset.url != expected || asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid asset entry in signed update manifest".into());
        }
    }
    if !manifest.assets.iter().any(|a| a.name == "atria.exe") {
        return Err("signed manifest does not contain the Windows executable".into());
    }
    Ok(manifest)
}

async fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("Atria/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(35))
        .build()
        .map_err(|e| format!("cannot initialize update client: {e}"))
}

async fn response_bytes(response: reqwest::Response) -> Result<Vec<u8>, String> {
    let status = response.status();
    if !status.is_success() {
        return Err(format!("update server returned HTTP {status}"));
    }
    let bytes = response.bytes().await.map_err(|e| format!("update download failed: {e}"))?;
    if bytes.len() > MAX_UPDATE_BYTES {
        return Err("update file exceeds the safety size limit".into());
    }
    Ok(bytes.to_vec())
}

async fn latest_verified() -> Result<(SignedManifest, reqwest::Client), String> {
    let http = client().await?;
    let api = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let release_resp = http.get(api).header("Accept", "application/vnd.github+json").send().await
        .map_err(|e| format!("cannot check for updates: {e}"))?;
    let release_bytes = response_bytes(release_resp).await?;
    let release: serde_json::Value = serde_json::from_slice(&release_bytes).map_err(|e| format!("invalid release response: {e}"))?;
    let tag = release.get("tag_name").and_then(|v| v.as_str()).ok_or_else(|| "release tag is missing".to_string())?.to_string();
    let release_assets = release.get("assets").and_then(|v| v.as_array()).ok_or_else(|| "release assets are missing".to_string())?;
    let asset_url = |name: &str| -> Result<String, String> {
        release_assets.iter().find(|a| a.get("name").and_then(|v| v.as_str()) == Some(name))
            .and_then(|a| a.get("browser_download_url").and_then(|v| v.as_str()))
            .map(str::to_string).ok_or_else(|| format!("release asset {name} is missing"))
    };
    let manifest_url = asset_url("update-manifest.json")?;
    let signature_url = asset_url("update-manifest.sig")?;
    let raw = response_bytes(http.get(manifest_url).send().await.map_err(|e| format!("cannot download update manifest: {e}"))?).await?;
    let signature = response_bytes(http.get(signature_url).send().await.map_err(|e| format!("cannot download update signature: {e}"))?).await?;
    let signature = String::from_utf8(signature).map_err(|_| "update signature is not valid UTF-8".to_string())?;
    let manifest = verify_manifest(&raw, &signature, &tag)?;
    Ok((manifest, http))
}

pub async fn check() -> Result<UpdateStatus, String> {
    let (manifest, _) = latest_verified().await?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let available = version_tuple(&manifest.version)? > version_tuple(&current)?;
    let exe = manifest.assets.iter().find(|a| a.name == "atria.exe").ok_or_else(|| "executable asset is missing".to_string())?;
    Ok(UpdateStatus {
        current_version: current,
        latest_version: manifest.version,
        available,
        release_url: manifest.release_url,
        download_size: exe.size,
    })
}

pub async fn download_verified_executable() -> Result<(String, std::path::PathBuf), String> {
    let (manifest, http) = latest_verified().await?;
    let current = env!("CARGO_PKG_VERSION");
    if version_tuple(&manifest.version)? <= version_tuple(current)? {
        return Err("Atria is already up to date".into());
    }
    let asset = manifest.assets.iter().find(|a| a.name == "atria.exe").ok_or_else(|| "executable asset is missing".to_string())?;
    if asset.size > MAX_UPDATE_BYTES as u64 {
        return Err("update executable exceeds the safety size limit".into());
    }
    let bytes = response_bytes(http.get(&asset.url).send().await.map_err(|e| format!("cannot download update: {e}"))?).await?;
    if bytes.len() as u64 != asset.size {
        return Err("downloaded executable size does not match the signed manifest".into());
    }
    let digest = digest::digest(&digest::SHA256, &bytes);
    let actual = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect::<String>();
    if actual.to_ascii_lowercase() != asset.sha256.to_ascii_lowercase() {
        return Err("downloaded executable SHA-256 does not match the signed manifest".into());
    }
    let path = std::env::temp_dir().join(format!("atria-update-{}.exe", manifest.version));
    std::fs::write(&path, bytes).map_err(|e| format!("cannot stage verified update: {e}"))?;
    Ok((manifest.version, path))
}

#[cfg(windows)]
pub fn launch_replacement(source: &std::path::Path, target: &std::path::Path, pid: u32) -> Result<(), String> {
    use std::process::Command;
    let parent = target.parent().ok_or_else(|| "application install folder is unavailable".to_string())?;
    let probe = parent.join(format!(".atria-write-test-{pid}"));
    std::fs::write(&probe, b"Atria update permission test").map_err(|_| "Windows blocked automatic replacement in this install folder; download the verified installer manually".to_string())?;
    let _ = std::fs::remove_file(probe);
    let script_path = std::env::temp_dir().join(format!("atria-update-replace-{pid}.ps1"));
    let script = r#"param([string]$Source,[string]$Target,[int]$WaitPid)
for ($i=0; $i -lt 180; $i++) {
  try { Get-Process -Id $WaitPid -ErrorAction Stop | Out-Null; Start-Sleep -Milliseconds 500 }
  catch { break }
}
try {
  $backup = "$Target.previous"
  Copy-Item -LiteralPath $Target -Destination $backup -Force
  Move-Item -LiteralPath $Source -Destination $Target -Force
  Start-Process -FilePath $Target
} catch {
  exit 1
}
"#;
    std::fs::write(&script_path, script).map_err(|e| format!("cannot create update helper: {e}"))?;
    Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(script_path)
        .arg("-Source").arg(source)
        .arg("-Target").arg(target)
        .arg("-WaitPid").arg(pid.to_string())
        .spawn().map_err(|e| format!("cannot start update helper: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn launch_replacement(_: &std::path::Path, _: &std::path::Path, _: u32) -> Result<(), String> {
    Err("automatic replacement is currently supported on Windows only".into())
}

#[cfg(test)]
mod tests {
    use super::{decode_hex, verify_manifest, version_tuple};

    #[test]
    fn parses_versions_and_hex_safely() {
        assert_eq!(version_tuple("v0.8.0").unwrap(), (0, 8, 0));
        assert!(version_tuple("v0.8").is_err());
        assert_eq!(decode_hex("00aF").unwrap(), vec![0, 175]);
        assert!(decode_hex("0x").is_err());
    }

    #[test]
    fn rejects_manifest_with_untrusted_signature() {
        let raw = br#"{"version":"0.8.0","tag":"v0.8.0","release_url":"https://github.com/Manixpcshot/atria-ai/releases/tag/v0.8.0","assets":[{"name":"atria.exe","url":"https://github.com/Manixpcshot/atria-ai/releases/download/v0.8.0/atria.exe","sha256":"0000000000000000000000000000000000000000000000000000000000000000","size":1}]}"#;
        assert!(verify_manifest(raw, &"00".repeat(64), "v0.8.0").is_err());
    }
}
