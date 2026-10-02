//! 有界的不可变对象字节缓存；不得存储授权结果或完整 HTTP 响应。

use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_OBJECT_BYTES: usize = 512 * 1024;
const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const TTL: Duration = Duration::from_secs(30);

pub(crate) struct ContentCache {
    entries: HashMap<String, Entry>,
    total_bytes: usize,
    generation: Option<u64>,
}

impl Default for ContentCache {
    /// 代次溢出后永久禁用回填，不能使在途旧读取误命中新代次。
    fn default() -> Self {
        Self { entries: HashMap::new(), total_bytes: 0, generation: Some(0) }
    }
}

struct Entry {
    fingerprint: String,
    bytes: Vec<u8>,
    stored_at: Instant,
}

impl ContentCache {
    /// 在 GET 前冻结代次，GET 完成后只能向同一代次回填。
    pub(crate) fn generation(&self) -> Option<u64> {
        self.generation
    }

    /// 任一对象变更的开始或结束都会推进代次，阻止跨变更的在途 GET 回填。
    pub(crate) fn invalidate(&mut self, key: &str) {
        self.remove(key);
        self.generation = self.generation.and_then(|value| value.checked_add(1));
    }

    /// 变更期间读到的旧对象不得在变更完成后重新进入缓存。
    pub(crate) fn insert_if_current(
        &mut self,
        generation: Option<u64>,
        key: String,
        fingerprint: String,
        bytes: &[u8],
        now: Instant,
    ) {
        if generation.is_some() && generation == self.generation {
            self.insert(key, fingerprint, bytes, now);
        }
    }

    /// 内容身份或绝对有效期变化时不返回旧字节；命中不延长有效期。
    pub(crate) fn read(&mut self, key: &str, fingerprint: &str, now: Instant) -> Option<Vec<u8>> {
        let entry = self.entries.get(key)?;
        if entry.fingerprint != fingerprint || now.duration_since(entry.stored_at) >= TTL {
            self.remove(key);
            return None;
        }
        Some(entry.bytes.clone())
    }

    /// 仅接收已经成功从对象存储取得的小对象，按年龄淘汰并限制总内存。
    pub(crate) fn insert(&mut self, key: String, fingerprint: String, bytes: &[u8], now: Instant) {
        self.remove(&key);
        if fingerprint.is_empty() || bytes.len() > MAX_OBJECT_BYTES {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES || self.total_bytes + bytes.len() > MAX_TOTAL_BYTES {
            let Some(oldest) =
                self.entries.iter().min_by_key(|(_, entry)| entry.stored_at).map(|(key, _)| key.clone())
            else {
                break;
            };
            self.remove(&oldest);
        }
        self.total_bytes += bytes.len();
        self.entries.insert(key, Entry { fingerprint, bytes: bytes.to_vec(), stored_at: now });
    }

    /// 应用发出对象变更前清除该键，失败的变更亦不得继续使用旧缓存。
    pub(crate) fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.total_bytes -= entry.bytes.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 有效缓存命中后，内容身份变化与到期都必须失效。
    #[test]
    fn fingerprint_and_absolute_expiry_control_hits() {
        let now = Instant::now();
        let mut cache = ContentCache::default();
        cache.insert("key".into(), "first".into(), b"private", now);
        assert_eq!(cache.read("key", "first", now + Duration::from_secs(29)), Some(b"private".to_vec()));
        assert_eq!(cache.read("key", "first", now + TTL), None);
        cache.insert("key".into(), "first".into(), b"private", now);
        assert_eq!(cache.read("key", "changed", now), None);
        assert_eq!(cache.total_bytes, 0);
    }

    /// 超大对象、空身份和主动删除不得留下缓存字节。
    #[test]
    fn rejects_large_or_unidentified_objects_and_removes_bytes() {
        let now = Instant::now();
        let mut cache = ContentCache::default();
        cache.insert("large".into(), "hash".into(), &vec![0; MAX_OBJECT_BYTES + 1], now);
        cache.insert("empty".into(), String::new(), b"private", now);
        assert_eq!(cache.total_bytes, 0);
        cache.insert("key".into(), "hash".into(), b"private", now);
        cache.remove("key");
        assert_eq!(cache.read("key", "hash", now), None);
        assert_eq!(cache.total_bytes, 0);
    }

    /// 总字节数与条目数都必须有界，并优先淘汰最早进入的对象。
    #[test]
    fn eviction_bounds_memory_and_entry_count() {
        let now = Instant::now();
        let mut cache = ContentCache::default();
        for index in 0..17 {
            cache.insert(
                index.to_string(),
                "hash".into(),
                &vec![0; MAX_OBJECT_BYTES],
                now + Duration::from_secs(index),
            );
        }
        assert_eq!(cache.total_bytes, MAX_TOTAL_BYTES);
        assert_eq!(cache.read("0", "hash", now + Duration::from_secs(17)), None);
        let mut small = ContentCache::default();
        for index in 0..257 {
            small.insert(index.to_string(), "hash".into(), b"x", now + Duration::from_millis(index));
        }
        assert_eq!(small.entries.len(), MAX_ENTRIES);
        assert_eq!(small.read("0", "hash", now + Duration::from_secs(1)), None);
    }

    /// GET 在写入开始前或写入过程中开始，都不得跨过写入完成回填旧内容。
    #[test]
    fn concurrent_mutation_prevents_stale_get_refill() {
        let now = Instant::now();
        let mut cache = ContentCache::default();
        let before_write = cache.generation();
        cache.invalidate("key");
        let during_write = cache.generation();
        cache.insert_if_current(before_write, "key".into(), "hash".into(), b"old", now);
        assert_eq!(cache.read("key", "hash", now), None);
        cache.invalidate("key");
        cache.insert_if_current(during_write, "key".into(), "hash".into(), b"old", now);
        assert_eq!(cache.read("key", "hash", now), None);
        cache.insert_if_current(cache.generation(), "key".into(), "hash".into(), b"current", now);
        assert_eq!(cache.read("key", "hash", now), Some(b"current".to_vec()));
        cache.generation = Some(u64::MAX);
        cache.invalidate("key");
        cache.insert_if_current(None, "key".into(), "hash".into(), b"old", now);
        assert_eq!(cache.read("key", "hash", now), None);
    }
}
