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
    // 新客户端的空缓存从 Some(0) 起算；将 None 留给代次溢出，确保此后不再接受在途 GET 回填。
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
    // 发起远程 GET 前取得当前代次，用于判断读取期间是否发生对象变更。None 表示溢出并已禁用回填。
    pub(crate) fn generation(&self) -> Option<u64> {
        self.generation
    }

    // 对象写入或删除时移除同键字节并推进全局代次，防止变更前或变更中的 GET 回填旧值。
    // 代次溢出后保持 None，不从零重新开始。
    pub(crate) fn invalidate(&mut self, key: &str) {
        self.remove(key);
        self.generation = self.generation.and_then(|value| value.checked_add(1));
    }

    // 成功 GET 后仅在开始读取时的代次仍有效且未变化时回填。允许回填时先清除同键旧条目，
    // 但没有指纹或超过单对象上限的内容不入缓存。
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

    // 不可变对象读取先按键和登记指纹查缓存。指纹不符或写入已满 30 秒时删除条目，交给调用方重新 GET。
    // 命中返回字节副本，不延长绝对期限；应传入不早于写入时刻的单调时间。
    pub(crate) fn read(&mut self, key: &str, fingerprint: &str, now: Instant) -> Option<Vec<u8>> {
        let entry = self.entries.get(key)?;
        if entry.fingerprint != fingerprint || now.duration_since(entry.stored_at) >= TTL {
            self.remove(key);
            return None;
        }
        Some(entry.bytes.clone())
    }

    // 成功 GET 的内容才可入缓存。先移除同键旧条目，再拒绝无指纹或超过 512 KiB 的对象；
    // 写入时按存入时间从早到晚淘汰，确保总量不超过 8 MiB、条目不超过 256 项。
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

    // 失效、淘汰或同键替换时统一移除条目并扣减字节数；键已不存在时无需更新计数。
    pub(crate) fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.total_bytes -= entry.bytes.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 固定时刻验证命中不会续期，并确认指纹变化会立即清除旧字节，不能等到过期才失效。
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

    // 验证超限或无指纹的对象不占用缓存，以及主动删除后查不到对象且字节计数归零。
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

    // 分别用大对象和小对象触发字节上限与条目上限，确认两种情况下都淘汰最早写入的条目。
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

    // 模拟写入前与写入中开始的 GET，确认两者都不能在失效后回填；代次溢出也不允许回填。
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
