//! The four words both screens show at pairing (PLAN-SERVER-CLIENT §4.1
//! layer 4): derived from the server's and the device's public keys, so a man
//! in the middle — who must present his own key to each side — makes them
//! differ. 44 bits from SHA-256, four BIP-39 English words.

use sha2::{Digest, Sha256};

const LIST: &str = include_str!("bip39-english.txt");

fn wordlist() -> Vec<&'static str> {
    LIST.lines().collect()
}

pub fn pairing_words(server: &[u8; 32], device: &[u8; 32]) -> [String; 4] {
    let mut h = Sha256::new();
    h.update(b"quantsuite-pairing-v1");
    h.update(server);
    h.update(device);
    let d = h.finalize();
    let bits = u64::from_be_bytes([d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]]);
    let list = wordlist();
    let pick = |i: u32| list[((bits >> (64 - 11 * (i + 1))) & 0x7ff) as usize].to_string();
    [pick(0), pick(1), pick(2), pick(3)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_bip39_english() {
        let list = wordlist();
        assert_eq!(list.len(), 2048);
        assert_eq!((list[0], list[2047]), ("abandon", "zoo"));
        assert_eq!(hex::encode(Sha256::digest(LIST.as_bytes())), "2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda");
    }

    #[test]
    fn the_words_change_when_either_key_is_swapped() {
        let (s, d, m) = ([1u8; 32], [2u8; 32], [3u8; 32]);
        let w = pairing_words(&s, &d);
        assert_eq!(w, pairing_words(&s, &d), "the same on both screens");
        assert_ne!(w, pairing_words(&m, &d), "a man in the middle shows the device his key");
        assert_ne!(w, pairing_words(&s, &m), "and the server his");
        assert_ne!(w, pairing_words(&d, &s), "the order matters");
    }
}
