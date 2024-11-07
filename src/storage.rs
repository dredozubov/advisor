use anyhow::Result;
use langchain_rust::{
    embedding::openai::OpenAiEmbedder,
    schemas::Document,
    vectorstore::qdrant::{Qdrant, StoreBuilder},
    vectorstore::{VecStoreOptions, VectorStore},
};

pub struct Storage {
    store: Box<dyn VectorStore>,
}

impl Storage {
    pub async fn new(collection_name: &str) -> Result<Self> {
        let embedder = OpenAiEmbedder::default();
        let client = Qdrant::from_url("http://localhost:6334").build()?;

        let store = StoreBuilder::new()
            .embedder(embedder)
            .client(client)
            .collection_name(collection_name)
            .build()
            .await?;

        Ok(Self {
            store: Box::new(store),
        })
    }

    pub async fn add_documents(&self, documents: Vec<Document>) -> Result<()> {
        self.store
            .add_documents(&documents, &VecStoreOptions::default())
            .await?;
        Ok(())
    }

    pub async fn similarity_search(&self, query: &str, limit: u32) -> Result<Vec<Document>> {
        let results = self
            .store
            .similarity_search(query, limit as usize, &VecStoreOptions::default())
            .await?;
        Ok(results)
    }
}
