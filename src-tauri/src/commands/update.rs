use std::cmp::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const LATEST_API: &str = "https://api.github.com/repos/JerryJian/AidTerm/releases/latest";

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateInfo {
    current_version: String,
    latest_version: String,
    has_update: bool,
    release_url: String,
    asset_name: Option<String>,
    asset_url: Option<String>,
    published_at: Option<String>,
    body: Option<String>,
    installer_type: String,
}

#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn parse_version(v: &str) -> Vec<u32> {
    v.trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

fn compare_versions(a: &str, b: &str) -> Ordering {
    let pa = parse_version(a);
    let pb = parse_version(b);
    let len = pa.len().max(pb.len());
    for i in 0..len {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

fn pick_asset(assets: &[GithubAsset], installer: &str) -> (Option<String>, Option<String>) {
    let os = std::env::consts::OS;
    let arch_key = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        a => a,
    };
    let mut best: Option<&GithubAsset> = None;
    for asset in assets {
        let name = asset.name.to_lowercase();
        if name.contains("electron") {
            continue;
        }
        let ok = match os {
            "windows" => {
                if installer == "msi" {
                    name.ends_with(".msi")
                } else {
                    name.ends_with(".exe") && name.contains("setup")
                }
            }
            "macos" => name.ends_with(".dmg"),
            "linux" => name.ends_with(".appimage") || name.ends_with(".deb"),
            _ => false,
        };
        if !ok {
            continue;
        }
        if name.contains(&format!("_{arch_key}")) {
            best = Some(asset);
            break;
        }
        if best.is_none() {
            best = Some(asset);
        }
    }
    best.map(|a| (Some(a.name.clone()), Some(a.browser_download_url.clone())))
        .unwrap_or((None, None))
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

#[cfg(target_os = "windows")]
fn get_installer_type() -> String {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    use winreg::RegKey;

    const APP_ID: &str = "com.jwlsn.aidterm";
    const PRODUCT_NAME: &str = "AidTerm";
    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let flags = KEY_READ | view;
            let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(UNINSTALL, flags) else {
                continue;
            };
            // NSIS registers under the app id; MSI registers under a product GUID.
            if let Ok(key) = root.open_subkey_with_flags(APP_ID, flags) {
                if key.get_raw_value("WindowsInstaller").is_ok() {
                    return "msi".into();
                }
                return "nsis".into();
            }
            // MSI fallback: scan for our product name and check WindowsInstaller.
            for sub in root.enum_keys() {
                let Ok(name) = sub else { continue };
                let Ok(k) = root.open_subkey_with_flags(&name, flags) else { continue };
                if let Ok(display) = k.get_value::<String, _>("DisplayName") {
                    if display.eq_ignore_ascii_case(PRODUCT_NAME) {
                        if k.get_raw_value("WindowsInstaller").is_ok() {
                            return "msi".into();
                        }
                        return "nsis".into();
                    }
                }
            }
        }
    }
    "unknown".into()
}

#[cfg(not(target_os = "windows"))]
fn get_installer_type() -> String {
    "unknown".into()
}

#[tauri::command(rename = "get_installer_type")]
pub fn get_installer_type_command() -> String {
    get_installer_type()
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(LATEST_API)
        .header("User-Agent", format!("AidTerm/{}", current_version(&app)))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Update check failed: HTTP {}", resp.status()));
    }
    let release: GithubRelease = resp.json().await.map_err(|e| e.to_string())?;
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let current = current_version(&app);
    let has_update = compare_versions(&latest, &current) == Ordering::Greater;
    let installer = get_installer_type();
    let (asset_name, asset_url) = pick_asset(&release.assets, &installer);
    Ok(UpdateInfo {
        current_version: current,
        latest_version: latest,
        has_update,
        release_url: release.html_url,
        asset_name,
        asset_url,
        published_at: release.published_at,
        body: release.body,
        installer_type: installer,
    })
}

#[tauri::command]
pub async fn download_update(app: AppHandle, url: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .header("User-Agent", format!("AidTerm/{}", current_version(&app)))
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let fname = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("AidTerm-update");
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(fname);
    let mut file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;

    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut received: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        if total > 0 && last_emit.elapsed().as_millis() >= 100 {
            let _ = app.emit(
                "update-progress",
                serde_json::json!({ "received": received, "total": total }),
            );
            last_emit = std::time::Instant::now();
        }
    }
    let _ = app.emit(
        "update-progress",
        serde_json::json!({ "received": received, "total": total }),
    );
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub fn install_update(app: AppHandle, path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    #[cfg(target_os = "windows")]
    {
        if p.extension().map(|e| e.eq_ignore_ascii_case("msi")).unwrap_or(false) {
            std::process::Command::new("msiexec")
                .arg("/i")
                .arg(&p)
                .arg("/qn")
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            std::process::Command::new(&p)
                .arg("/S")
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1200));
            app.exit(0);
        });
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
        if p.to_string_lossy().ends_with(".AppImage") {
            std::process::Command::new(&p)
                .spawn()
                .map_err(|e| e.to_string())?;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1200));
                app.exit(0);
            });
        } else {
            std::process::Command::new("xdg-open")
                .arg(&p)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
