//! LRU token cache for vLLM response caching.
//!
//! Caches responses keyed by (model, messages_hash, temperature, max_tokens).
//! Thread-safe via `Arc<RwLock<>>`. Designed for the high-concurrency gateway
//! path where Rust's performance advantage matters most.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
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
}

impl CacheKey {
    /// Build a cache key from request parameters.
    /// Temperature is converted to bits for deterministic hashing.
    pub fn new(
        model: &str,
        messages: &serde_json::Value,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
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
        }
    }
}

struct CacheEntry {
    response: serde_json::Value,
    created_at: Instant,
    access_count: u64,
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
    hits: Arc<RwLock<u64>>,
    misses: Arc<RwLock<u64>>,
}

impl TokenCache {
    pub fn new(max_entries: usize, ttl_seconds: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            ttl: Duration::from_secs(ttl_seconds),
            hits: Arc::new(RwLock::new(0)),
            misses: Arc::new(RwLock::new(0)),
        }
    }

    /// Look up a cached response. Returns None on miss or if expired.
    pub async fn get(&self, key: &CacheKey) -> Option<serde_json::Value> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            if entry.created_at.elapsed() > self.ttl {
                entries.remove(key);
                let mut misses = self.misses.write().await;
                *misses += 1;
                return None;
            }
            entry.access_count += 1;
            let response = entry.response.clone();
            let mut hits = self.hits.write().await;
            *hits += 1;
            Some(response)
        } else {
            let mut misses = self.misses.write().await;
            *misses += 1;
            None
        }
    }

    /// Store a response in the cache. Evicts expired entries and oldest if at capacity.
    pub async fn put(&self, key: CacheKey, response: serde_json::Value) {
        let mut entries = self.entries.write().await;

        // Evict expired entries first
        let now = Instant::now();
        let ttl = self.ttl;
        entries.retain(|_, v| now.duration_since(v.created_at) < ttl);

        // If still at capacity, evict the entry with the oldest creation time
        if entries.len() >= self.max_entries {
            if let Some(oldest_key) = entries
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest_key);
            }
        }

        entries.insert(
            key,
            CacheEntry {
                response,
                created_at: Instant::now(),
                access_count: 0,
            },
        );
    }

    /// Return cache performance statistics.
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        let hits = *self.hits.read().await;
        let misses = *self.misses.read().await;
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
        // This should evict the oldest entry ("a")
        cache
            .put(test_key("c"), json!({"content": "c"}))
            .await;

        let stats = cache.stats().await;
        assert_eq!(stats.entries, 2);

        // "a" should be evicted
        assert!(cache.get(&test_key("a")).await.is_none());
        // "b" and "c" should still be present
        assert!(cache.get(&test_key("b")).await.is_some());
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
        );
        let key2 = CacheKey::new(
            "model",
            &json!([{"role": "user", "content": "hi"}]),
            Some(0.8),
            Some(100),
        );
        assert_ne!(key1, key2);
    }
}
