use std::collections::{HashMap, HashSet};
use std::path::Path;
use log::{debug, error, info};
use walkdir::WalkDir;

use crate::dynamodb::HashTracker;
use crate::environment::{BackupArgs, Cli};
use crate::models::{GlacierFile, LocalFile};

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
use regex::Regex;

// Use BLAKE2B if running on 64 bit CPU
#[cfg(target_pointer_width = "64")]
use checksums::Algorithm::BLAKE2B as HASH_ALGO;

// Use BLAKE2S if running on 32 bit CPU or lower
#[cfg(not(target_pointer_width = "64"))]
use checksums::Algorithm::BLAKE2S as HASH_ALGO;

/// The `FileChange` struct in Rust represents a change in a GlacierFile with an
/// optional old hash value.
/// 
/// Properties:
/// 
/// * `g_file`: The `g_file` property in the `FileChange` struct appears to be of
/// type `GlacierFile`. It likely represents a file within a Glacier storage system.
/// * `old_hash`: The `old_hash` property in the `FileChange` struct is an optional
/// field that can hold a value of type `String`. It represents the previous hash
/// value associated with the file before the change occurred. The `Option` enum in
/// Rust is used to express that a value can be either something
struct FileChange {
    g_file: GlacierFile,
    old_hash: Option<String>,
}

/// The `HashTrackerChange` struct represents changes in a `HashTracker` along with
/// associated created and deleted files.
/// 
/// Properties:
/// 
/// * `new`: The `new` property in the `HashTrackerChange` struct is of type
/// `HashTracker`. It likely represents the updated or new state of a hash tracker
/// object.
/// * `old`: The `old` property in the `HashTrackerChange` struct represents the
/// previous state of a `HashTracker` object before any changes were made. It is
/// used to track the original state before any modifications or updates occurred.
/// * `created_files`: The `created_files` property in the `HashTrackerChange`
/// struct is a vector that contains instances of the `GlacierFile` struct. This
/// vector stores the files that were created as part of the change being tracked by
/// the `HashTrackerChange` struct.
/// * `deleted_files`: The `deleted_files` property in the `HashTrackerChange`
/// struct is a vector of `GlacierFile` instances representing the files that were
/// deleted in the change.
#[derive(Clone, Debug)]
struct HashTrackerChange {
    new: HashTracker,
    old: HashTracker,
    created_files: Vec<GlacierFile>,
    deleted_files: Vec<GlacierFile>,
}

/// The above Rust code defines an implementation for the `HashTrackerChange`
/// struct. It includes a method `changed()` that returns a boolean value indicating
/// whether the `new` field of the `HashTrackerChange` instance is different from
/// the `old` field. The method compares the two fields and returns `true` if they
/// are different, and `false` otherwise.
impl HashTrackerChange {
    fn changed(&self) -> bool {
        self.new != self.old
    }
}

/// The function `load` iterates through files in a directory, extracts metadata,
/// and inserts file information into a database.
/// 
/// Arguments:
/// 
/// * `args`: The `args` parameter in the `load` function seems to be of type
/// `BackupArgs`, which likely contains information or settings related to a backup
/// operation. It is being used to access the `target_dir` field, which is a
/// directory path where files are being loaded from.
/// * `conn`: The `conn` parameter in the `load` function is a mutable reference to
/// a `PgConnection`, which is a connection to a PostgreSQL database. This parameter
/// allows the function to interact with the database to load data from the local
/// file system into the database.
pub fn load(args: BackupArgs, conn: &mut PgConnection) {
    // Load local_state into database
    for file in WalkDir::new(args.target_dir.clone()).into_iter().filter_map(|e: Result<walkdir::DirEntry, walkdir::Error>| e.ok()) {

        let Ok(metadata) = file.metadata() else {continue };

        if metadata.is_file() {

            let file_path = file.path().display().to_string();
            
            let mut filtered = false;
            let mut i = 0;
    
            while !filtered && i < args.filter.len() {
                filtered = Regex::new(&args.filter[i]).unwrap()
                    .is_match(&file_path);
                i += 1;
            }

            if filtered {
                info!("File filtered out of tracked files: {file_path}");
                continue;
            }
    
            let result = LocalFile {
                file_path,
                modified: metadata.modified().expect("Error: OS does not support modified time metadata.")
            }.insert(conn);

            match result {
                Ok(_) => (),
                Err(error) => error!("Failed to load file into local database: {:?}\n Error: {}", file, error),
            }
        }
    }
}

