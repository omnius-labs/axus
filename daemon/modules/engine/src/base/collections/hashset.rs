use std::hash::Hash;
use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};

use omnius_core_base::clock::Clock;

pub struct VolatileHashSet<T> {
    map: HashMap<T, DateTime<Utc>>,
    expired_time: Duration,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

#[allow(unused)]
impl<T> VolatileHashSet<T>
where
    T: Hash + Eq,
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
        self.map.retain(|_, v| now - *v < expired_time);
    }

    pub fn shrink(&mut self, max_size: usize) {
        self.refresh();

        if self.map.len() <= max_size {
            return;
        }

        let mut entries: Vec<(T, DateTime<Utc>)> = self.map.drain().collect();
        entries.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        entries.truncate(max_size);

        self.map = entries.into_iter().collect();
    }

    pub fn insert(&mut self, value: T) {
        self.map.insert(value, self.clock.now());
    }

    pub fn extend(&mut self, values: impl IntoIterator<Item = T>) {
        let now = self.clock.now();
        self.map.extend(values.into_iter().map(|v| (v, now)));
    }

    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    pub fn remove(&mut self, value: &T) {
        self.map.remove(value);
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.map.keys()
    }
}
