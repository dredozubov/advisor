pub mod memory;
pub use memory::InMemoryStore;
#[async_trait::async_trait]
pub trait VectorStorage {
    fn is_ephemeral(&self) -> bool;
    // Other methods from VectorStore...
}
