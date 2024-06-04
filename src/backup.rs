use std::io::Error;

use crate::environment::Args;
use crate::models::{
    GlacierFile,
    LocalFile,
};

use crate::s3;

use aws_smithy_runtime_api::http::Response;
use aws_sdk_s3::{
    error::SdkError,
    operation::put_object::{
        PutObjectError,
        PutObjectOutput
    }, 
    Client as S3Client
};
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
    stream::FuturesOrdered, Future, StreamExt
};

pub async fn fix_pending_uploads(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let pending_upload_files: Vec<GlacierFile> = get_pending_upload_files(conn);
    let length = pending_upload_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in pending_upload_files {
        futures.push_back(s3::upsert(s3_client, args.bucket_name.clone(), file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };

    let failures: usize = complete_upsert(conn, futures, &mut files).await;

    (length - failures, failures)
}

pub async fn fix_pending_updates(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let pending_update_files: Vec<GlacierFile> = get_pending_update_files(conn);
    let length = pending_update_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in pending_update_files {
        futures.push_back(s3::upsert(s3_client, args.bucket_name.clone(), file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };

    let failures: usize = complete_upsert(conn, futures, &mut files).await;

    (length - failures, failures)
}

pub async fn fix_pending_deletes(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let pending_delete_files: Vec<GlacierFile> = get_pending_delete_files(conn);
    let length = pending_delete_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in pending_delete_files {
        futures.push_back(s3::delete(s3_client, args.bucket_name.clone(), file.file_path.clone()));
        files.push(file);
    };
    
    let failures: usize = complete_delete(conn, futures, &mut files).await;

    (length - failures, failures)
}

pub async fn upload_new_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let new_files: Vec<LocalFile> = get_new_files(conn);
    let length = new_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in new_files {
        // Copy from local_state to glacier state, leaving uploaded null.
        let file = GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.insert(conn);

        // Upload to glacier.
        futures.push_back(s3::upsert(s3_client, args.bucket_name.clone(), file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };

    let failures: usize = complete_upsert(conn, futures, &mut files).await;

    (length - failures, failures)
}

pub async fn update_changed_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let updated_files: Vec<LocalFile> = get_changed_files(conn);
    let length = updated_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for file in updated_files {
        // Copy from local_state to glacier state, leaving uploaded as it was.
        let file = GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.update(conn);

        futures.push_back(s3::upsert(s3_client, args.bucket_name.clone(), file.file_path.clone(), file.file_path.clone()));
        files.push(file);
    };
    
    let failures: usize = complete_upsert(conn, futures, &mut files).await;

    (length - failures, failures)
}

pub async fn delete_missing_files(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) -> (usize, usize) {
    let deleted_files: Vec<GlacierFile> = get_missing_files(conn);
    let length = deleted_files.len();
    let mut futures = FuturesOrdered::new();
    let mut files = Vec::with_capacity(length);

    if args.dry_run {
        return (length, 0)
    }

    for mut file in deleted_files {
        // Set pending_delete to TRUE.
        file.pending_delete = true;
        file.update(conn);

        futures.push_back(s3::delete(s3_client, args.bucket_name.clone(), file.file_path.clone()));
        files.push(file);
    };
    
    let failures: usize = complete_delete(conn, futures, &mut files).await;

    (length - failures, failures)
}

async fn complete_upsert<F>(conn: &mut PgConnection, futures: FuturesOrdered<F>, files: &mut Vec<GlacierFile>) -> usize
    where F: Future<Output = Result<PutObjectOutput, SdkError<PutObjectError, Response>>> {
        
    let mut failures = 0;
    let length = futures.len();
    let results: Vec<_> = futures.collect().await;

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

async fn complete_delete<F>(conn: &mut PgConnection, futures: FuturesOrdered<F>, files: &mut Vec<GlacierFile>) -> usize
    where F: Future<Output = Result<(), Error>> {

    let mut failures = 0;
    let length = futures.len();
    let results: Vec<_> = futures.collect().await;

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