//! Step-up (PLAN-SERVER-CLIENT §4.1 layer 6): `external` calls from another
//! device — arming live execution, starting a live bot, closing positions,
//! changing a key — need the server PIN. A right PIN is valid 10 minutes for
//! that device; 5 wrong PINs lock it for 15 minutes. The server keeps only an
//! Argon2id hash of the PIN.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const STEP_UP_VALID: Duration = Duration::from_secs(10 * 60);
pub const PIN_ATTEMPTS: u32 = 5;
pub const PIN_LOCKOUT: Duration = Duration::from_secs(15 * 60);

pub fn check_pin_format(pin: &str) -> Result<(), String> {
    if (6..=12).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err("The server PIN has 6 to 12 digits.".into())
    }
}

pub fn hash_pin(pin: &str) -> Result<String, String> {
    check_pin_format(pin)?;
    use ring::rand::SecureRandom;
    let mut raw = [0u8; 16];
    ring::rand::SystemRandom::new().fill(&mut raw).map_err(|_| "no system randomness".to_string())?;
    let salt = SaltString::encode_b64(&raw).map_err(|e| format!("PIN: {e}"))?;
    Argon2::default().hash_password(pin.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| format!("PIN: {e}"))
}

pub fn verify_pin(hash: &str, pin: &str) -> bool {
    PasswordHash::new(hash).map(|h| Argon2::default().verify_password(pin.as_bytes(), &h).is_ok()).unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepUpError {
    NoPin,
    Wrong { attempts_left: u32 },
    Locked { secs: u64 },
}

impl std::fmt::Display for StepUpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPin => write!(f, "No server PIN is set. Set one on the server under Settings → Server."),
            Self::Wrong { attempts_left } => write!(f, "Wrong PIN ({attempts_left} attempts left)."),
            Self::Locked { secs } => write!(f, "Too many wrong PINs. Try again in {} min.", secs.div_ceil(60)),
        }
    }
}

#[derive(Default)]
pub struct StepUps {
    grants: HashMap<String, Instant>,
    failures: HashMap<String, u32>,
    locked: HashMap<String, Instant>,
}

impl StepUps {
    pub fn granted(&self, device: &str, now: Instant) -> bool {
        self.grants.get(device).is_some_and(|until| *until > now)
    }

    /// Before the (slow) PIN check: refused while the device is locked.
    pub fn check_locked(&self, device: &str, now: Instant) -> Result<(), StepUpError> {
        match self.locked.get(device).and_then(|t| t.checked_duration_since(now)) {
            Some(d) if !d.is_zero() => Err(StepUpError::Locked { secs: d.as_secs().max(1) }),
            _ => Ok(()),
        }
    }

    /// The outcome of a PIN check.
    pub fn record(&mut self, device: &str, right: bool, now: Instant) -> Result<Duration, StepUpError> {
        self.check_locked(device, now)?;
        if right {
            self.failures.remove(device);
            self.locked.remove(device);
            self.grants.insert(device.to_string(), now + STEP_UP_VALID);
            return Ok(STEP_UP_VALID);
        }
        let count = {
            let n = self.failures.entry(device.to_string()).or_insert(0);
            *n += 1;
            *n
        };
        if count >= PIN_ATTEMPTS {
            self.failures.remove(device);
            self.grants.remove(device);
            self.locked.insert(device.to_string(), now + PIN_LOCKOUT);
            return Err(StepUpError::Locked { secs: PIN_LOCKOUT.as_secs() });
        }
        Err(StepUpError::Wrong { attempts_left: PIN_ATTEMPTS - count })
    }

    /// A revoked device or a changed role loses its grant (a lock stays).
    pub fn forget(&mut self, device: &str) {
        self.grants.remove(device);
        self.failures.remove(device);
    }

    /// A new PIN ends every grant.
    pub fn clear_grants(&mut self) {
        self.grants.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_is_hashed_and_checked() {
        let h = hash_pin("482915").unwrap();
        assert!(!h.contains("482915"));
        assert!(verify_pin(&h, "482915"));
        assert!(!verify_pin(&h, "482916"));
        assert!(!verify_pin("not a hash", "482915"));
        assert!(hash_pin("12345").is_err() && hash_pin("12345a").is_err() && hash_pin("1234567890123").is_err());
    }

    #[test]
    fn a_grant_lasts_ten_minutes_and_wrong_pins_lock() {
        let t = Instant::now();
        let mut s = StepUps::default();
        assert!(!s.granted("d", t));
        assert_eq!(s.record("d", true, t), Ok(STEP_UP_VALID));
        assert!(s.granted("d", t + STEP_UP_VALID - Duration::from_secs(1)));
        assert!(!s.granted("d", t + STEP_UP_VALID));
        assert!(!s.granted("other", t), "per device");

        for left in (1..PIN_ATTEMPTS).rev() {
            assert_eq!(s.record("e", false, t), Err(StepUpError::Wrong { attempts_left: left }));
        }
        assert!(matches!(s.record("e", false, t), Err(StepUpError::Locked { .. })));
        assert!(matches!(s.record("e", true, t), Err(StepUpError::Locked { .. })), "even the right PIN");
        assert_eq!(s.record("e", true, t + PIN_LOCKOUT), Ok(STEP_UP_VALID));
        s.forget("e");
        assert!(!s.granted("e", t + PIN_LOCKOUT));
    }
}
