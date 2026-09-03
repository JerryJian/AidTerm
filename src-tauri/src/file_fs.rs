//! Local and WSL filesystem backends for the unified file browser.
//!
//! `local` operates directly on the host filesystem. `wsl` maps POSIX paths
//! inside a WSL distro onto the `\\wsl$\<distro>` UNC namespace so the host
//! `std::fs` can read and write them without spawning `wsl.exe`.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::sftp::FileEntry;

#[derive(Debug, Clone)]
pub enum FsTarget {
    Local,
    Wsl { distro: String },
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve(target: &FsTarget, remote: &str) -> Result<PathBuf, String> {
    match target {
        FsTarget::Local => {
            if remote.is_empty() {
                Ok(home_dir())
            } else {
                Ok(PathBuf::from(remote))
            }
        }
        FsTarget::Wsl { distro } => {
            let mut p = PathBuf::from(format!(r"\\wsl$\{}", distro));
            for seg in remote.trim_start_matches('/').split('/') {
                if !seg.is_empty() {
                    p.push(seg);
                }
            }
            Ok(p)
        }
    }
}

fn format_system_time(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, mo, d, h, mi) = civil_from_days(secs.div_euclid(86_400), secs.rem_euclid(86_400));
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, mi)
}

/// Convert days since the Unix epoch plus seconds-of-day into a civil date.
fn civil_from_days(days: i64, sod: i64) -> (i64, u32, u32, u32, u32) {
    let h = (sod / 3600) as u32;
    let mi = ((sod % 3600) / 60) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, h, mi)
}

fn to_entry(name: String, meta: &std::fs::Metadata) -> FileEntry {
    FileEntry {
        name,
        is_dir: meta.is_dir(),
        size: meta.len(),
        modified: meta.modified().map(format_system_time).unwrap_or_default(),
        permissions: String::new(),
    }
}

pub fn list_dir(target: &FsTarget, remote: &str) -> Result<Vec<FileEntry>, String> {
    let dir = resolve(target, remote)?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {}", dir.display(), e))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        out.push(to_entry(entry.file_name().to_string_lossy().into_owned(), &meta));
    }
    Ok(out)
}

pub fn read_file(target: &FsTarget, remote: &str) -> Result<String, String> {
    let p = resolve(target, remote)?;
    std::fs::read_to_string(&p).map_err(|e| format!("{}: {}", p.display(), e))
}

pub fn write_file(target: &FsTarget, remote: &str, content: &str) -> Result<(), String> {
    let p = resolve(target, remote)?;
    std::fs::write(&p, content).map_err(|e| format!("{}: {}", p.display(), e))
}

pub fn remove(target: &FsTarget, remote: &str, is_dir: bool) -> Result<(), String> {
    let p = resolve(target, remote)?;
    let result = if is_dir {
        std::fs::remove_dir_all(&p)
    } else {
        std::fs::remove_file(&p)
    };
    result.map_err(|e| format!("{}: {}", p.display(), e))
}

pub fn rename(target: &FsTarget, old_path: &str, new_path: &str) -> Result<(), String> {
    let old = resolve(target, old_path)?;
    let new = resolve(target, new_path)?;
    std::fs::rename(&old, &new).map_err(|e| format!("{} -> {}: {}", old.display(), new.display(), e))
}

pub fn mkdir(target: &FsTarget, remote: &str) -> Result<(), String> {
    let p = resolve(target, remote)?;
    std::fs::create_dir_all(&p).map_err(|e| format!("{}: {}", p.display(), e))
}

pub fn create_file(target: &FsTarget, remote: &str, is_dir: bool) -> Result<(), String> {
    let p = resolve(target, remote)?;
    if is_dir {
        std::fs::create_dir_all(&p).map_err(|e| format!("{}: {}", p.display(), e))
    } else {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&p)
            .map(|_| ())
            .map_err(|e| format!("{}: {}", p.display(), e))
    }
}

fn copy_file(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("{} -> {}: {}", src.display(), dst.display(), e))
}

/// Copy a browsed (remote) file or directory recursively onto a host path.
pub fn download(target: &FsTarget, remote: &str, local: &str) -> Result<(), String> {
    let src = resolve(target, remote)?;
    let dst = PathBuf::from(local);
    if src.is_dir() {
        if !dst.exists() {
            std::fs::create_dir_all(&dst).map_err(|e| format!("mkdir {}: {}", dst.display(), e))?;
        }
        for entry in std::fs::read_dir(&src).map_err(|e| format!("read_dir {}: {}", src.display(), e))? {
            let entry = entry.map_err(|e| format!("entry: {}", e))?;
            let child_name = entry.file_name();
            let child_remote = format!("{}/{}", remote.trim_end_matches('/'), child_name.to_string_lossy());
            let child_local = dst.join(&child_name);
            download(target, &child_remote, &child_local.to_string_lossy())?;
        }
    } else {
        copy_file(&src, &dst)?;
    }
    Ok(())
}

/// Copy a host path or directory recursively onto the browsed (remote) location.
pub fn upload(target: &FsTarget, local: &str, remote: &str) -> Result<(), String> {
    let dst = resolve(target, remote)?;
    let src = PathBuf::from(local);
    if src.is_dir() {
        if !dst.exists() {
            std::fs::create_dir_all(&dst).map_err(|e| format!("mkdir {}: {}", dst.display(), e))?;
        }
        for entry in std::fs::read_dir(&src).map_err(|e| format!("local readdir {}: {}", src.display(), e))? {
            let entry = entry.map_err(|e| format!("entry: {}", e))?;
            let child_name = entry.file_name();
            let child_local = entry.path();
            let child_remote = format!("{}/{}", remote.trim_end_matches('/'), child_name.to_string_lossy());
            upload(target, &child_local.to_string_lossy(), &child_remote)?;
        }
    } else {
        copy_file(&src, &dst)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_roundtrip() {
        let (y, m, d, h, mi) = civil_from_days(0, 0);
        assert_eq!((y, m, d, h, mi), (1970, 1, 1, 0, 0));
        let (y, m, d, h, mi) = civil_from_days(20_000, 36_699);
        assert_eq!((y, m, d), (2024, 10, 4));
        assert_eq!((h, mi), (10, 11));
    }

    #[test]
    fn wsl_unc_mapping() {
        let t = FsTarget::Wsl { distro: "Ubuntu-22.04".into() };
        let p = resolve(&t, "/home/user/test.txt").unwrap();
        assert_eq!(p.to_string_lossy(), r"\\wsl$\Ubuntu-22.04\home\user\test.txt");
        let root = resolve(&t, "/").unwrap();
        assert_eq!(root.to_string_lossy(), r"\\wsl$\Ubuntu-22.04");
        let empty = resolve(&t, "").unwrap();
        assert_eq!(empty.to_string_lossy(), r"\\wsl$\Ubuntu-22.04");
    }

    #[test]
    fn local_empty_path_resolves_home() {
        let p = resolve(&FsTarget::Local, "").unwrap();
        assert!(!p.as_os_str().is_empty());
    }
}
