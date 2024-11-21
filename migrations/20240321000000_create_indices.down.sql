-- Drop indices in reverse order
DROP INDEX IF EXISTS idx_embeddings_vector;
DROP INDEX IF EXISTS idx_documents_report_type;
DROP INDEX IF EXISTS idx_documents_filing_date;
DROP INDEX IF EXISTS idx_documents_ticker;
DROP INDEX IF EXISTS idx_embeddings_created_at;
DROP INDEX IF EXISTS idx_embeddings_collection;
DROP INDEX IF EXISTS idx_conversations_updated_at;
DROP INDEX IF EXISTS idx_conversations_created_at;
