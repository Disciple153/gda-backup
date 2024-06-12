#! /bin/bash
docker compose down
docker volume rm gda-backup_postgres_volume
docker build -t disciple153/gda-backup:0.0.1 .
docker compose up -d
sleep 65
docker logs gda-backup-gda_backup-1
