pub mod memory;
use langchain_rust::vectorstore::VectorStore;
pub use memory::InMemoryStore;

#[async_trait::async_trait]
pub trait VectorStorage: VectorStore {
    fn is_ephemeral(&self) -> bool;
    // Other methods from VectorStore...
}
