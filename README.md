# gda-backup

GDA Backup is a cloud backup solution which is optimized for AWS S3 Glacier Deep Archive in order to be the least expensive disaster recovery solution.

## Docker

### Compose

### Restore

```bash
docker exec gda-backup-gda_backup-1 gda_backup restore \
    -t "/restore" \
    -b "disciple153-test" \
    -d "gda-backup-test"
    
```
