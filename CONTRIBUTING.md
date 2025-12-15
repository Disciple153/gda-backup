# Contributing to GDA Backup

## Prerequisites

- [Rust/Cargo](https://rustup.rs/)
- [Diesel CLI](https://diesel.rs/guides/getting-started)
- Docker and Docker Compose
- PostgreSQL (for local development)

## Environment Setup

### 1. Install Rust and Cargo

```bash
curl https://sh.rustup.rs -sSf | sh
source ~/.cargo/env
```

### 2. Install Diesel CLI

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/diesel-rs/diesel/releases/download/v2.2.0/diesel_cli-installer.sh | sh
```

### 3. Set up PostgreSQL Database

#### Using Docker (Recommended)

```bash
docker run --name gda-postgres -e POSTGRES_PASSWORD=password -p 5432:5432 -d postgres:latest
```

#### Using Docker Compose (for tests)

```bash
cd tests
docker-compose up -d
```

### 4. Initialize Database

```bash
diesel migration run --database-url postgres://postgres:password@localhost/postgres
```

### 5. Environment Variables

Copy the test environment file:

```bash
cp tests/.env .env
```

For AWS integration tests, add your AWS credentials to `.env`:

```bash
AWS_ACCESS_KEY_ID=your_access_key
AWS_SECRET_ACCESS_KEY=your_secret_key
AWS_DEFAULT_REGION=us-east-1
BUCKET_NAME=your-test-bucket
DYNAMO_TABLE=your-test-table
```

## Building

### Development Build

```bash
cargo build
```

### Release Build

```bash
cargo build --release
```

The optimized binary will be in `target/release/gda_backup`.

### Docker Build

```bash
docker build -t gda-backup:local .
```

## Testing

### Unit Tests

```bash
cargo test
```

### Integration Tests

Requires PostgreSQL running and AWS credentials configured:

```bash
cargo test --test integration_test
```

### Docker Integration Test

```bash
# Create test files
mkdir -p backup_test
echo "test content" > backup_test/test.txt

# Run full docker test
docker-compose up -d
sleep 90  # Wait for backup to trigger
docker logs gda-backup-gda_backup-1
docker-compose down
```

## Development Workflow

1. **Make changes** to the source code
2. **Run tests** to ensure functionality works:
   ```bash
   cargo test
   ```
3. **Check formatting**:
   ```bash
   cargo fmt --check
   ```
4. **Run clippy** for linting:
   ```bash
   cargo clippy -- -D warnings
   ```
5. **Test Docker build**:
   ```bash
   docker build -t gda-backup:test .
   ```

## Project Structure

- `src/` - Main source code
- `migrations/` - Database migrations
- `tests/` - Integration tests
- `docker/` - Docker-related scripts
- `.github/workflows/` - CI/CD configuration

## Database Migrations

### Create New Migration

```bash
diesel migration generate migration_name
```

### Run Migrations

```bash
diesel migration run
```

### Revert Migration

```bash
diesel migration revert
```

## Release Process

Releases are automated via GitHub Actions when pushing to main branch. The workflow:

1. Runs tests with PostgreSQL
2. Builds release binary
3. Creates Docker image
4. Publishes to GitHub Container Registry

## Troubleshooting

### Database Connection Issues

Ensure PostgreSQL is running and accessible:

```bash
psql postgres://postgres:password@localhost/postgres -c "SELECT 1;"
```

### Diesel Schema Issues

Regenerate schema after database changes:

```bash
diesel print-schema > src/schema.rs
```

### Docker Permission Issues

On Linux, you may need to add your user to the docker group:

```bash
sudo usermod -aG docker $USER
```