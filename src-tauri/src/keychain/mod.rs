use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use russh::keys::{Algorithm, HashAlg, PrivateKey};
use russh::keys::ssh_key::LineEnding;
use russh::keys::ssh_key::private::RsaKeypair;

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

    fn key_type_of(key: &PrivateKey) -> &'static str {
        match key.public_key().algorithm() {
            Algorithm::Ed25519 => "ED25519",
            Algorithm::Rsa { .. } => "RSA",
            Algorithm::Ecdsa { .. } => "ECDSA",
            _ => "Unknown",
        }
    }

    fn write_key(
        mut key: PrivateKey,
        passphrase: Option<&str>,
        priv_path: &Path,
        pub_path: &Path,
        comment: &str,
    ) -> Result<(String, String), String> {
        let mut rng = rand::rng();
        if let Some(pass) = passphrase.filter(|p| !p.is_empty()) {
            key = key
                .encrypt(&mut rng, pass)
                .map_err(|e| format!("Failed to encrypt key: {}", e))?;
        }
        let public_key = key
            .public_key()
            .to_openssh()
            .map_err(|e| format!("Failed to encode public key: {}", e))?;
        let pub_line = format!("{} {}\n", public_key, comment);
        // write_openssh_file applies 0600 permissions on Unix
        key.write_openssh_file(priv_path, LineEnding::LF)
            .map_err(|e| format!("Failed to write private key: {}", e))?;
        fs::write(pub_path, pub_line.as_bytes())
            .map_err(|e| format!("Failed to write public key: {}", e))?;
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        Ok((pub_line.trim().to_string(), fingerprint))
    }

    pub fn generate_rsa(&self, name: String, bits: u32, passphrase: Option<String>) -> Result<KeyInfo, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let priv_path = self.keys_dir.join(format!("{}_{}", &name, "id_rsa"));
        let priv_str = priv_path.to_string_lossy().to_string();
        let pub_path = self.keys_dir.join(format!("{}_{}.pub", &name, "id_rsa"));
        let comment = format!("aidterm-{}", name);

        let bits = bits.clamp(2048, 8192);
        let mut rng = rand::rng();
        let keypair = RsaKeypair::random(&mut rng, bits as usize)
            .map_err(|e| format!("Failed to generate RSA key: {}", e))?;
        let key = PrivateKey::from(keypair);

        let (public_key, fingerprint) = Self::write_key(
            key,
            passphrase.as_deref(),
            &priv_path,
            &pub_path,
            &comment,
        )?;

        let info = KeyInfo {
            id,
            name,
            key_type: "RSA".to_string(),
            bits,
            public_key,
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
        let pub_path = self.keys_dir.join(format!("{}_{}.pub", &name, "id_ed25519"));
        let comment = format!("aidterm-{}", name);

        let mut rng = rand::rng();
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
            .map_err(|e| format!("Failed to generate key: {}", e))?;

        let (public_key, fingerprint) = Self::write_key(
            key,
            passphrase.as_deref(),
            &priv_path,
            &pub_path,
            &comment,
        )?;

        let info = KeyInfo {
            id,
            name,
            key_type: "ED25519".to_string(),
            bits: 256,
            public_key,
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
        let path = PathBuf::from(&private_key_path);
        if !path.exists() {
            return Err("Private key file not found".to_string());
        }

        let key = russh::keys::load_secret_key(&path, None)
            .map_err(|e| format!("Failed to load private key: {}", e))?;

        let id = uuid::Uuid::new_v4().to_string();
        let pub_path = path.with_extension("pub");
        let pub_content = key
            .public_key()
            .to_openssh()
            .map_err(|e| format!("Failed to extract public key: {}", e))?;
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        let key_type = Self::key_type_of(&key);

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
