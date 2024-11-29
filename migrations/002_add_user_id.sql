ALTER TABLE conversations 
ADD COLUMN user_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';

-- Add index for common queries that will filter by user_id
CREATE INDEX idx_conversations_user_id_updated_at 
ON conversations(user_id, updated_at DESC);

-- Add index for looking up conversations by both user and id
CREATE INDEX idx_conversations_user_id_id 
ON conversations(user_id, id);

-- Drop old index as it's superseded by the compound index
DROP INDEX idx_conversations_updated_at;
