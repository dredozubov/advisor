# Financial Document Advisor

An AI-powered financial document analysis tool that helps analyze SEC filings and earnings transcripts.

## Installation

### Dockerized Setup (Recommended)

1. **Clone the repository**:
   ```bash
   git clone https://github.com/yourusername/financial-document-advisor.git
   cd financial-document-advisor
   ```

2. **Start all services using Docker Compose**:
   ```bash
   docker-compose up -d
   ```

   This will start:
   - The Advisor application
   - PostgreSQL with pgvector
   - Qdrant vector database

3. **Check the status of the services**:
   ```bash
   docker-compose ps
   ```

4. **View the logs**:
   ```bash
   docker-compose logs -f
   ```

5. **Stop the services**:
   ```bash
   docker-compose down
   ```

### Non-Dockerized Setup (Manual)

If you prefer to set up the environment manually, follow these steps:

#### Setting up PostgreSQL with pgvector on macOS

1. **Install PostgreSQL**:
   If you don't have PostgreSQL installed, you can install it using Homebrew:
   ```bash
   brew install postgresql
   ```

2. **Start PostgreSQL**:
   After installation, start the PostgreSQL service:
   ```bash
   brew services start postgresql
   ```

3. **Install pgvector**:
   Install the `pgvector` extension by running the following command:
   ```bash
   psql -d postgres -c "CREATE EXTENSION IF NOT EXISTS vector;"
   ```

4. **Create a new database**:
   Create a new database for your project:
   ```bash
   createdb advisor
   ```

5. **Verify pgvector installation**:
   Connect to the database and verify that `pgvector` is installed:
   ```bash
   psql -d advisor -c "\dx"
   ```

   You should see `pgvector` listed in the output.

6. **Set up the database URL**:
   Update your `.env` or environment variables with the following:
   ```bash
   DATABASE_URL=postgres://localhost/advisor
   ```

#### Running the Application

1. **Build the project**:
   ```bash
   cargo build --release
   ```

2. **Run the advisor with your OpenAI key**:
   ```bash
   OPENAI_KEY=your_openai_api_key_here cargo run --release
   ```

## Configuration

The default configuration uses:
- Qdrant running on `http://localhost:6334`
- GPT-4 for text generation
- OpenAI embeddings for document vectorization

You can modify these settings in the `docker-compose.yml` file.

## Troubleshooting

1. If Qdrant fails to start:
   ```bash
   docker-compose restart qdrant
   ```

2. If you need to rebuild the application:
   ```bash
   docker-compose build --no-cache advisor
   ```

3. To reset all data:
   ```bash
   docker-compose down -v
   ```
