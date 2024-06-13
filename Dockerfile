FROM rust:latest
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

RUN apt-get install -y cron

# RUN apk update
# RUN apk add busybox-openrc

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
ENV PATH="${PATH}:/gda_backup/target/release"
ENV FILTER_DELIMITER=":"

# Start
CMD ["./docker/start.sh"]