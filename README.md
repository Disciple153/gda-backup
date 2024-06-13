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
      BUCKET_NAME: my-bucket
      DYNAMO_TABLE: my-table
      AWS_ACCESS_KEY_ID: AKIAIOSFODNN7EXAMPLE
      AWS_SECRET_ACCESS_KEY: wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
      AWS_DEFAULT_REGION: us-east-1
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
```

### Environment variables

| Variable               | Required | Default    | Description                                                                                             |
| ---------------------- | -------- | ---------- | ------------------------------------------------------------------------------------------------------- |
| BACKUP_CRON            | yes      |            | A UTC cron expression that defines when backups will run.                                               |
| TARGET_DIR:            | no       | "/backup"  | The directory targeted by automatic backups.                                                            |
| FILTER:                | no       |            | A regular expression used to filter files out of backups.                                               |
| FILTER_DELIMITER:      | no       |            | A delimiter that if supplied, can be used to split "FILTER" into multiple regex strings.                |
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

### Unscheduled Backup

To perform a backup outside of the cron job, run the following command:

```bash
docker exec gda_backup gda_backup backup
```

### Restore

To restore your backups to a file, run the following command:

```bash
docker exec gda_backup gda_backup restore \
    --target-dir "/restore" \
    --bucket-name "my-bucket" \
    --dynamo-table "my-table"

```

| Note: Restoring files from any tier of S3 Glacier comes with an additional cost. To minimize mistakes and charges, it is recommended that you use the AWS CLI to restore your archive to a regular S3 bucket before restoring your files.

### Terraform

If you are using terraform, you can deploy gda_backup and all required AWS resources using the provided [terraform stack](./gda-backup.tf).

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

## AWS Resources

GDA Backup requires an S3 bucket and a DynamoDB table to operate as well as an IAM Role to access those resources.

### S3 Bucket

The bucket you create should have these settings:
- Versioning: enabled
  - This ensures that objects are not overwritten before the minimum storage duration has elapsed.
- Lifecycle_policies
  - Move objects to a cheaper storage tier.
  - Delete non-current objects.
    - Make sure that `noncurrent days` is set to a value greater than the minimum storage duration for the storage class you are using. (For Glacier Deep archive, this is 180 days)

### DynamoDB table

The DynamoDB table you create should have these settings:
- Hash value:
  - Name: `hash`
  - Type: `S` 
- Billing mode:
  - `PAY_PER_REQUEST` aka Serverless
- Table class
  - Standard is recommended for the initial backup.
  - Switch to Standard-IA after the initial backup is complete.

### IAM Role

This role which will enables all features of GDA Backup.

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "S3Actions",
      "Effect": "Allow",
      "Action": [
        "s3:DeleteObject",
        "s3:DeleteObjectVersion",
        "s3:GetObject",
        "s3:ListBucket",
        "s3:ListBucketVersions",
        "s3:PutObject",
        "s3:RestoreObject"
      ],
      "Resource": ["arn:aws:s3:::my-bucket/*", "arn:aws:s3:::my-bucket"]
    },
    {
      "Sid": "DynamoDbActions",
      "Effect": "Allow",
      "Action": [
        "dynamodb:DeleteItem",
        "dynamodb:GetItem",
        "dynamodb:PutItem",
        "dynamodb:Scan"
      ],
      "Resource": [
        "arn:aws:dynamodb:us-east-1:387145356314:table/my-table",
        "arn:aws:dynamodb:us-east-1:387145356314:table/my-table/index/hash"
      ]
    }
  ]
}
```

This role which will only enables the backup feature.

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "S3Actions",
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:RestoreObject"
      ],
      "Resource": ["arn:aws:s3:::my-bucket/*", "arn:aws:s3:::my-bucket"]
    },
    {
      "Sid": "DynamoDbActions",
      "Effect": "Allow",
      "Action": [
        "dynamodb:DeleteItem",
        "dynamodb:GetItem",
        "dynamodb:PutItem",
      ],
      "Resource": [
        "arn:aws:dynamodb:us-east-1:387145356314:table/my-table",
        "arn:aws:dynamodb:us-east-1:387145356314:table/my-table/index/hash"
      ]
    }
  ]
}