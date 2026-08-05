use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct KnownHostEntry {
    pub host: String,
    pub key_type: String,
    pub fingerprint: String,
    pub line: String,
}

pub struct KnownHostsManager {
    known_hosts_path: PathBuf,
    entries: Mutex<HashMap<String, KnownHostEntry>>,
}

impl KnownHostsManager {
    pub fn new(home_dir: PathBuf) -> Self {
        let ssh_dir = home_dir.join(".ssh");
        let known_hosts_path = ssh_dir.join("known_hosts");
        let entries = Mutex::new(HashMap::new());
        let manager = Self { known_hosts_path, entries };
        let _ = manager.reload();
        manager
    }

    pub fn reload(&self) -> Result<(), String> {
        let content = match fs::read_to_string(&self.known_hosts_path) {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };

        let mut entries = self.entries.lock().map_err(|e| e.to_string())?;
        entries.clear();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            // Skip marker lines (@cert-authority / @revoked): they are not host
            // entries and must be preserved verbatim by add/remove.
            if parts[0].starts_with('@') {
                continue;
            }

            let host = parts[0].to_string();
            let key_type = parts[1].to_string();
            let key = parts[2].to_string();
            let fingerprint = format!("{} {}...{}",
                key_type,
                &key[..std::cmp::min(16, key.len())],
                &key[key.len().saturating_sub(8)..],
            );

            let entry = KnownHostEntry {
                host: host.clone(),
                key_type: key_type.clone(),
                fingerprint,
                line: line.to_string(),
            };
            let map_key = format!("{}|{}", host, key_type);
            entries.insert(map_key, entry);
        }

        Ok(())
    }

    pub fn list(&self) -> Result<Vec<KnownHostEntry>, String> {
        let entries = self.entries.lock().map_err(|e| e.to_string())?;
        let mut list: Vec<KnownHostEntry> = entries.values().cloned().collect();
        list.sort_by(|a, b| a.host.cmp(&b.host));
        Ok(list)
    }

    pub fn add(&self, host: &str, key_type: &str, key: &str) -> Result<(), String> {
        if let Some(dir) = self.known_hosts_path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| format!("Failed to create ~/.ssh: {}", e))?;
        }
        let line = format!("{} {} {}\n", host, key_type, key);
        let existing = fs::read_to_string(&self.known_hosts_path).unwrap_or_default();
        fs::write(&self.known_hosts_path, format!("{}{}", existing, line))
            .map_err(|e| format!("Failed to write known_hosts: {}", e))?;
        self.reload()
    }

    pub fn remove(&self, host: &str, key_type: &str) -> Result<(), String> {
        let content = match fs::read_to_string(&self.known_hosts_path) {
            Ok(c) => c,
            Err(_) => {
                let mut entries = self.entries.lock().map_err(|e| e.to_string())?;
                entries.remove(&format!("{}|{}", host, key_type));
                return Ok(());
            }
        };

        // Rewrite the file line-by-line, dropping only the matching host entry.
        // Comments, blank lines, hashed hosts (`|1|...`), @cert-authority /
        // @revoked markers and the original order are all preserved.
        let mut removed = false;
        let mut out = String::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let is_match = parts.len() >= 3
                && !parts[0].starts_with('@')
                && parts[0].eq_ignore_ascii_case(host)
                && parts[1].eq_ignore_ascii_case(key_type);
            if is_match {
                removed = true;
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        if removed {
            fs::write(&self.known_hosts_path, out)
                .map_err(|e| format!("Failed to write known_hosts: {}", e))?;
            self.reload()?;
        }
        Ok(())
    }

}
