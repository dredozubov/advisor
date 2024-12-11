-- Add migration script here
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Create indices for conversations table
CREATE INDEX IF NOT EXISTS idx_conversations_created_at 
    ON conversations(created_at);
CREATE INDEX IF NOT EXISTS idx_conversations_updated_at 
    ON conversations(updated_at);

-- Create indices for vector store
CREATE INDEX IF NOT EXISTS idx_embeddings_collection 
    ON vs_embeddings(collections);
CREATE INDEX IF NOT EXISTS idx_embeddings_created_at 
    ON vs_embeddings(created_at);

-- Create GiST index for vector similarity search
CREATE INDEX IF NOT EXISTS idx_vs_embeddings_vector 
    ON vs_embeddings 
    USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);
