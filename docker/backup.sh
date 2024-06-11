#! /bin/bash

./target/release/gda_backup --debug --dry-run \
    backup \
    --target-dir "/backup" \
    --bucket-name "$h" \
    --dynamo-table "gda-backup-test" \
    --db-engine "postgres" \
    --db-user "username" \
    --db-password "password" \
    --db-host "localhost" \
    --db-name "backup_state" \
    --filter "test1" \
    --filter "filter2" \
    --min-storage-duration 1
