use bytes::Bytes;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
enum DatabaseError {
    PoisonedLock,
}
/// We use Arc to share it across connection tasks and RwLock to allow
/// concurrent reads but exclusive writes.
#[derive(Clone, Debug)]
pub struct KvStore {
    // We use Bytes because it's cheap to clone (reference counted)
    db: Arc<RwLock<HashMap<String, Bytes>>>,
}

impl Default for KvStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KvStore {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Sets a value in the store.
    pub fn set(&self, key: String, value: Bytes) -> Result<(), DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        db.insert(key, value);
        Ok(())
    }

    /// Gets a value from the store.
    /// Returns None if the key does not exist.
    pub fn get(&self, key: &str) -> Result<Option<Bytes>, DatabaseError> {
        let db = self.db.read().map_err(|_| DatabaseError::PoisonedLock)?;
        Ok(db.get(key).cloned()) // Cloning Bytes is O(1)
    }

    /// Removes a key from the store.
    pub fn del(&self, key: &str) -> Result<bool, DatabaseError> {
        let mut db = self.db.write().map_err(|_| DatabaseError::PoisonedLock)?;
        Ok(db.remove(key).is_some())
    }
}
