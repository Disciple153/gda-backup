use crate::environment::Args;
use crate::models::{
    GlacierFile,
    LocalFile,
};

use crate::s3;

use aws_sdk_s3::Client as S3Client;
use diesel::prelude::*;

use crate::{
    get_changed_files,
    get_missing_files,
    get_new_files,
    get_pending_delete_files,
    get_pending_upload_files,
    get_pending_update_files,
};

use futures::{
    stream::FuturesOrdered,
    StreamExt,
};

pub async fn fix_pending_uploads(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut pending_upload_files: Vec<GlacierFile> = get_pending_upload_files(conn);
    let length = pending_upload_files.len();

    if args.dry_run {
        return (length, 0)
    }

    let failures: usize = complete_upsert(args, conn, s3_client, &mut pending_upload_files).await;

    (length - failures, failures)
}

pub async fn fix_pending_updates(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut pending_update_files: Vec<GlacierFile> = get_pending_update_files(conn);
    let length = pending_update_files.len();

    if args.dry_run {
        return (length, 0)
    }

    let failures: usize = complete_upsert(args, conn, s3_client, &mut pending_update_files).await;

    (length - failures, failures)
}

pub async fn fix_pending_deletes(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut pending_delete_files: Vec<GlacierFile> = get_pending_delete_files(conn);
    let length = pending_delete_files.len();

    if args.dry_run {
        return (length, 0)
    }
    
    let failures: usize = complete_delete(args, conn, s3_client, &mut pending_delete_files).await;

    (length - failures, failures)
}

pub async fn upload_new_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let new_files: Vec<LocalFile> = get_new_files(conn);
    let length = new_files.len();
    let mut glacier_files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in new_files {
        // Copy from local_state to glacier state, leaving uploaded null.
        glacier_files.push(GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.insert(conn));
    };

    let failures: usize = complete_upsert(args, conn, s3_client, &mut glacier_files).await;

    (length - failures, failures)
}

pub async fn update_changed_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let updated_files: Vec<LocalFile> = get_changed_files(conn);
    let length = updated_files.len();
    let mut glacier_files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in updated_files {
        // Copy from local_state to glacier state, leaving uploaded as it was.
        glacier_files.push(GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.update(conn));
    };
    
    let failures: usize = complete_upsert(args, conn, s3_client, &mut glacier_files).await;

    (length - failures, failures)
}

pub async fn delete_missing_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let mut deleted_files: Vec<GlacierFile> = get_missing_files(conn);
    let length = deleted_files.len();

    if args.dry_run {
        return (length, 0)
    }

    for file in &mut *deleted_files {
        // Set pending_delete to TRUE.
        file.pending_delete = true;
        file.update(conn);
    };
    
    let failures: usize = complete_delete(args, conn, s3_client, &mut deleted_files).await;

    (length - failures, failures)
}

async fn complete_upsert(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, files: &mut Vec<GlacierFile>) -> usize {
        
    // Build S3 upsert jobs for every file to be upserted.
    let mut futures = FuturesOrdered::new();

    for file in &mut *files {
        futures.push_back(s3::upsert(s3_client, args.bucket_name.clone(), file.file_path.clone(), file.file_path.clone()))
    };

    let mut failures = 0;
    let length = futures.len();

    // Complete S3 upsert jobs.
    let results: Vec<_> = futures.collect().await;

    // Modify the database according to the results
    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                files[i].uploaded = Some(files[i].modified);
                files[i].update(conn);
            }
        }
    };

    failures
}

async fn complete_delete(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, files: &mut Vec<GlacierFile>) -> usize {

    // Build S3 delete jobs for every file to be deleted.
    let mut futures = FuturesOrdered::new();

    for file in &mut *files {
        futures.push_back(s3::delete(s3_client, args.bucket_name.clone(), file.file_path.clone()))
    };

    let mut failures = 0;
    let length = futures.len();

    // Complete S3 delete jobs.
    let results: Vec<_> = futures.collect().await;

    // Modify the database according to the results
    for i in 0..length {
        let result = &results[i];

        match result {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Delete from glacier_state
                files[i].delete(conn);
            }
        }
    };

    failures
}