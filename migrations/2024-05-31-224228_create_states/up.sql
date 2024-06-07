-- Your SQL goes here
CREATE TABLE glacier_state (
  file_path TEXT PRIMARY KEY,
  file_hash TEXT,
  modified TIMESTAMP NOT NULL
);

CREATE TABLE local_state (
  file_path TEXT PRIMARY KEY,
  --file_hash BYTEA NOT NULL,
  modified TIMESTAMP NOT NULL
);