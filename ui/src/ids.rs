//! New entity IDs for sections, entries and bullets added in the editor: a
//! ULID from the clock and the browser's secure random source (spec section 4.1).

use cv_core::Id;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A fresh ULID.
pub fn new_id() -> Id {
    let mut random = [0u8; 10];
    if let Some(crypto) = web_sys::window().and_then(|w| w.crypto().ok()) {
        // Without a crypto source the bytes stay zero; the timestamp and the
        // store's duplicate-ID check still apply.
        let _ = crypto.get_random_values_with_u8_array(&mut random);
    }
    // Milliseconds since 1970 fit in the 48-bit ULID timestamp until 10889.
    let ms = js_sys::Date::now() as u64;
    encode(ms, random)
}

/// Encodes a 48-bit millisecond timestamp and 80 random bits as a ULID.
pub fn encode(ms: u64, random: [u8; 10]) -> Id {
    let mut value: u128 = u128::from(ms & 0xFFFF_FFFF_FFFF) << 80;
    for (i, b) in random.iter().enumerate() {
        value |= u128::from(*b) << (8 * (9 - i));
    }
    let mut text = [0u8; 26];
    for (i, slot) in text.iter_mut().enumerate() {
        let shift = 5 * (25 - i);
        *slot = CROCKFORD[((value >> shift) & 0x1F) as usize];
    }
    let text = String::from_utf8_lossy(&text);
    Id::parse(&text).unwrap_or_else(|e| unreachable!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_known_ulid() {
        // Timestamp 0 and all-zero randomness.
        assert_eq!(encode(0, [0; 10]).as_str(), "00000000000000000000000000");
        // 1_469_918_176_385 ms is "01ARYZ6S41" in the ULID spec's example.
        assert!(
            encode(1_469_918_176_385, [0; 10])
                .as_str()
                .starts_with("01ARYZ6S41")
        );
        assert_eq!(
            encode(u64::MAX, [0xFF; 10]).as_str(),
            "7ZZZZZZZZZZZZZZZZZZZZZZZZZ"
        );
    }
}
