-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Create indices for conversations table
CREATE INDEX IF NOT EXISTS idx_conversations_created_at 
    ON conversations(created_at);
CREATE INDEX IF NOT EXISTS idx_conversations_updated_at 
    ON conversations(updated_at);

-- Create indices for vector store
CREATE INDEX IF NOT EXISTS idx_embeddings_collection 
    ON embeddings(collection);
CREATE INDEX IF NOT EXISTS idx_embeddings_created_at 
    ON embeddings(created_at);

-- Create indices for document metadata
CREATE INDEX IF NOT EXISTS idx_documents_ticker 
    ON documents(ticker);
CREATE INDEX IF NOT EXISTS idx_documents_filing_date 
    ON documents(filing_date);
CREATE INDEX IF NOT EXISTS idx_documents_report_type 
    ON documents(report_type);

-- Create GiST index for vector similarity search
CREATE INDEX IF NOT EXISTS idx_embeddings_vector 
    ON embeddings 
    USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);
