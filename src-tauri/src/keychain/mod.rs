use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::process::Command;

pub struct KeychainManager {
    keys_dir: PathBuf,
    keys: Mutex<HashMap<String, KeyInfo>>,
    index_path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct KeyInfo {
    pub id: String,
    pub name: String,
    pub key_type: String,
    pub bits: u32,
    pub public_key: String,
    pub fingerprint: String,
    pub private_key_path: String,
    pub public_key_path: String,
    pub created_at: String,
}

impl KeychainManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let keys_dir = app_data_dir.join("keys");
        let _ = fs::create_dir_all(&keys_dir);
        let index_path = app_data_dir.join("keys_index.json");
        let keys = Mutex::new(Self::load_index(&index_path));
        Self { keys_dir, keys, index_path }
    }

    fn load_index(path: &PathBuf) -> HashMap<String, KeyInfo> {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<HashMap<String, KeyInfo>>(&s).ok())
            .unwrap_or_default()
    }

    fn save_index(&self) {
        if let Ok(keys) = self.keys.lock() {
            if let Ok(content) = serde_json::to_string_pretty(&*keys) {
                let _ = fs::write(&self.index_path, &content);
            }
        }
    }

    fn run_ssh_keygen(args: &[&str]) -> Result<String, String> {
        let output = Command::new("ssh-keygen")
            .args(args)
            .output()
            .map_err(|e| format!("ssh-keygen not found: {}. Install OpenSSH client.", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ssh-keygen failed: {}", stderr));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn generate_rsa(&self, name: String, bits: u32, passphrase: Option<String>) -> Result<KeyInfo, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let priv_path = self.keys_dir.join(format!("{}_{}", &name, "id_rsa"));
        let priv_str = priv_path.to_string_lossy().to_string();
        let bits_str = bits.to_string();
        let comment = format!("aidterm-{}", name);

        let mut args = vec![
            "-t", "rsa",
            "-b", bits_str.as_str(),
            "-f", priv_str.as_str(),
            "-C", comment.as_str(),
        ];

        if let Some(ref pass) = passphrase {
            args.extend_from_slice(&["-N", pass]);
        } else {
            args.extend_from_slice(&["-N", ""]);
        }

        Self::run_ssh_keygen(&args)?;

        let pub_path = self.keys_dir.join(format!("{}_{}.pub", &name, "id_rsa"));
        let pub_content = fs::read_to_string(&pub_path)
            .map_err(|e| format!("Failed to read public key: {}", e))?;

        let fingerprint = Self::run_ssh_keygen(&["-l", "-f", &priv_str])?;
        let fingerprint = fingerprint.trim().split_whitespace().next().unwrap_or("").to_string();

        let info = KeyInfo {
            id,
            name,
            key_type: "RSA".to_string(),
            bits,
            public_key: pub_content.trim().to_string(),
            fingerprint,
            private_key_path: priv_str,
            public_key_path: pub_path.to_string_lossy().to_string(),
            created_at: format!("{:?}", std::time::SystemTime::now()),
        };

        let mut keys = self.keys.lock().map_err(|e| e.to_string())?;
        keys.insert(info.id.clone(), info.clone());
        drop(keys);
        self.save_index();

        Ok(info)
    }

    pub fn generate_ed25519(&self, name: String, passphrase: Option<String>) -> Result<KeyInfo, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let priv_path = self.keys_dir.join(format!("{}_{}", &name, "id_ed25519"));
        let priv_str = priv_path.to_string_lossy().to_string();
        let comment = format!("aidterm-{}", name);

        let mut args = vec![
            "-t", "ed25519",
            "-f", priv_str.as_str(),
            "-C", comment.as_str(),
        ];

        if let Some(ref pass) = passphrase {
            args.extend_from_slice(&["-N", pass]);
        } else {
            args.extend_from_slice(&["-N", ""]);
        }

        Self::run_ssh_keygen(&args)?;

        let pub_path = self.keys_dir.join(format!("{}_{}.pub", &name, "id_ed25519"));
        let pub_content = fs::read_to_string(&pub_path)
            .map_err(|e| format!("Failed to read public key: {}", e))?;

        let fingerprint = Self::run_ssh_keygen(&["-l", "-f", &priv_str])?;
        let fingerprint = fingerprint.trim().split_whitespace().next().unwrap_or("").to_string();

        let info = KeyInfo {
            id,
            name,
            key_type: "ED25519".to_string(),
            bits: 256,
            public_key: pub_content.trim().to_string(),
            fingerprint,
            private_key_path: priv_str,
            public_key_path: pub_path.to_string_lossy().to_string(),
            created_at: format!("{:?}", std::time::SystemTime::now()),
        };

        let mut keys = self.keys.lock().map_err(|e| e.to_string())?;
        keys.insert(info.id.clone(), info.clone());
        drop(keys);
        self.save_index();

        Ok(info)
    }

    pub fn list(&self) -> Result<Vec<KeyInfo>, String> {
        let keys = self.keys.lock().map_err(|e| e.to_string())?;
        let mut list: Vec<KeyInfo> = keys.values().cloned().collect();
        list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(list)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut keys = self.keys.lock().map_err(|e| e.to_string())?;
        if let Some(info) = keys.remove(id) {
            let _ = fs::remove_file(&info.private_key_path);
            let _ = fs::remove_file(&info.public_key_path);
            drop(keys);
            self.save_index();
        }
        Ok(())
    }

    pub fn import(&self, name: String, private_key_path: String) -> Result<KeyInfo, String> {
        // Import an existing key - just registers it in our index
        let path = PathBuf::from(&private_key_path);
        if !path.exists() {
            return Err("Private key file not found".to_string());
        }

        let id = uuid::Uuid::new_v4().to_string();
        let pub_path = path.with_extension("pub");
        let pub_content = if pub_path.exists() {
            fs::read_to_string(&pub_path).unwrap_or_default()
        } else {
            // Try to extract public key
            String::new()
        };

        let fingerprint = Self::run_ssh_keygen(&["-l", "-f", &private_key_path])
            .ok()
            .and_then(|out| out.trim().split_whitespace().next().map(|s| s.to_string()))
            .unwrap_or_default();

        let key_type = if pub_content.contains("ssh-ed25519") {
            "ED25519"
        } else if pub_content.contains("ssh-rsa") {
            "RSA"
        } else {
            "Unknown"
        };

        let info = KeyInfo {
            id,
            name,
            key_type: key_type.to_string(),
            bits: 0,
            public_key: pub_content.trim().to_string(),
            fingerprint,
            private_key_path,
            public_key_path: pub_path.to_string_lossy().to_string(),
            created_at: format!("{:?}", std::time::SystemTime::now()),
        };

        let mut keys = self.keys.lock().map_err(|e| e.to_string())?;
        keys.insert(info.id.clone(), info.clone());
        drop(keys);
        self.save_index();

        Ok(info)
    }
}
