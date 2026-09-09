use std::collections::HashMap;
use std::sync::Arc;

use rebook_engine::ReaderSpread;
use rebook_engine::frame::SpreadFrameKey;

use crate::compositor::{ReaderCompositor, StaticSpreadLayers};

pub const DEFAULT_SCENE_CACHE_CAPACITY: usize = 5;

pub struct SpreadSceneCache {
    capacity: usize,
    entries: HashMap<SpreadFrameKey, Arc<StaticSpreadLayers>>,
    lru_order: Vec<SpreadFrameKey>,
}

impl SpreadSceneCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::with_capacity(capacity),
            lru_order: Vec::with_capacity(capacity),
        }
    }

    pub fn get_or_build(
        &mut self,
        key: &SpreadFrameKey,
        spread: &ReaderSpread,
    ) -> Arc<StaticSpreadLayers> {
        if let Some(layers) = self.entries.get(key).cloned() {
            self.touch(key);
            return layers;
        }

        let layers = Arc::new(ReaderCompositor::build_static_layers(spread, key.clone()));

        if self.entries.len() >= self.capacity {
            self.evict_oldest();
        }

        self.entries.insert(key.clone(), Arc::clone(&layers));
        self.lru_order.push(key.clone());

        layers
    }

    pub fn invalidate_all(&mut self) {
        self.entries.clear();
        self.lru_order.clear();
    }

    fn touch(&mut self, key: &SpreadFrameKey) {
        if let Some(pos) = self.lru_order.iter().position(|k| k == key) {
            let item = self.lru_order.remove(pos);
            self.lru_order.push(item);
        }
    }

    fn evict_oldest(&mut self) {
        if !self.lru_order.is_empty() {
            let oldest = self.lru_order.remove(0);
            self.entries.remove(&oldest);
        }
    }
}
