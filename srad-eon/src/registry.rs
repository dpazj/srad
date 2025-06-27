use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;



pub struct Registry {
}

impl Registry {
    pub fn new() -> Self {
        Self {
            device_ids: HashMap::new(),
        }
    }

}
