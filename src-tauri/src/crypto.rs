use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use std::fs;
use std::path::PathBuf;

fn key_path(app_data_dir: &PathBuf) -> PathBuf {
    app_data_dir.join(".store_key")
}

fn load_or_create_key(app_data_dir: &PathBuf) -> Result<[u8; 32], String> {
    let path = key_path(app_data_dir);
    if path.exists() {
        let data = fs::read(&path).map_err(|e| format!("Failed to read key file: {}", e))?;
        if data.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&data);
            return Ok(key);
        }
    }
    let mut key = [0u8; 32];
    let rng = SystemRandom::new();
    rng.fill(&mut key)
        .map_err(|e| format!("Failed to generate key: {}", e))?;
    fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("Failed to create app data dir: {}", e))?;
    fs::write(&path, key).map_err(|e| format!("Failed to write key file: {}", e))?;
    Ok(key)
}

pub fn encrypt_password(plaintext: &str, app_data_dir: &PathBuf) -> Result<String, String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let key = load_or_create_key(app_data_dir)?;
    let unbound_key =
        UnboundKey::new(&AES_256_GCM, &key).map_err(|e| format!("Invalid key: {}", e))?;
    let key = LessSafeKey::new(unbound_key);

    let mut nonce_bytes = [0u8; 12];
    let rng = SystemRandom::new();
    rng.fill(&mut nonce_bytes)
        .map_err(|e| format!("Failed to generate nonce: {}", e))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.as_bytes().to_vec();
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&in_out);
    Ok(base64::engine::general_purpose::STANDARD.encode(&result))
}

pub fn decrypt_password(encoded: &str, app_data_dir: &PathBuf) -> Result<String, String> {
    if encoded.is_empty() {
        return Ok(String::new());
    }
    let key = load_or_create_key(app_data_dir)?;
    let unbound_key =
        UnboundKey::new(&AES_256_GCM, &key).map_err(|e| format!("Invalid key: {}", e))?;
    let key = LessSafeKey::new(unbound_key);

    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    if data.len() < 12 {
        return Err("Encrypted data too short".into());
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce_bytes: [u8; 12] = nonce_bytes
        .try_into()
        .map_err(|_| "Invalid nonce length".to_string())?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = ciphertext.to_vec();
    let plaintext = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| "Decryption failed (wrong key or corrupted data)".to_string())?;

    String::from_utf8(plaintext.to_vec())
        .map_err(|e| format!("UTF-8 decode failed: {}", e))
}

