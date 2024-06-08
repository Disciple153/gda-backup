use std::collections::{HashMap, HashSet};
use std::path::Path;

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
        Some(time) => time,
        None => DateTime::UNIX_EPOCH,
    }
}

struct FileChange {
    g_file: GlacierFile,
    old_hash: Option<String>,
}

#[derive(Clone, Debug)]
struct HashTrackerChange {
    new: HashTracker,
    old: HashTracker,
    created_files: Vec<GlacierFile>,
    deleted_files: Vec<GlacierFile>,
}

impl HashTrackerChange {
    fn changed(&self) -> bool {
        self.new != self.old
    }
}


pub async fn backup(args: &Args, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {

    // Keeps track of files that still exist locally
    let mut existing_g_files: HashSet<String> = HashSet::new();

    // Keeps track of GlacierFiles that have been saved to the local database
    let mut saved_g_files: HashSet<String> = HashSet::new();

    // Keeps track of GlacierFiles that have been deleted from the local database
    let mut deleted_g_files: HashSet<String> = HashSet::new();

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
            existing_g_files.insert(file_change.g_file.file_path.clone());
        };

        // If a file version was deleted 
        if let Some(hash) = file_change.old_hash {
            let h_t_c = get_hash_tracker_change(args,  dynamo_client, &mut hash_tracker_changes, hash).await;
            h_t_c.new.del_file_name(file_change.g_file.file_path.clone());
            h_t_c.deleted_files.push(file_change.g_file.clone());
        };
    };

    let num_changes = hash_tracker_changes.len();
    let mut failures = 0;

    // Make all updates in the order S3 -> DynamoDB -> PostgreSQL, and continue on any failure
    for (hash, mut hash_tracker_change) in hash_tracker_changes {
        
        if hash_tracker_change.changed() {

            // Publish S3 changes

            // Delete
            if hash_tracker_change.old.has_files() {
                if !hash_tracker_change.new.has_files() {
                    println!("Deleting hash: {} from S3.", hash.clone());
                    let Ok(_) = s3::delete(s3_client, args.bucket_name.clone(), hash.clone()).await 
                        else { failures += 1; continue; };
                }
            }

            // Put
            else if hash_tracker_change.old.is_expired() {
                if hash_tracker_change.new.has_files() {
                    println!("Uploading hash: {} to S3.", hash.clone());
                    let Some(g_file) = hash_tracker_change.created_files.first() 
                        else { failures += 1; continue; };
                    let Ok(_) = s3::put(s3_client, args.bucket_name.clone(), hash.clone(), g_file.file_path.to_string()).await 
                        else { failures += 1; continue; };
                }
            }

            // Undelete
            else {
                if hash_tracker_change.new.has_files() {
                    println!("Undeleting hash: {} to S3.", hash.clone());
                    let Ok(_) = s3::undelete(s3_client, args.bucket_name.clone(), hash.clone()).await 
                        else { failures += 1; continue; };
                    hash_tracker_change.new.expiration = new_expiration(args.min_storage_duration.clone());
                }
            }

            // Publish HashTrackers
            println!("Uploading hash tracker: {} to DynamoDB.", hash.clone());
            let Ok(_) = hash_tracker_change.new.update(dynamo_client, args.dynamo_table.clone()).await 
                else { failures += 1; continue; };
        }

        // Publish GlacierFiles
        for d_file in hash_tracker_change.deleted_files {
            if !deleted_g_files.contains(&d_file.file_path) && !existing_g_files.contains(&d_file.file_path) {
                println!("Deleting file entry: {} from local database.", d_file.file_path.clone());
                d_file.delete(conn);
                deleted_g_files.insert(d_file.file_path.clone());
            }
        }

        for c_file in hash_tracker_change.created_files {
            if !saved_g_files.contains(&c_file.file_path) {
                println!("Inserting file entry: {} to local database.", c_file.file_path.clone());
                c_file.insert(conn);
                saved_g_files.insert(c_file.file_path.clone());
            }
        }
    };


    (num_changes - failures, failures)
}

async fn get_hash_tracker_change<'a>(args: &Args, dynamo_client: &DynamoClient, hash_tracker_changes: &'a mut HashMap<String, HashTrackerChange>, hash: String) -> &'a mut HashTrackerChange {

    if !hash_tracker_changes.contains_key(&hash) {

        let new;
        let old;
        
        match HashTracker::get(dynamo_client, args.dynamo_table.clone(), hash.clone()).await {
            Some(hash_tracker) => {
                new = hash_tracker.clone();
                old = hash_tracker;
            },
            None => {
                new = HashTracker::new(hash.clone(), new_expiration(args.min_storage_duration.clone()));
                old = HashTracker::new(hash.clone(), DateTime::UNIX_EPOCH);
            },
        };

        hash_tracker_changes.insert(
            hash.clone(),
            HashTrackerChange {
                new,
                old,
                created_files: vec![],
                deleted_files: vec![],
            }
        );
    }

    hash_tracker_changes.get_mut(&hash).unwrap()
}