/// The `backup` function in Rust asynchronously manages file backups by tracking
/// changes, updating databases, and interacting with S3 and DynamoDB services.
/// 
/// Arguments:
/// 
/// * `args`: The `args` parameter in the `backup` function seems to be a struct or
/// object containing configuration settings or parameters required for the backup
/// operation. It likely includes information such as the bucket name, DynamoDB
/// table name, minimum storage duration, and possibly other settings needed for
/// interacting with AWS services and databases
/// * `conn`: The `conn` parameter in the `backup` function is a mutable reference
/// to a `PgConnection`, which represents a connection to a PostgreSQL database.
/// This connection is used to interact with the database to perform operations like
/// querying for files, updating records, and deleting entries during the backup
/// process.
/// * `s3_client`: The `s3_client` parameter in the `backup` function is a reference
/// to an S3 client that is used to interact with an Amazon Simple Storage Service
/// (S3) bucket. This client is responsible for performing operations such as
/// uploading files to S3, deleting files from S3, and
/// * `dynamo_client`: The `dynamo_client` parameter in the `backup` function is an
/// instance of the `DynamoClient` struct, which is used to interact with DynamoDB
/// for storing and retrieving data related to hash trackers. This client is
/// responsible for performing operations such as updating hash trackers in DynamoDB
/// and retrieving
pub async fn backup(cli: Cli, args: BackupArgs, conn: &mut PgConnection, s3_client: &S3Client, dynamo_client: &DynamoClient) -> (usize, usize) {

    info!("Preparing to back up: Scanning all files...");

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

    info!("Preparing to back up: Determining which files need to be backed up...");

    // Get HashTrackers for all changes and update them to reflect the current state
    let mut hash_tracker_changes: HashMap<String, HashTrackerChange> = HashMap::new();
    for file_change in file_changes {

        // If a file version was created 
        if let Some(hash) = file_change.g_file.file_hash.clone() { 
            let h_t_c = get_hash_tracker_change(args.clone(), dynamo_client, &mut hash_tracker_changes, hash).await;
            h_t_c.new.add_file_name(file_change.g_file.file_path.clone());
            h_t_c.created_files.push(file_change.g_file.clone());
            existing_g_files.insert(file_change.g_file.file_path.clone());
        };

        // If a file version was deleted 
        if let Some(hash) = file_change.old_hash {
            let h_t_c = get_hash_tracker_change(args.clone(), dynamo_client, &mut hash_tracker_changes, hash).await;
            h_t_c.new.del_file_name(file_change.g_file.file_path.clone());
            h_t_c.deleted_files.push(file_change.g_file.clone());
        };
    };

    let num_changes = hash_tracker_changes.len();
    let mut failures = 0;

    if cli.dry_run {
        info!("Preparation complete. Dry run output:");
        for (_, hash_tracker_change) in hash_tracker_changes {
            for file in hash_tracker_change.created_files {
                info!("Backup: {}", file.file_path);
            };

            for file in hash_tracker_change.deleted_files {
                info!("Delete: {}", file.file_path);
            };
        };

        return (num_changes - failures, failures);
    }

    info!("Preparation complete. Backing up...");

    // Make all updates in the order S3 -> DynamoDB -> PostgreSQL, and continue on any failure
    for (hash, mut hash_tracker_change) in hash_tracker_changes {
        
        if hash_tracker_change.changed() {

            // Publish S3 changes

            // Delete
            if hash_tracker_change.old.has_files() {
                if !hash_tracker_change.new.has_files() {
                    debug!("Deleting hash: {} from S3.", hash.clone());
                    match s3::delete(args.clone().into(), s3_client, hash.clone()).await {
                        Ok(_) => (),
                        Err(error) => {
                            error!("Failed to delete file from S3: {:?}\n Error: {}", hash_tracker_change, error);
                            failures += 1;
                            continue;
                        }
                    }
                }
            }

            // Put
            else if hash_tracker_change.old.is_expired() {
                if hash_tracker_change.new.has_files() {
                    debug!("Uploading hash: {} to S3.", hash.clone());
                    
                    let g_file = match hash_tracker_change.created_files.first() {
                        Some(value) => value,
                        None => {
                            error!("Internal error. File missing from hash tracker: {:?}", hash_tracker_change);
                            failures += 1;
                            continue;
                        }
                    };
                    match s3::put(args.clone().into(), s3_client, hash.clone(), g_file.file_path.to_string()).await {
                        Ok(_) => (),
                        Err(error) => {
                            error!("Failed to upload file to S3: {:?}\n Error: {}", hash_tracker_change, error);
                            failures += 1;
                            continue;
                        }
                    }
                }
            }

            // Undelete
            else {
                if hash_tracker_change.new.has_files() {
                    debug!("Undeleting hash: {} to S3.", hash.clone());
                    match s3::restore(args.clone().into(), s3_client, hash.clone()).await {
                        Ok(_) => (),
                        Err(error) => {
                            error!("Failed to remove delete marker from file in S3: {:?}\n Error: {}", hash_tracker_change, error);
                            failures += 1;
                            continue;
                        }
                    }
                    hash_tracker_change.new.expiration = new_expiration(args.min_storage_duration.clone());
                }
            }

            // Publish HashTrackers
            debug!("Uploading hash tracker: {} to DynamoDB.", hash.clone());
            match hash_tracker_change.new.update(args.clone().into(), dynamo_client).await {
                Ok(_) => (),
                Err(error) => {
                    error!("Failed to upload hash tracker to DynamoDB: {:?}\n Error: {}", hash_tracker_change, error);
                    failures += 1;
                    continue;
                }
            }
        }

        // Publish GlacierFiles
        for d_file in hash_tracker_change.deleted_files {
            if !deleted_g_files.contains(&d_file.file_path) && !existing_g_files.contains(&d_file.file_path) {
                debug!("Deleting file entry: {} from local database.", d_file.file_path.clone());
                match d_file.delete(conn) {
                    Ok(_) => info!("Deleted: {}", d_file.file_path),
                    Err(error) => {
                        error!("Failed to remove file from local database: {:?}\n Error: {}", d_file, error);
                        failures += 1;
                        continue;
                    }
                }
                deleted_g_files.insert(d_file.file_path.clone());
            }
        }

        for c_file in hash_tracker_change.created_files {
            if !saved_g_files.contains(&c_file.file_path) {
                debug!("Inserting file entry: {} to local database.", c_file.file_path.clone());
                match c_file.insert(conn) {
                    Ok(_) => info!("Uploaded: {}", c_file.file_path),
                    Err(error) => {
                        error!("Failed to insert/update file into local database: {:?}\n Error: {}", c_file, error);
                        failures += 1;
                        continue;
                    }
                };
                saved_g_files.insert(c_file.file_path.clone());
            }
        }
    };


    (num_changes - failures, failures)
}

