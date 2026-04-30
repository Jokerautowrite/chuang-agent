const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

pub fn stable_content_hash(content: &str) -> String {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::stable_content_hash;

    #[test]
    fn stable_content_hash_is_deterministic_for_same_input() {
        let first = stable_content_hash("hello browser worker");
        let second = stable_content_hash("hello browser worker");

        assert_eq!(first, second);
        assert_eq!(first, "ae2212e02f06c4bb");
        assert_eq!(first.len(), 16);
    }

    #[test]
    fn stable_content_hash_changes_when_content_changes() {
        assert_ne!(
            stable_content_hash("done"),
            stable_content_hash("Page summary")
        );
    }
}
