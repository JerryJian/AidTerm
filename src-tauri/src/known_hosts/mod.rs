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

            let host = parts[0].to_string();
            let key_type = parts[1].to_string();
            let key = parts[2].to_string();
            let fingerprint = format!("{} {}...{}",
                key_type,
                &key[..std::cmp::min(16, key.len())],
                &key[key.len()-8..],
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
        let line = format!("{} {} {}\n", host, key_type, key);
        let existing = fs::read_to_string(&self.known_hosts_path).unwrap_or_default();
        fs::write(&self.known_hosts_path, format!("{}{}", existing, line))
            .map_err(|e| format!("Failed to write known_hosts: {}", e))?;
        self.reload()
    }

    pub fn remove(&self, host: &str, key_type: &str) -> Result<(), String> {
        let map_key = format!("{}|{}", host, key_type);
        {
            let mut entries = self.entries.lock().map_err(|e| e.to_string())?;
            entries.remove(&map_key);
        }

        let entries = self.entries.lock().map_err(|e| e.to_string())?;
        let content: String = entries.values()
            .map(|e| e.line.clone() + "\n")
            .collect();
        fs::write(&self.known_hosts_path, content)
            .map_err(|e| format!("Failed to write known_hosts: {}", e))?;
        Ok(())
    }

    pub fn check_host(&self, host: &str, key_type: &str) -> bool {
        let map_key = format!("{}|{}", host, key_type);
        let entries = self.entries.lock().map_err(|_| ()).is_ok();
        if !entries { return false; }
        let entries = self.entries.lock().unwrap();
        entries.contains_key(&map_key)
    }
}
