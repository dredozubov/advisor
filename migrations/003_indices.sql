-- Add migration script here
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

-- Create indices for conversations table
CREATE INDEX IF NOT EXISTS idx_conversations_created_at 
    ON conversations(created_at);
CREATE INDEX IF NOT EXISTS idx_conversations_updated_at 
    ON conversations(updated_at);

-- Create indices for conversation_messages table
CREATE INDEX IF NOT EXISTS idx_conversation_messages_conversation_id
    ON conversation_messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_conversation_messages_created_at
    ON conversation_messages(created_at);

-- Create indices for vector store
CREATE INDEX IF NOT EXISTS idx_vs_embeddings_collection_id
    ON vs_embeddings(collection_id);

-- Create vector similarity index
CREATE INDEX IF NOT EXISTS idx_vs_embeddings_embedding
    ON vs_embeddings 
    USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);
