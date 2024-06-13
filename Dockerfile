## Build gda_backup
FROM rust:latest as builder
LABEL maintainer="c_eps@yahoo.com"

# Install dependencies
RUN apt-get update
RUN curl --proto '=https' --tlsv1.2 -LsSf https://github.com/diesel-rs/diesel/releases/download/v2.2.0/diesel_cli-installer.sh | sh

# Copy gda_backup
WORKDIR "/gda_backup"
COPY Cargo.toml Cargo.toml
COPY diesel.toml diesel.toml
COPY migrations migrations
COPY src src

# Build gda_backup
RUN cargo build --release

## Build Minimal Container
FROM rust:latest
WORKDIR "/gda_backup"
COPY --from=builder /gda_backup/target/release/gda_backup /gda_backup/gda_backup
COPY diesel.toml diesel.toml
COPY migrations migrations

# Install dependencies
RUN apt-get update
RUN apt-get install -y cron
RUN curl --proto '=https' --tlsv1.2 -LsSf https://github.com/diesel-rs/diesel/releases/download/v2.2.0/diesel_cli-installer.sh | sh

# Copy other files
COPY LICENSE LICENSE
COPY README.md README.md
COPY docker docker
RUN chmod +x ./docker/*.sh

# Set enfironment variables
ENV DB_ENGINE=postgres
ENV POSTGRES_HOST=database
ENV POSTGRES_USER=postgres
ENV POSTGRES_DB=postgres
ENV TARGET_DIR="/backup"
ENV PATH="${PATH}:/gda_backup"

# Start
CMD ["./docker/start.sh"]