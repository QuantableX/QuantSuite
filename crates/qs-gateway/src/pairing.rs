//! The one-time pairing code (PLAN-SERVER-CLIENT §4.1 layer 4): 8 digits,
//! valid 5 minutes, used once; 5 wrong codes lock pairing for 15 minutes —
//! new codes included, so guessing cannot continue on a fresh code.

use serde::Serialize;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub const CODE_DIGITS: usize = 8;
pub const CODE_VALID: Duration = Duration::from_secs(5 * 60);
pub const MAX_ATTEMPTS: u32 = 5;
pub const LOCKOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CodeError {
    NoCode,
    Expired,
    Wrong { attempts_left: u32 },
    Locked { secs: u64 },
}

impl std::fmt::Display for CodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCode => write!(f, "No pairing code is open on the server. Start pairing there first."),
            Self::Expired => write!(f, "The pairing code has expired. Start pairing again on the server."),
            Self::Wrong { attempts_left } => write!(f, "Wrong pairing code ({attempts_left} attempts left)."),
            Self::Locked { secs } => write!(
                f,
                "Pairing is locked after {MAX_ATTEMPTS} wrong codes. Try again in {} min.",
                secs.div_ceil(60)
            ),
        }
    }
}

struct Active {
    code: Zeroizing<String>,
    issued: Instant,
}

#[derive(Default)]
pub struct Pairing {
    active: Option<Active>,
    failures: u32,
    locked_until: Option<Instant>,
}

/// Equal in time independent of where the inputs differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn random_code() -> Result<String, CodeError> {
    use ring::rand::SecureRandom;
    let rng = ring::rand::SystemRandom::new();
    // Rejection sampling: 4,200,000,000 is the largest multiple of 10^8
    // below 2^32, so every code is equally likely.
    loop {
        let mut b = [0u8; 4];
        rng.fill(&mut b).map_err(|_| CodeError::NoCode)?;
        let n = u32::from_be_bytes(b);
        if n < 4_200_000_000 {
            return Ok(format!("{:08}", n % 100_000_000));
        }
    }
}

impl Pairing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn locked_for(&self, now: Instant) -> Option<Duration> {
        self.locked_until.and_then(|t| t.checked_duration_since(now)).filter(|d| !d.is_zero())
    }

    /// A new code; replaces an open one. Refused while locked.
    pub fn start(&mut self, now: Instant) -> Result<String, CodeError> {
        let code = random_code()?;
        self.start_with(code.clone(), now)?;
        Ok(code)
    }

    pub fn start_with(&mut self, code: String, now: Instant) -> Result<(), CodeError> {
        if let Some(d) = self.locked_for(now) {
            return Err(CodeError::Locked { secs: d.as_secs().max(1) });
        }
        self.active = Some(Active { code: Zeroizing::new(code), issued: now });
        self.failures = 0;
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.active = None;
    }

    /// The open code and how long it stays valid.
    pub fn open(&mut self, now: Instant) -> Option<(String, Duration)> {
        let left = self.active.as_ref().and_then(|a| CODE_VALID.checked_sub(now.duration_since(a.issued)));
        match (left, self.active.as_ref()) {
            (Some(l), Some(a)) if !l.is_zero() => Some((a.code.to_string(), l)),
            _ => {
                self.active = None;
                None
            }
        }
    }

    /// One attempt; a right code is used up.
    pub fn check(&mut self, attempt: &str, now: Instant) -> Result<(), CodeError> {
        if let Some(d) = self.locked_for(now) {
            return Err(CodeError::Locked { secs: d.as_secs().max(1) });
        }
        let Some(active) = self.active.as_ref() else {
            return Err(CodeError::NoCode);
        };
        if now.duration_since(active.issued) >= CODE_VALID {
            self.active = None;
            return Err(CodeError::Expired);
        }
        let digits: String = attempt.chars().filter(|c| c.is_ascii_digit()).collect();
        if same(digits.as_bytes(), active.code.as_bytes()) {
            self.active = None;
            self.failures = 0;
            return Ok(());
        }
        self.failures += 1;
        if self.failures >= MAX_ATTEMPTS {
            self.active = None;
            self.failures = 0;
            self.locked_until = Some(now + LOCKOUT);
            return Err(CodeError::Locked { secs: LOCKOUT.as_secs() });
        }
        Err(CodeError::Wrong { attempts_left: MAX_ATTEMPTS - self.failures })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_works_once_and_with_spaces() {
        let t = Instant::now();
        let mut p = Pairing::new();
        let code = p.start(t).unwrap();
        assert_eq!(code.len(), CODE_DIGITS);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        let spaced = format!("{} {}", &code[..4], &code[4..]);
        assert_eq!(p.check(&spaced, t + Duration::from_secs(10)), Ok(()));
        assert_eq!(p.check(&code, t + Duration::from_secs(11)), Err(CodeError::NoCode), "used up");
        assert_ne!(Pairing::new().start(t).unwrap(), code, "random (a 1 in 10^8 flake)");
    }

    #[test]
    fn an_expired_code_fails() {
        let t = Instant::now();
        let mut p = Pairing::new();
        p.start_with("12345678".into(), t).unwrap();
        assert!(p.open(t + CODE_VALID - Duration::from_secs(1)).is_some());
        assert_eq!(p.check("12345678", t + CODE_VALID), Err(CodeError::Expired));
        assert_eq!(p.check("12345678", t + CODE_VALID), Err(CodeError::NoCode));
    }

    #[test]
    fn five_wrong_codes_lock_pairing_for_fifteen_minutes() {
        let t = Instant::now();
        let mut p = Pairing::new();
        p.start_with("12345678".into(), t).unwrap();
        for left in (1..MAX_ATTEMPTS).rev() {
            assert_eq!(p.check("00000000", t), Err(CodeError::Wrong { attempts_left: left }));
        }
        assert_eq!(p.check("00000000", t), Err(CodeError::Locked { secs: LOCKOUT.as_secs() }));
        // The right code no longer helps, nor does a fresh code.
        assert!(matches!(p.check("12345678", t), Err(CodeError::Locked { .. })));
        assert!(matches!(p.start(t + Duration::from_secs(60)), Err(CodeError::Locked { .. })));
        // After the lock, pairing works again.
        let later = t + LOCKOUT + Duration::from_secs(1);
        p.start_with("87654321".into(), later).unwrap();
        assert_eq!(p.check("87654321", later), Ok(()));
        assert!(CodeError::Locked { secs: 840 }.to_string().contains("14 min"));
    }
}
