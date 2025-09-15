use std::borrow::Borrow;
use std::hash::Hash;
use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};

use omnius_core_base::clock::Clock;

struct ValueEntry<T> {
    pub value: T,
    pub created_time: DateTime<Utc>,
}

pub struct VolatileHashMap<K, V> {
    map: HashMap<K, ValueEntry<V>>,
    expired_time: Duration,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

#[allow(unused)]
impl<K, V> VolatileHashMap<K, V>
where
    K: Hash + Eq,
{
    pub fn new(expired_time: Duration, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Self {
        Self {
            map: HashMap::new(),
            expired_time,
            clock: clock.clone(),
        }
    }

    pub fn refresh(&mut self) {
        let now = self.clock.now();
        let expired_time = self.expired_time;
        self.map.retain(|_, v| now - v.created_time < expired_time);
    }

    pub fn shrink(&mut self, max_size: usize) {
        self.refresh();

        if self.map.len() <= max_size {
            return;
        }

        let mut entries: Vec<(K, ValueEntry<V>)> = self.map.drain().collect();
        entries.sort_by_key(|(_, v)| std::cmp::Reverse(v.created_time));
        entries.truncate(max_size);

        self.map = entries.into_iter().collect();
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.map.insert(
            key,
            ValueEntry {
                value,
                created_time: self.clock.now(),
            },
        );
    }

    pub fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        let now = self.clock.now();
        self.map.extend(iter.into_iter().map(|(k, v)| (k, ValueEntry { value: v, created_time: now })));
    }

    pub fn contains_key<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(k)
    }

    pub fn get<Q>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.get(k).as_ref().map(|n| &n.value)
    }

    pub fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.remove(k).map(|n| n.value)
    }

    pub fn clear(&mut self) {
        self.map.clear()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter().map(|(k, v)| (k, &v.value))
    }
}
