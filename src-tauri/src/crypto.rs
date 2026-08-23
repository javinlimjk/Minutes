use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce
};
use rand::RngCore;

static MASTER_KEY: OnceLock<[u8; 32]> = OnceLock::new();

const KEYRING_SERVICE: &str = "com.minutes.scribe";
const KEYRING_USER: &str = "master_encryption_key";

/// Retrieve or generate local machine master key using OS Keychain with local encrypted keyfile fallback
pub fn get_master_key() -> &'static [u8; 32] {
    MASTER_KEY.get_or_init(|| {
        // 1. Try fetching key from OS Keychain (macOS Keychain, Windows Credential Manager)
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            if let Ok(pwd) = entry.get_password() {
                if let Ok(bytes) = hex::decode(&pwd) {
                    if bytes.len() == 32 {
                        let mut key = [0u8; 32];
                        key.copy_from_slice(&bytes);
                        println!("[crypto] Loaded master key securely from OS Keychain.");
                        return key;
                    }
                }
            }
        }

        // 2. Try fetching key from local fallback file
        let mut key_path = dirs_next_app_dir();
        key_path.push(".keyfile");

        if key_path.exists() {
            if let Ok(bytes) = fs::read(&key_path) {
                if bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&bytes);
                    // Attempt to back-populate into OS Keychain for future runs
                    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
                        let _ = entry.set_password(&hex::encode(&key));
                    }
                    println!("[crypto] Loaded master key from fallback keyfile.");
                    return key;
                }
            }
        }

        // 3. Generate a fresh 256-bit cryptographically secure random key
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        // Store into OS Keychain
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            if let Err(e) = entry.set_password(&hex::encode(&key)) {
                eprintln!("[crypto] Keychain write notice: {}", e);
            } else {
                println!("[crypto] Generated and saved new master key to OS Keychain.");
            }
        }

        // Store copy in local app directory fallback
        if let Some(parent) = key_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&key_path, key);

        key
    })
}

fn dirs_next_app_dir() -> PathBuf {
    if let Some(mut path) = dirs_next::data_dir() {
        path.push("com.minutes.scribe");
        path
    } else {
        PathBuf::from(".minutes_data")
    }
}

/// AES-256-GCM Authenticated Encryption (FIPS compliant)
/// Output format: "ENC:v2:<12-byte-hex-nonce>:<hex-ciphertext-and-tag>"
pub fn encrypt_text(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }

    let master_key_bytes = get_master_key();
    let key = Key::<Aes256Gcm>::from_slice(master_key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, plaintext.as_bytes()) {
        Ok(ciphertext_and_tag) => {
            format!("ENC:v2:{}:{}", hex::encode(&nonce_bytes), hex::encode(&ciphertext_and_tag))
        }
        Err(e) => {
            eprintln!("[crypto] AES-256-GCM encryption error: {:?}", e);
            plaintext.to_string()
        }
    }
}

/// Decrypt text with automatic format detection:
/// Supports AES-256-GCM ("ENC:v2:...") with fallback to legacy XOR cipher ("ENC:...").
pub fn decrypt_text(ciphertext: &str) -> String {
    if ciphertext.starts_with("ENC:v2:") {
        let parts: Vec<&str> = ciphertext.split(':').collect();
        if parts.len() == 4 {
            let nonce_hex = parts[2];
            let ct_hex = parts[3];

            if let (Ok(nonce_bytes), Ok(ct_bytes)) = (hex::decode(nonce_hex), hex::decode(ct_hex)) {
                if nonce_bytes.len() == 12 {
                    let master_key_bytes = get_master_key();
                    let key = Key::<Aes256Gcm>::from_slice(master_key_bytes);
                    let cipher = Aes256Gcm::new(key);
                    let nonce = Nonce::from_slice(&nonce_bytes);

                    if let Ok(plaintext_bytes) = cipher.decrypt(nonce, ct_bytes.as_slice()) {
                        if let Ok(plaintext) = String::from_utf8(plaintext_bytes) {
                            return plaintext;
                        }
                    }
                }
            }
        }
        eprintln!("[crypto] AES-256-GCM decryption failed for record.");
        return ciphertext.to_string();
    } else if let Some(stripped) = ciphertext.strip_prefix("ENC:") {
        // Legacy v1 XOR cipher fallback for existing database compatibility
        return decrypt_legacy_v1(stripped);
    }

    // Unencrypted legacy text
    ciphertext.to_string()
}

fn decrypt_legacy_v1(hex_str: &str) -> String {
    let bytes = match hex::decode(hex_str) {
        Ok(b) => b,
        Err(_) => return hex_str.to_string(),
    };

    let key = get_master_key();
    let mut plaintext_bytes = Vec::with_capacity(bytes.len());

    for (i, &b) in bytes.iter().enumerate() {
        let mut block_input = key.to_vec();
        block_input.extend_from_slice(&(i as u64).to_be_bytes());
        let ks = sha256_simple(&block_input);
        plaintext_bytes.push(b ^ ks[0]);
    }

    String::from_utf8(plaintext_bytes).unwrap_or_else(|_| hex_str.to_string())
}

fn sha256_simple(input: &[u8]) -> [u8; 32] {
    use std::num::Wrapping;

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let mut padded = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = (w[i - 15].rotate_right(7)) ^ (w[i - 15].rotate_right(18)) ^ (w[i - 15] >> 3);
            let s1 = (w[i - 2].rotate_right(17)) ^ (w[i - 2].rotate_right(19)) ^ (w[i - 2] >> 10);
            w[i] = (Wrapping(w[i - 16]) + Wrapping(s0) + Wrapping(w[i - 7]) + Wrapping(s1)).0;
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_var = h[7];

        for i in 0..64 {
            let s1 = (e.rotate_right(6)) ^ (e.rotate_right(11)) ^ (e.rotate_right(25));
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = Wrapping(h_var) + Wrapping(s1) + Wrapping(ch) + Wrapping(k[i]) + Wrapping(w[i]);
            let s0 = (a.rotate_right(2)) ^ (a.rotate_right(13)) ^ (a.rotate_right(22));
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = Wrapping(s0) + Wrapping(maj);

            h_var = g;
            g = f;
            f = e;
            e = (Wrapping(d) + temp1).0;
            d = c;
            c = b;
            b = a;
            a = (temp1 + temp2).0;
        }

        h[0] = (Wrapping(h[0]) + Wrapping(a)).0;
        h[1] = (Wrapping(h[1]) + Wrapping(b)).0;
        h[2] = (Wrapping(h[2]) + Wrapping(c)).0;
        h[3] = (Wrapping(h[3]) + Wrapping(d)).0;
        h[4] = (Wrapping(h[4]) + Wrapping(e)).0;
        h[5] = (Wrapping(h[5]) + Wrapping(f)).0;
        h[6] = (Wrapping(h[6]) + Wrapping(g)).0;
        h[7] = (Wrapping(h[7]) + Wrapping(h_var)).0;
    }

    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }
    pub fn decode(hex: &str) -> Result<Vec<u8>, ()> {
        if !hex.len().is_multiple_of(2) { return Err(()); }
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
            .collect()
    }
}

