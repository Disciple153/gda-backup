#! /bin/bash

cargo run -- --debug \
    -t "/home/disciple153/documents/gda-backup/backup_test" \
    -b "disciple153-test" \
    -d "gda-backup-test" \
    backup \
    -e "postgres" \
    -u "username" \
    -p "password" \
    -a "localhost" \
    -n "backup_state" \
    -m 1