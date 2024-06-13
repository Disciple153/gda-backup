#! /bin/bash

# Exit on error
set -e

# Set environment variables and expose them for cron
export DATABASE_URL=$DB_ENGINE://$POSTGRES_USER:$POSTGRES_PASSWORD@$POSTGRES_HOST/$POSTGRES_DB
printenv | grep -v "no_proxy" >> /etc/environment

# Initialize the database
echo Waiting for database...
sleep 5
diesel migration run

# Configure cron job for periodic backups
touch /var/log/docker.log
echo Loading cron: "$BACKUP_CRON"
echo "$BACKUP_CRON /gda_backup/docker/backup.sh" | crontab -
cron

# Watch the output
tail -F /var/log/docker.log
