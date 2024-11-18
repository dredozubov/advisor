# Financial Document Advisor

An AI-powered financial document analysis tool that helps analyze SEC filings and earnings transcripts.

## Prerequisites

- Docker and Docker Compose
- OpenAI API key

## Quick Start with Docker Compose

1. Create a `.env` file in the project root:

```env
OPENAI_KEY=your_openai_api_key_here
```

2. Start the services using Docker Compose:

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
