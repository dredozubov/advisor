# Financial Document Advisor

An AI-powered financial document analysis tool that helps analyze SEC filings and earnings transcripts.

## Installation

### Prerequisites

- Rust toolchain (install via [rustup](https://rustup.rs/))
- Docker (optional, for running Qdrant)
- OpenAI API key

### Local Installation

1. Clone the repository:
```bash
git clone https://github.com/yourusername/financial-document-advisor.git
cd financial-document-advisor
```

2. Build the project:
```bash
cargo build --release
```

### Running

1. Start Qdrant (choose one option):

   a. Using Docker:
   ```bash
   docker run -p 6334:6334 qdrant/qdrant
   ```
   
   b. Or using Docker Compose:
   ```bash
   docker-compose up -d qdrant
   ```

2. Run the advisor with your OpenAI key:
```bash
OPENAI_KEY=your_openai_api_key_here cargo run --release
```

## Quick Start with Docker Compose

1. Start all services using Docker Compose:

```bash
docker-compose up -d
```

This will start:
- The Advisor application
- Qdrant vector database

3. Check the status of the services:

```bash
docker-compose ps
```

4. View the logs:

```bash
docker-compose logs -f
```

5. Stop the services:

```bash
docker-compose down
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
