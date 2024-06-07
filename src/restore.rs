use crate::dynamodb::HashTracker;
use crate::environment::Args;
use crate::models::GlacierFile;

use crate::s3;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::Client as S3Client;
use diesel::prelude::PgConnection;

pub async fn db_from_s3(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoDbClient) -> Option<()> {
    
    if args.dry_run {
        return Some(())
    }

    // Get all objects in S3
    let modified_times = s3::list(&s3_client, args.bucket_name.clone()).await.ok()?;
    
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
            }.insert(conn);

            Some(())
        });

        Some(())
    });

    Some(())
}