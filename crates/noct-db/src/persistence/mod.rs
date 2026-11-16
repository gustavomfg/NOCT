//! Persistence helpers shared by the stores that write files: atomic replacement, serialized
//! writes per key, and the choice of the user-data directory. Port of `electron/persistence`.

pub mod atomic_file;
pub mod product_data;
pub mod write_queue;
