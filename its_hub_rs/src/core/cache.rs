//! LRU token cache for vLLM response caching.
//!
//! Caches responses keyed by (model, messages_hash, temperature, max_tokens, budget).
//! Thread-safe via `Arc<RwLock<>>` for entries and `AtomicU64` for counters.
//! Designed for the high-concurrency gateway path where Rust's performance
//! advantage matters most.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::RwLock;

/// Cache key combining request parameters that determine a unique response.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct CacheKey {
    pub model: String,
    pub messages_hash: u64,
    /// f64 bits stored as u64 for hashing.
    pub temperature: Option<u64>,
    pub max_tokens: Option<u32>,
    /// Budget value: different budgets produce different results.
    pub budget: u32,
}

impl CacheKey {
    /// Build a cache key from request parameters.
    /// Temperature is converted to bits for deterministic hashing.
    pub fn new(
        model: &str,
        messages: &serde_json::Value,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        budget: u32,
    ) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let msg_str = serde_json::to_string(messages).unwrap_or_default();
        msg_str.hash(&mut hasher);
        let messages_hash = hasher.finish();

        Self {
            model: model.to_string(),
            messages_hash,
            temperature: temperature.map(|t| t.to_bits()),
            max_tokens,
            budget,
        }
    }
}

struct CacheEntry {
    response: serde_json::Value,
    created_at: Instant,
    last_accessed: Instant,
}

/// Statistics about cache performance.
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub entries: usize,
    pub max_entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub ttl_seconds: u64,
}

/// Thread-safe LRU token cache with TTL eviction.
pub struct TokenCache {
    entries: Arc<RwLock<HashMap<CacheKey, CacheEntry>>>,
    max_entries: usize,
    ttl: Duration,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl TokenCache {
    pub fn new(max_entries: usize, ttl_seconds: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            ttl: Duration::from_secs(ttl_seconds),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Look up a cached response. Returns None on miss or if expired.
    ///
    /// Uses a read lock first for non-expired hits. Only acquires a write lock
    /// when the entry is expired (to remove it) or on a hit (to bump access time).
    pub async fn get(&self, key: &CacheKey) -> Option<serde_json::Value> {
        // First, try a read lock to check for a valid entry
        {
            let entries = self.entries.read().await;
            if let Some(entry) = entries.get(key) {
                if entry.created_at.elapsed() <= self.ttl {
                    // Valid hit: clone response, then upgrade to write lock to bump access time
                    let response = entry.response.clone();
                    drop(entries);
                    // Bump last_accessed under write lock
                    {
                        let mut entries = self.entries.write().await;
                        if let Some(entry) = entries.get_mut(key) {
                            entry.last_accessed = Instant::now();
                        }
                    }
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    return Some(response);
                }
                // Entry is expired: fall through to remove it
            } else {
                // Key not found at all
                self.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        }

        // Entry was expired: take write lock and remove it
        {
            let mut entries = self.entries.write().await;
            entries.remove(key);
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Store a response in the cache. Evicts expired entries and LRU if at capacity.
    pub async fn put(&self, key: CacheKey, response: serde_json::Value) {
        let mut entries = self.entries.write().await;

        // Evict expired entries first
        let now = Instant::now();
        let ttl = self.ttl;
        entries.retain(|_, v| now.duration_since(v.created_at) < ttl);

        // If still at capacity, evict the least recently accessed entry
        if entries.len() >= self.max_entries {
            if let Some(lru_key) = entries
                .iter()
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&lru_key);
            }
        }

        let now = Instant::now();
        entries.insert(
            key,
            CacheEntry {
                response,
                created_at: now,
                last_accessed: now,
            },
        );
    }

    /// Return cache performance statistics.
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        CacheStats {
            entries: entries.len(),
            max_entries: self.max_entries,
            hits,
            misses,
            hit_rate,
            ttl_seconds: self.ttl.as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_key(model: &str) -> CacheKey {
        CacheKey::new(
            model,
            &json!([{"role": "user", "content": "hello"}]),
            Some(0.7),
            Some(100),
            5,
        )
    }

    #[tokio::test]
    async fn cache_put_and_get() {
        let cache = TokenCache::new(100, 300);
        let key = test_key("model-a");
        let response = json!({"role": "assistant", "content": "hi"});

        cache.put(key.clone(), response.clone()).await;
        let cached = cache.get(&key).await;

        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), response);
    }

    #[tokio::test]
    async fn cache_miss_on_different_key() {
        let cache = TokenCache::new(100, 300);
        let key1 = test_key("model-a");
        let key2 = test_key("model-b");
        let response = json!({"role": "assistant", "content": "hi"});

        cache.put(key1, response).await;
        let cached = cache.get(&key2).await;

        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn cache_ttl_expiry() {
        let cache = TokenCache::new(100, 0); // 0 second TTL
        let key = test_key("model-a");
        let response = json!({"role": "assistant", "content": "hi"});

        cache.put(key.clone(), response).await;
        // TTL is 0 seconds, so it should be expired immediately
        tokio::time::sleep(Duration::from_millis(10)).await;
        let cached = cache.get(&key).await;

        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn cache_evicts_at_capacity() {
        let cache = TokenCache::new(2, 300);

        cache
            .put(test_key("a"), json!({"content": "a"}))
            .await;
        cache
            .put(test_key("b"), json!({"content": "b"}))
            .await;
        // Access "a" so its last_accessed is bumped; "b" becomes the LRU entry
        cache.get(&test_key("a")).await;
        // This should evict the LRU entry ("b")
        cache
            .put(test_key("c"), json!({"content": "c"}))
            .await;

        let stats = cache.stats().await;
        assert_eq!(stats.entries, 2);

        // "b" should be evicted (least recently accessed)
        assert!(cache.get(&test_key("b")).await.is_none());
        // "a" and "c" should still be present
        assert!(cache.get(&test_key("a")).await.is_some());
        assert!(cache.get(&test_key("c")).await.is_some());
    }

    #[tokio::test]
    async fn cache_stats_tracking() {
        let cache = TokenCache::new(100, 300);
        let key = test_key("model-a");
        let response = json!({"role": "assistant", "content": "hi"});

        cache.put(key.clone(), response).await;
        cache.get(&key).await; // hit
        cache.get(&key).await; // hit
        cache.get(&test_key("miss")).await; // miss

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.6667).abs() < 0.01);
        assert_eq!(stats.entries, 1);
    }

    #[tokio::test]
    async fn cache_key_temperature_sensitivity() {
        let key1 = CacheKey::new(
            "model",
            &json!([{"role": "user", "content": "hi"}]),
            Some(0.7),
            Some(100),
            5,
        );
        let key2 = CacheKey::new(
            "model",
            &json!([{"role": "user", "content": "hi"}]),
            Some(0.8),
            Some(100),
            5,
        );
        assert_ne!(key1, key2);
    }

    #[tokio::test]
    async fn cache_key_budget_sensitivity() {
        let key1 = CacheKey::new(
            "model",
            &json!([{"role": "user", "content": "hi"}]),
            Some(0.7),
            Some(100),
            1,
        );
        let key2 = CacheKey::new(
            "model",
            &json!([{"role": "user", "content": "hi"}]),
            Some(0.7),
            Some(100),
            100,
        );
        assert_ne!(key1, key2);
    }
}
