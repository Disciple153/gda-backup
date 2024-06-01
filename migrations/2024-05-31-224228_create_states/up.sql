-- Your SQL goes here
CREATE TABLE glacier_state (
  file_path TEXT PRIMARY KEY,
  --file_hash BYTEA NOT NULL,
  modified TIMESTAMP NOT NULL,
  uploaded TIMESTAMP,
  pending_delete BOOL NOT NULL DEFAULT FALSE
);

CREATE TABLE local_state (
  file_path TEXT PRIMARY KEY,
  --file_hash BYTEA NOT NULL,
  modified TIMESTAMP NOT NULL
);