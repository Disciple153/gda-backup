use std::collections::HashMap;
use std::path::Path;
use futures::future;

use crate::dynamodb::HashTracker;
use crate::environment::Args;
use crate::models::GlacierFile;

use crate::s3;

use aws_sdk_s3::Client as S3Client;
use aws_sdk_dynamodb::Client as DynamoClient;
use chrono::{
    DateTime,
    Duration,
    Utc,
};
use diesel::prelude::PgConnection;

use crate::{
    get_glacier_file,
    get_changed_files,
    get_missing_files,
    get_new_files,
};

use checksums::hash_file;

// Use BLAKE2B if running on 64 bit CPU
#[cfg(target_pointer_width = "64")]
use checksums::Algorithm::BLAKE2B as HASH_ALGO;

// Use BLAKE2S if running on 32 bit CPU or lower
#[cfg(not(target_pointer_width = "64"))]
use checksums::Algorithm::BLAKE2S as HASH_ALGO;

fn new_expiration(min_storage_duration: i64) -> DateTime<Utc> {
    match Utc::now().checked_add_signed(Duration::days(min_storage_duration)) {
        Some(e) => e,
        None => DateTime::UNIX_EPOCH,
    }
}

async fn get_old_hash_tracker(args: &Args, s3_client: &S3Client, dynamo_client: &DynamoClient, file: GlacierFile) -> Option<HashTracker> {
    let mut hash_tracker = HashTracker::get(dynamo_client, args.dynamo_table.clone(), file.file_hash.clone()?).await?;
    hash_tracker.del_file_name(file.file_path);

    if !hash_tracker.has_files() {
        let _ = s3::delete(s3_client, args.bucket_name.clone(), file.file_hash?);
    };

    Some(hash_tracker)
}

struct FileChange {
    g_file: GlacierFile,
    old_hash: Option<String>,
}

struct HashTrackerChange {
    new: HashTracker,
    old: HashTracker,
    created_files: Vec<GlacierFile>,
    deleted_files: Vec<GlacierFile>,
    failed: bool,
}

impl HashTrackerChange {
    fn changed(&self) -> bool {
        self.new != self.old
    }
}


async fn backup(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> Option<()> {

    // Get all changes
    let file_changes: Vec<FileChange> = 
        get_new_files(conn).iter().flat_map(|l_file| { 
            let g_file = GlacierFile {
                file_path: l_file.file_path.clone(),
                file_hash: Some(hash_file(Path::new(&l_file.file_path), HASH_ALGO)),
                modified: l_file.modified,
            };

            Some(FileChange {
                g_file,
                old_hash: None
            })
        })
        
        .chain(get_missing_files(conn).iter_mut().flat_map(|g_file| {
            let old_hash = g_file.file_hash.clone();

            g_file.file_hash = None;

            Some(FileChange {
                g_file: g_file.to_owned(),
                old_hash,
            })
        }))

        .chain(get_changed_files(conn).iter().flat_map(|l_file| { 
            let mut g_file = get_glacier_file(conn, l_file.file_path.clone()).ok()?; // TODO do this in the get_changed_files query
            let old_hash = g_file.file_hash;

            g_file.file_hash = Some(hash_file(Path::new(&l_file.file_path), HASH_ALGO));
            g_file.modified = l_file.modified;

            Some(FileChange {
                g_file,
                old_hash,
            })
        }))
        
    .collect();

    // Get HashTrackers for all changes and update them to reflect the current state
    let mut hash_tracker_changes: HashMap<String, HashTrackerChange> = HashMap::new();
    for file_change in file_changes {

        // If a file version was created 
        if let Some(hash) = file_change.g_file.file_hash.clone() { 
            let h_t_c = get_hash_tracker_change(args,  dynamo_client, &mut hash_tracker_changes, hash).await;
            h_t_c.new.add_file_name(file_change.g_file.file_path.clone());
            h_t_c.created_files.push(file_change.g_file.clone());
        };

        // If a file version was deleted 
        if let Some(hash) = file_change.old_hash {
            let h_t_c = get_hash_tracker_change(args,  dynamo_client, &mut hash_tracker_changes, hash).await;
            h_t_c.new.del_file_name(file_change.g_file.file_path.clone());
            h_t_c.deleted_files.push(file_change.g_file.clone());
        };
    };

    // Publish S3 changes
    let hash_tracker_changes: Vec<&mut HashTrackerChange> = future::join_all(hash_tracker_changes.iter_mut().map(|(hash, hash_tracker_change)| async {
        
        if hash_tracker_change.changed() {

            // Delete
            if hash_tracker_change.old.has_files() {
                if !hash_tracker_change.new.has_files() {
                    // TODO delete
                    s3::delete(s3_client, args.bucket_name.clone(), hash.clone()).await.ok()?;
                }
            }

            // Put
            else if hash_tracker_change.old.is_expired() {
                if hash_tracker_change.new.has_files() {
                    // TODO put
                    let file_path = &hash_tracker_change.created_files.first()?.file_path;

                    s3::put(s3_client, args.bucket_name.clone(), hash.clone(), file_path.to_string()).await.ok()?;
                }
            }

            // Undelete
            else {
                if hash_tracker_change.new.has_files() {
                    s3::undelete(s3_client, args.bucket_name.clone(), hash.clone()).await.ok()?;
                    hash_tracker_change.new.expiration = new_expiration(args.min_storage_duration.clone());
                }
            }
        }

        Some(hash_tracker_change)
    })).await.into_iter().flatten().collect();

    // Publish HashTrackers 
    let hash_tracker_changes: Vec<&mut HashTrackerChange> = future::join_all(hash_tracker_changes.into_iter().map(|hash_tracker_change| async {
        if hash_tracker_change.changed() {
            hash_tracker_change.new.update(dynamo_client, args.dynamo_table.clone()).await.ok()?;
        }

        Some(hash_tracker_change)
    })).await.into_iter().flatten().collect();

    // Publish GlacierFiles
    for hash_tracker_change in hash_tracker_changes {
        for d_file in &hash_tracker_change.deleted_files {
            d_file.delete(conn);
        }

        for d_file in &hash_tracker_change.created_files {
            d_file.delete(conn);
        }
    };

    Some(())
}

async fn get_hash_tracker_change<'a>(args: &Args, dynamo_client: &DynamoClient, hash_tracker_changes: &'a mut HashMap<String, HashTrackerChange>, hash: String) -> &'a mut HashTrackerChange {
    if !hash_tracker_changes.contains_key(&hash) {
        let hash_tracker = HashTracker::get(dynamo_client, args.dynamo_table.clone(), hash.clone()).await
            .unwrap_or(HashTracker::new(hash.clone()));

        hash_tracker_changes.insert(
            hash.clone(),
            HashTrackerChange {
                new: hash_tracker.clone(),
                old: hash_tracker,
                created_files: vec![],
                deleted_files: vec![],
                failed: false,
            }
        );
    }

    hash_tracker_changes.get_mut(&hash).unwrap()
}