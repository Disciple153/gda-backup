#! /bin/bash

cargo run -- \
    -t "/home/disciple153/documents/gda-backup/backup_test" \
    -b "disciple153-test" \
    -d "gda-backup-test" \
    -e "postgres" \
    -u "username" \
    -p "password" \
    -a "localhost" \
    -n "backup_state" \
    -m 1 \
    --dry-run