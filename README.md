# Financial Document Advisor

An AI-powered financial document analysis tool that helps analyze SEC filings and earnings transcripts.

## Development Setup

### Prerequisites
- Docker and Docker Compose
- VS Code with Remote Containers extension
- API keys for:
  - OpenAI (OPENAI_KEY and OPENAI_API_KEY)
  - Anthropic (ANTHROPIC_API_KEY)
  - Groq (GROQ_API_KEY)

### Quick Start with VS Code DevContainer

1. **Clone and Setup**:
   ```bash
   git clone https://github.com/yourusername/advisor.git
   cd advisor
   ```

2. **Configure API Keys**:
   Create a `.env` file in the project root:
   ```bash
   ANTHROPIC_API_KEY=your_anthropic_key_here
   OPENAI_API_KEY=your_openai_key_here
   OPENAI_KEY=your_openai_key_here
   GROQ_API_KEY=your_groq_key_here
   ```

3. **Open in VS Code**:
   ```bash
   code .
   ```
   Then press F1 and select "Remote-Containers: Reopen in Container"

### Alternative: Docker Compose Setup

1. **Start Services**:
   ```bash
   make up
   # or
   docker-compose up -d
   ```

2. **View Logs**:
   ```bash
   make logs
   # or
   docker-compose logs -f
   ```

3. **Access Shell**:
   ```bash
   make shell
   # or
   docker-compose exec app /bin/bash
   ```

### Development Commands

```bash
# Build all services
make build

# Run tests
make test

# Watch for changes and rebuild
make watch

# Clean up all containers and volumes
make clean
```

## Project Structure

```
.
├── .devcontainer/          # VS Code DevContainer configuration
├── .docker/               # Docker-related files
├── migrations/           # Database migrations
├── scripts/             # Utility scripts
├── src/                 # Source code
└── tests/              # Test files
```

## Database Setup

The project uses PostgreSQL with pgvector for vector similarity search. The database is automatically initialized with:
- Required extensions (vector, uuid-ossp)
- Optimized indices for conversations, embeddings, and documents
- Vector similarity search capabilities

### Database Migrations

Migrations are handled automatically when starting the container. To manually run migrations:
```bash
sqlx migrate run
```

To revert migrations:
```bash
sqlx migrate revert
```

## Security Notes

- All sensitive data (API keys, database passwords) is managed via environment variables or Docker secrets
- No credentials are stored in Docker/compose files
- Development container uses non-root user (vscode)
- Production container uses non-root user (advisor)
- Database access is restricted to container network
- Volumes are properly permissioned

## Container Features

### Development Container
- Full Rust development environment
- Hot reload with cargo-watch
- Integrated debugging support
- AI assistance tools (aider)
- Git integration
- VS Code extensions pre-configured

### Production Container
- Multi-stage build for minimal image size
- Only runtime dependencies included
- Health checks configured
- Non-root user
- Proper security settings

## Resource Management

- Container resource limits configured
- Volume mounts optimized for performance
- Cargo caching implemented
- Target directory caching
- Proper Docker layer caching

## Troubleshooting

1. **Database Connection Issues**:
   ```bash
   # Check database status
   docker-compose ps db
   # View database logs
   docker-compose logs db
   ```

2. **Rebuild Development Environment**:
   ```bash
   # Full rebuild
   docker-compose down
   docker-compose build --no-cache
   docker-compose up -d
   ```

3. **Reset Development Environment**:
   ```bash
   # Remove all containers and volumes
   make clean
   # or
   docker-compose down -v
   ```

4. **VS Code DevContainer Issues**:
   - Try "Remote-Containers: Rebuild Container"
   - Check `.devcontainer/devcontainer.json` for configuration
   - Verify API keys in `.env` file

## Contributing

1. Fork the repository
2. Create your feature branch
3. Commit your changes
4. Push to the branch
5. Create a new Pull Request

## License

[Add your license information here]
