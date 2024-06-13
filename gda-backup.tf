# Providers

terraform {
  backend "s3" {}
  required_providers {
    docker = {
      source  = "kreuzwerker/docker"
      version = "2.23.1"
    }
    random = {
      source  = "hashicorp/random"
      version = "> 2.0"
    }
  }
}

provider "docker" {
  host = "unix:///var/run/docker.sock"
}

# VARIABLES

# If you are using Glacier Deep Archive, set this to at least 180 to avoid early
# deletion fees.
variable "NONCURRENT_DAYS" {
  type    = number
  default = 182
}

variable "AWS_REGION" {
  type    = string
  default = "us-west-2"
}

# PASSWORD

resource "random_password" "gda_backup_db_password" {
  length  = 16
  special = true
}

# AWS RESOURCES

# S3
resource "aws_s3_bucket" "gda_backup_bucket" {
  bucket_prefix = "gda-backup"
}

resource "aws_s3_bucket_lifecycle_configuration" "gda_backup_lifecycle" {
  bucket = aws_s3_bucket.gda_backup_bucket.id

  # This rule is used to automatically transition objects to Glacier Deep 
  # Archive after one day. 
  rule {
    id     = "move_to_deep_archive"
    status = "Enabled"
    transition {
      days          = 1
      storage_class = "DEEP_ARCHIVE"
    }
  }

  # This rule is used to automatically delete old object versions from Glacier 
  # Deep Archive after var.NONCURRENT_DAYS days. 
  rule {
    id     = "delete_noncurrent_from_deep_archive"
    status = "Enabled"
    noncurrent_version_expiration {
      noncurrent_days = var.NONCURRENT_DAYS
    }
  }
}

# This must be enabled to avoid early deletion fees when using Glacier Deep 
# Archive.
resource "aws_s3_bucket_versioning" "gda_backup_versioning" {
  bucket = aws_s3_bucket.gda_backup_bucket.id
  versioning_configuration {
    status = "Enabled"
  }
}

# DynamoDB
resource "aws_dynamodb_table" "gda_backup_table" {
  name         = "gda-backup"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "hash"

  # gda_backup assumes that the hash attribute is named "hash".
  attribute {
    name = "hash"
    type = "S"
  }
}

# IAM

# This policy grants gda_backup least privlidges to perform all actions it is 
# capable of.
# If you would rather only allow gda_backup to backup your files, grant only
# these permissions:
# s3:DeleteObject, s3:PutObject, s3:RestoreObject, dynamodb:DeleteItem, 
# dynamodb:GetItem, dynamodb:PutItem
data "aws_iam_policy_document" "gda_backup_policy" {
  statement {
    effect = "Allow"
    actions = [
      "s3:DeleteObject",
      "s3:DeleteObjectVersion",
      "s3:GetObject",
      "s3:ListBucket",
      "s3:ListBucketVersions",
      "s3:PutObject",
      "s3:RestoreObject",
    ]
    resources = [
      aws_s3_bucket.gda_backup_bucket.arn,
      "${aws_s3_bucket.gda_backup_bucket.arn}/*",
    ]
  }
  statement {
    effect = "Allow"
    actions = [
      "dynamodb:DeleteItem",
      "dynamodb:GetItem",
      "dynamodb:PutItem",
      "dynamodb:Scan",
    ]
    resources = [
      aws_dynamodb_table.gda_backup_table.arn,
      "${aws_dynamodb_table.gda_backup_table.arn}/index/hash",
    ]
  }
}

resource "aws_iam_user" "gda_backup_user" {
  name = "gda_backup_user"
}

resource "aws_iam_user_policy" "gda_backup_user_policy" {
  user   = aws_iam_user.gda_backup_user.name
  policy = data.aws_iam_policy_document.gda_backup_policy.json
}

resource "aws_iam_access_key" "gda_backup_access_key" {
  user = aws_iam_user.gda_backup_user.name
}

# IMAGES

resource "docker_image" "gda_backup" {
  name = "ghcr.io/disciple153/gda-backup:latest"
}

resource "docker_image" "gda_backup_postgres" {
  name = "postgres:latest"
}

# VOLUMES

resource "docker_volume" "gda_backup_volume" {
  name = "gda_backup_volume"
}

# NETWORK

resource "docker_network" "gda_backup_network" {
  name = "gda_backup_network"
}

# CONTAINERS

resource "docker_container" "gda_backup" {
  name  = "gda_backup"
  image = docker_image.gda_backup.image_id
  env = [
    "BACKUP_CRON=0 4 * * 0",
    "BUCKET_NAME=${aws_s3_bucket.gda_backup_bucket.id}",
    "DYNAMO_TABLE=${aws_dynamodb_table.gda_backup_table.id}",
    "MIN_STORAGE_DURATION=${var.NONCURRENT_DAYS}",
    "POSTGRES_PASSWORD=${random_password.gda_backup_db_password.result}",
    "AWS_ACCESS_KEY_ID=${aws_iam_access_key.gda_backup_access_key.id}",
    "AWS_SECRET_ACCESS_KEY=${aws_iam_access_key.gda_backup_access_key.secret}",
    "AWS_DEFAULT_REGION=${var.AWS_REGION}",
  ]
  volumes {
    host_path      = "[path to be backed up]"
    container_path = "/backup"
  }
  restart = "always"
  networks_advanced {
    name = docker_network.gda_backup_network.id
  }
  depends_on = [
    docker_container.gda_backup_database,
  ]
}

resource "docker_container" "gda_backup_database" {
  name  = "database"
  image = docker_image.gda_backup_postgres.image_id
  env = [
    "POSTGRES_PASSWORD=${random_password.gda_backup_db_password.result}"
  ]
  volumes {
    volume_name    = docker_volume.gda_backup_volume.name
    container_path = "/var/lib/postgresql/data"
  }
  restart = "always"
  networks_advanced {
    name = docker_network.gda_backup_network.id
  }
}
