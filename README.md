# gda-backup

GDA Backup is a cloud backup solution which is optimized for AWS S3 Glacier Deep Archive in order to be the least expensive disaster recovery solution.
GDA Backup works by gathering all changed files, computing their hashes, and uploading only one object for every hash.
This enables minimum uploads to S3, and by storing metadata in DynamoDB, expensive describe and list API calls to S3 glacier are eliminated.

## Docker

The simplest way to use gda-backup is to run it in a docker container.

### Compose

```yml
services:
  gda_backup:
    image: disciple153/gda-backup:0.0.1
    environment:
      BACKUP_CRON: "* * * * *"
      POSTGRES_PASSWORD: password
      AWS_ACCESS_KEY_ID: AKIAIOSFODNN7EXAMPLE
      AWS_SECRET_ACCESS_KEY: wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
      AWS_DEFAULT_REGION: us-east-1
    configs:
      - gda_backup_config
    networks:
      - gda_backup_network
    depends_on:
      - database
    volumes:
      - ./backup_test:/backup:ro
      - ./restore_test:/restore

  database:
    image: "postgres:latest"
    environment:
      POSTGRES_PASSWORD: password
    volumes:
      - gda_backup_volume:/var/lib/postgresql/data
    networks:
      - gda_backup_network

volumes:
  gda_backup_volume:

networks:
  gda_backup_network:

configs:
  gda_backup_config:
    content: |
      dry_run: true
      log_level: debug
      target_dir: /backup
      bucket_name: my-bucket
      dynamo_table: my-table
      filters:
        - ".txt$"
      min_storage_duration: 1
```

### Environment variables

| Variable               | Required | Default    | Description                                                                                             |
| ---------------------- | -------- | ---------- | ------------------------------------------------------------------------------------------------------- |
| BACKUP_CRON            | yes      |            | A UTC cron expression that defines when backups will run.                                               |
| TARGET_DIR:            | no       | "/backup"  | The directory targeted by automatic backups.                                                            |
| FILTER:                | no       |            | A regular expression used to filter files out of backups.                                               |
| DRY_RUN:               | no       | false      | Set dry run to true to view the list of files that would be backed up without uploading anything.       |
| LOG_LEVEL:             | no       | "info"     | Set to "debug" for more verbose logs, or "quiet" to only display errors.                                |
| DB_ENGINE:             | no       | "postgres" | The engine of the local database. (Only postgres is supported.)                                         |
| POSTGRES_USER:         | no       | "postgres" | The username of the postgres database.                                                                  |
| POSTGRES_PASSWORD:     | yes      |            | The password to the postgres database.                                                                  |
| POSTGRES_HOST:         | no       | "database" | The hostname of the postgres database. This should be the name of the postgres container.               |
| POSTGRES_DB:           | no       | "postgres" | The name of the postgres database.                                                                      |
| MIN_STORAGE_DURATION:  | no       |            | The length of time after an object is created before it will be deleted by S3 lifecycle configurations. |
| BUCKET_NAME:           | yes      |            | The S3 bucket to which backups will be uploaded.                                                        |
| DYNAMO_TABLE:          | yes      |            | The DynamoDB table which will store backup related metadata.                                            |
| AWS_ACCESS_KEY_ID:     | yes      |            | The AWS access key id used to access S3 and DynamoDB.                                                   |
| AWS_SECRET_ACCESS_KEY: | yes      |            | The AWS secret access key used to access S3 and DynamoDB.                                               |
| AWS_DEFAULT_REGION:    | yes      |            | The AWS region containing your S3 bucket and DynamoDB table.                                            |

### gda_backup_config file

The gda_backup_config file is not required, and is largely redundant to environment variables, but it enables more than one filter to be set, and you may find it to be a little more convenient.

| Variable             | Required | Default    | Description                                                                                             |
| -------------------- | -------- | ---------- | ------------------------------------------------------------------------------------------------------- |
| target_dir           | no       | "/backup"  | The directory targeted by automatic backups.                                                            |
| filters              | no       |            | A list of regular expressions used to filter files out of backups.                                      |
| dry_run              | no       | false      | Set dry run to true to view the list of files that would be backed up without uploading anything.       |
| log_level            | no       | "info"     | Set to "debug" for more verbose logs, or "quiet" to only display errors.                                |
| db_engine            | no       | "postgres" | The engine of the local database. (Only postgres is supported.)                                         |
| postgres_user        | no       | "postgres" | The username of the postgres database.                                                                  |
| postgres_password    | yes      |            | The password to the postgres database.                                                                  |
| postgres_host        | no       | "database" | The hostname of the postgres database. This should be the name of the postgres container.               |
| postgres_db          | no       | "postgres" | The name of the postgres database.                                                                      |
| min_storage_duration | no       |            | The length of time after an object is created before it will be deleted by S3 lifecycle configurations. |
| bucket_name          | yes      |            | The S3 bucket to which backups will be uploaded.                                                        |
| dynamo_table         | yes      |            | The DynamoDB table which will store backup related metadata.                                            |

### Restore

To restore your backups to a file, run the following command:

```bash
docker exec gda-backup-gda_backup-1 gda_backup restore \
    --target-dir "/restore" \
    --bucket-name "my-bucket" \
    --dynamo-table "my-table"

```

| Note: Restoring files from any tier of S3 Glacier comes with an additional cost. To minimize mistakes and charges, it is recommended that you use the AWS CLI to restore your archive to a regular S3 bucket before restoring your files.

### Terraform

## Command line

To use this program from the command line, you must have Rust/Cargo, and Diesel installed, and must have a postgres database running.

To install Rust/Cargo:
```bash
curl https://sh.rustup.rs -sSf | sh
```

To install Diesel"
```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/diesel-rs/diesel/releases/download/v2.2.0/diesel_cli-installer.sh | sh
```

To start a postgres database with docker:
```bash
docker run --name database -e POSTGRES_PASSWORD=password -d
```

From the root of the project directory, run: 
```bash
diesel migration run
```

Next download and extract the latest release.

Finally you can run gda_backup:
```bash
# If you cloned the repository
cargo run -- help

# If you downloaded the folder from releases
gda_backup help
```

## AWS Role

Here is a role which will enable all features of GDA Backup.
You can remove permissions to ensure certain actions are not possible.

```json
{
	"Version": "2012-10-17",
	"Statement": [
		{
			"Sid": "S3Actions",
			"Effect": "Allow",
			"Action": [
				"s3:DeleteObject"
				"s3:DeleteObjectVersion",
				"s3:GetObject",
				"s3:ListBucket",
				"s3:ListBucketVersions",
				"s3:PutObject",
				"s3:RestoreObject",
			],
			"Resource": [
				"arn:aws:s3:::*/*",
				"arn:aws:s3:::my-bucket",
			]
		},
        		{
			"Sid": "DynamoDbActions",
			"Effect": "Allow",
			"Action": [
				"dynamodb:DeleteItem",
				"dynamodb:GetItem",
				"dynamodb:PutItem",
				"dynamodb:Scan",
			],
			"Resource": [
				"arn:aws:dynamodb:us-east-1:387145356314:table/my-table",
				"arn:aws:dynamodb:us-east-1:387145356314:table/my-table/index/hash"
			]
		}
	]
}
```