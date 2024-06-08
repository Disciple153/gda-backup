use crate::dynamodb::HashTracker;
use crate::environment::Args;
use crate::models::GlacierFile;
use log::error;

use crate::s3;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::Client as S3Client;
use diesel::prelude::PgConnection;

/// The function `db_from_s3` asynchronously retrieves objects from S3 and DynamoDB,
/// and inserts files into a local database.
/// 
/// Arguments:
/// 
/// * `args`: The `args` parameter in the `db_from_s3` function represents the
/// arguments needed for the function to operate. These arguments could include
/// configuration settings, file paths, bucket names, table names, and other
/// parameters required for interacting with Amazon S3, DynamoDB, and the local
/// database. The
/// * `conn`: The `conn` parameter in the function `db_from_s3` is a mutable
/// reference to a `PgConnection`, which is likely a connection to a PostgreSQL
/// database. This connection is used to interact with the local database where
/// files are being inserted.
/// * `s3_client`: The `s3_client` parameter in your function `db_from_s3` is a
/// reference to an S3 client object that is used to interact with Amazon S3
/// service. This client is likely responsible for performing operations such as
/// listing objects in an S3 bucket.
/// * `dynamo_client`: The `dynamo_client` parameter in the function `db_from_s3` is
/// of type `&DynamoDbClient`, which is likely a client for interacting with
/// DynamoDB, a NoSQL database service provided by AWS. This client would be used to
/// perform operations such as querying, inserting,
/// 
/// Returns:
/// 
/// The function `db_from_s3` returns an `Option<()>`.
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
            let result = GlacierFile {
                file_path: file.to_string(),
                file_hash: Some(hash_tracker.hash.clone()),
                modified: *modified,
            }.insert(conn);

            match result {
                Ok(_) => (),
                Err(error) => error!("Failed to load file into local database from DynamoDB and S3: {:?}\n Error: {}", file, error),
            };

            Some(())
        });

        Some(())
    });

    Some(())
}