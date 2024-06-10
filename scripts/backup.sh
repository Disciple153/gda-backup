#! /bin/bash

cargo run -- --debug --dry-run \
    backup \
    -t "/home/disciple153/documents/gda-backup/backup_test" \
    -b "disciple153-test" \
    -d "gda-backup-test" \
    -e "postgres" \
    -u "username" \
    -p "password" \
    -a "localhost" \
    -n "backup_state" \
    -f "test1" \
    -f "filter2" \
    -m 1