//! Kod parowania (6 cyfr) i tokeny sparowanych telefonów. Tokeny trafiają do pliku, żeby telefon nie musiał
//! parować się po restarcie programu. Przy pierwszym uruchomieniu przejmujemy tokeny dawnego OpenTakeoff Link,
//! więc telefony sparowane z poprzednim programem łączą się bez nowego kodu.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub const MAX_FAILURES: u32 = 5;
pub const BLOCK_TIME: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq)]
pub enum PairResult {
    Ok(String),
    Failed,
    RateLimited,
}

/// Sparowany telefon (do listy urządzeń w programie).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Device {
    pub token: String,
    pub name: String,
    pub paired_at: i64,
    pub last_seen: i64,
    pub last_ip: String,
    pub last_sync: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    devices: Vec<Device>,
}

struct Inner {
    code: String,
    devices: Vec<Device>,
    failures: HashMap<String, (u32, Option<Instant>)>,
}

pub struct Pairing {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).expect("losowanie");
    b
}

fn new_code() -> String {
    let b = random_bytes(4);
    let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) % 1_000_000;
    format!("{n:06}")
}

fn new_token() -> String {
    base64::engine::general_purpose::STANDARD.encode(random_bytes(24)).replace('+', "-").replace('/', "_")
}

fn fixed_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Pairing {
    /// `path` = None – tylko w pamięci (testy).
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut devices = Vec::new();
        if let Some(p) = &path {
            if let Ok(text) = std::fs::read_to_string(p) {
                if let Ok(s) = serde_json::from_str::<Store>(&text) {
                    devices = s.devices;
                }
            } else {
                // Pierwsze uruchomienie: tokeny dawnego programu (lista tekstów).
                let legacy = crate::paths::legacy_link().join("tokens.json");
                if let Some(tokens) = std::fs::read_to_string(legacy).ok().and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()) {
                    let now = crate::source::unix_ms();
                    devices = tokens.into_iter().map(|token| Device { token, name: String::new(), paired_at: now, ..Default::default() }).collect();
                }
            }
        }
        let p = Self { path, inner: Mutex::new(Inner { code: new_code(), devices, failures: HashMap::new() }) };
        p.save();
        p
    }

    pub fn code(&self) -> String {
        self.inner.lock().code.clone()
    }

    pub fn regenerate_code(&self) {
        self.inner.lock().code = new_code();
    }

    pub fn devices(&self) -> Vec<Device> {
        self.inner.lock().devices.clone()
    }

    pub fn paired_count(&self) -> usize {
        self.inner.lock().devices.len()
    }

    pub fn revoke_all(&self) {
        self.inner.lock().devices.clear();
        self.save();
    }

    pub fn revoke(&self, token: &str) {
        self.inner.lock().devices.retain(|d| d.token != token);
        self.save();
    }

    pub fn is_valid_token(&self, token: &str) -> bool {
        !token.is_empty() && self.inner.lock().devices.iter().any(|d| fixed_time_eq(&d.token, token))
    }

    /// Nazwa, adres i czas ostatniego połączenia urządzenia.
    pub fn touch(&self, token: &str, name: &str, ip: &str) {
        let mut g = self.inner.lock();
        if let Some(d) = g.devices.iter_mut().find(|d| d.token == token) {
            if !name.is_empty() {
                d.name = name.to_string();
            }
            d.last_ip = ip.to_string();
            d.last_seen = crate::source::unix_ms();
        }
        drop(g);
        self.save();
    }

    pub fn mark_synced(&self, token: &str) {
        let mut g = self.inner.lock();
        if let Some(d) = g.devices.iter_mut().find(|d| d.token == token) {
            d.last_sync = crate::source::unix_ms();
        }
        drop(g);
        self.save();
    }

    pub fn device_name(&self, token: &str) -> Option<String> {
        self.inner.lock().devices.iter().find(|d| d.token == token).map(|d| d.name.clone())
    }

    pub fn try_pair(&self, ip: &str, code: Option<&str>, token: Option<&str>) -> PairResult {
        let now = Instant::now();
        let mut g = self.inner.lock();
        if let Some((_, Some(until))) = g.failures.get(ip) {
            if *until > now {
                return PairResult::RateLimited;
            }
        }
        if let Some(t) = token.filter(|t| !t.is_empty()) {
            if g.devices.iter().any(|d| fixed_time_eq(&d.token, t)) {
                g.failures.remove(ip);
                return PairResult::Ok(t.to_string());
            }
        }
        if let Some(c) = code.filter(|c| !c.is_empty()) {
            if fixed_time_eq(c, &g.code) {
                g.failures.remove(ip);
                let token = new_token();
                g.devices.push(Device { token: token.clone(), paired_at: crate::source::unix_ms(), last_ip: ip.to_string(), ..Default::default() });
                drop(g);
                self.save();
                return PairResult::Ok(token);
            }
        }
        let count = g.failures.get(ip).map(|f| f.0).unwrap_or(0) + 1;
        let entry = if count >= MAX_FAILURES { (0, Some(now + BLOCK_TIME)) } else { (count, None) };
        g.failures.insert(ip.to_string(), entry);
        PairResult::Failed
    }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        let devices = self.inner.lock().devices.clone();
        let unique: HashSet<&str> = devices.iter().map(|d| d.token.as_str()).collect();
        let devices: Vec<Device> = devices.iter().filter(|d| unique.contains(d.token.as_str())).cloned().collect();
        if let Ok(text) = serde_json::to_string_pretty(&Store { devices }) {
            let _ = crate::paths::write_atomic(p, text.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_with_code_then_token() {
        let p = Pairing::new(None);
        let code = p.code();
        let PairResult::Ok(token) = p.try_pair("1.1.1.1", Some(&code), None) else { panic!() };
        assert_eq!(p.try_pair("1.1.1.1", None, Some(&token)), PairResult::Ok(token.clone()));
        assert!(p.is_valid_token(&token));
        p.regenerate_code();
        assert_eq!(p.try_pair("1.1.1.1", None, Some(&token)), PairResult::Ok(token.clone()));
        p.revoke_all();
        assert_eq!(p.try_pair("1.1.1.1", None, Some(&token)), PairResult::Failed);
    }

    #[test]
    fn rate_limit_after_failures() {
        let p = Pairing::new(None);
        for _ in 0..4 {
            assert_eq!(p.try_pair("2.2.2.2", Some("bad"), None), PairResult::Failed);
        }
        assert_eq!(p.try_pair("2.2.2.2", Some("bad"), None), PairResult::Failed);
        let code = p.code();
        assert_eq!(p.try_pair("2.2.2.2", Some(&code), None), PairResult::RateLimited);
        assert!(matches!(p.try_pair("3.3.3.3", Some(&code), None), PairResult::Ok(_)));
    }

    #[test]
    fn code_is_six_digits() {
        let c = new_code();
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|c| c.is_ascii_digit()));
        assert!(!new_token().contains('+'));
    }
}
