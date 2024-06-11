#! /bin/bash

#set -e

export DATABASE_URL=$DB_ENGINE://$POSTGRES_USER:$POSTGRES_PASSWORD@$POSTGRES_HOST/$POSTGRES_DB
diesel migration run

echo "$CRON gda_backup backup-with-env gda_backup_config" | crontab - 
cron
tail -f /dev/null
