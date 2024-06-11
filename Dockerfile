FROM rust
LABEL maintainer="c_eps@yahoo.com"
RUN apt-get update
RUN apt-get install -y cron
RUN curl --proto '=https' --tlsv1.2 -LsSf https://github.com/diesel-rs/diesel/releases/download/v2.2.0/diesel_cli-installer.sh | sh
ENV DB_ENGINE=postgres
ENV POSTGRES_HOST=localhost
ENV POSTGRES_USER=postgres
ENV POSTGRES_DB=postgres
ENV TARGET_DIR="/backup"
WORKDIR "/gda_backup"
COPY ./ ./
RUN chmod +x ./docker/*.sh
RUN cargo build --release
ENV PATH="${PATH}:/gda_backup/target/release"
CMD ["./docker/start.sh"]