/// The function `get_hash_tracker_change` retrieves or creates a
/// `HashTrackerChange` object for a given hash from a HashMap.
/// 
/// Arguments:
/// 
/// * `args`: The `args` parameter is a reference to a struct or object that
/// contains various configuration or input arguments needed for the function to
/// operate. It likely includes information such as the DynamoDB table name, minimum
/// storage duration, and possibly other settings required for the function's logic.
/// * `dynamo_client`: The `dynamo_client` parameter in the function
/// `get_hash_tracker_change` is a reference to a `DynamoClient` instance. This
/// parameter is used to interact with a DynamoDB database in order to retrieve or
/// store data related to hash tracking. The `DynamoClient` likely provides methods
/// * `hash_tracker_changes`: The `hash_tracker_changes` parameter is a mutable
/// reference to a `HashMap` that stores `String` keys and `HashTrackerChange`
/// values. This HashMap is used to keep track of changes related to a specific hash
/// value. The function `get_hash_tracker_change` checks if the provided `hash
/// * `hash`: The `hash` parameter is a string that represents the unique identifier
/// of a hash value.
/// 
/// Returns:
/// 
/// A mutable reference to the `HashTrackerChange` object corresponding to the
/// provided `hash` key in the `hash_tracker_changes` HashMap is being returned.
async fn get_hash_tracker_change<'a>(args: BackupArgs, dynamo_client: &DynamoClient, hash_tracker_changes: &'a mut HashMap<String, HashTrackerChange>, hash: String) -> &'a mut HashTrackerChange {

    if !hash_tracker_changes.contains_key(&hash) {

        let new;
        let old;
        
        match HashTracker::get(args.clone().into(), dynamo_client, hash.clone()).await {
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

/// The function `new_expiration` calculates a new expiration date based on the
/// current time and a minimum storage duration in Rust.
/// 
/// Arguments:
/// 
/// * `min_storage_duration`: The `min_storage_duration` parameter in the
/// `new_expiration` function represents the minimum duration in days for which an
/// item should be stored before it expires. This value is used to calculate the
/// expiration time by adding the specified number of days to the current time.
/// 
/// Returns:
/// 
/// A `DateTime<Utc>` value is being returned. The function `new_expiration`
/// calculates a new expiration time based on the current time (`Utc::now()`) and a
/// minimum storage duration provided as input.
fn new_expiration(min_storage_duration: i64) -> DateTime<Utc> {
    match Utc::now().checked_add_signed(Duration::days(min_storage_duration)) {
        Some(time) => time,
        None => DateTime::UNIX_EPOCH,
    }
}