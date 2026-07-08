use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;

/// Shared state for Zmodem responses from frontend.
pub struct ZmodemState {
    pub responses: Mutex<HashMap<String, Option<String>>>,
}

impl ZmodemState {
    pub fn new() -> Self {
        Self { responses: Mutex::new(HashMap::new()) }
    }
}

/// Check if data contains Zmodem init sequence: `**\x18` (ZPAD + ZDLD)
pub fn detect_init(data: &[u8]) -> bool {
    data.windows(3).any(|w| w == b"*\x18" || w == &[0x2a, 0x2a, 0x18])
}

/// Save captured Zmodem data to file.
#[allow(dead_code)]
pub fn save_to_file(path: &str, data: &[u8]) -> Result<(), String> {
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to create file: {}", e))?;
    file.write_all(data)
        .map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}
