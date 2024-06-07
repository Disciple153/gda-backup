use crate::dynamodb::HashTracker;
use crate::environment::Args;
use crate::models::GlacierFile;

use crate::s3;
use aws_sdk_dynamodb::error::SdkError as DynamoDbSdkError;
use aws_sdk_dynamodb::operation::scan::ScanError;
use aws_sdk_s3::error::SdkError as S3SdkError;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::Client as S3Client;
use aws_smithy_runtime_api::http::Response;
use diesel::prelude::PgConnection;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RestoreError {

    #[error("DynamoDbSdkErrorScan")]
    DynamoDbSdkErrorScan(#[from] DynamoDbSdkError<ScanError, Response>),

    #[error("S3SdkErrorPut")]
    DynamoDbSdkErrorPut(#[from] S3SdkError<ListObjectsV2Error>),
}

pub async fn db_from_s3(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoDbClient) -> Result<(), RestoreError> {
    
    if args.dry_run {
        return Ok(())
    }

    // Get all objects in S3
    let modified_times = s3::list(&s3_client, args.bucket_name.clone()).await?;
    
    // Get all objects in DynamoDB
    let hash_trackers = HashTracker::get_all(dynamo_client, args.dynamo_table.clone()).await?;

    // For every object in DynamoDB
    let _ = hash_trackers.iter().map(|hash_tracker| {

        let modified = modified_times.get(&hash_tracker.hash.clone())?;

        // For every local file referenced by the DynamoDB object
        let _ = hash_tracker.files().map(|file| {

            // Insert the file into the local database
            GlacierFile {
                file_path: file.to_string(),
                file_hash: Some(hash_tracker.hash.clone()),
                modified: *modified,
                uploaded: Some(*modified),
                pending_delete: false,
            }.insert(conn);

            Some(())
        });

        Some(())
    });

    Ok(())
}