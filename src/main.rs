use std::{io::Error, time::SystemTime};
use aws_sdk_s3::Client as S3Client;
use walkdir::WalkDir;
use diesel::prelude::*;

use glacier_sync::{
    clear_local_state,
    establish_connection,
    get_changed_files,
    get_missing_files,
    get_new_files,
    get_pending_delete_files,
    get_pending_upsert_files,
    glacier_state_is_empty,
    models::*
};

mod s3;

const BUCKET_NAME: &str = "disciple153-test";

#[tokio::main]
async fn main() -> Result<(), Error> {
    // let pwd: std::path::PathBuf = env::current_dir()?;
    let pwd = "/home/disciple153/documents/homelab-tf/glacier_sync/backup_test";

    // GET CONNECTIONS
    let conn: &mut PgConnection = &mut establish_connection();
    let s3_client = s3::get_client().await;

    // CLEAR LOCAL STATE FROM DATABASE
    clear_local_state(conn);

    // LOAD LOCAL STATE INTO DATABASE

    for file in WalkDir::new(pwd).into_iter().filter_map(|e: Result<walkdir::DirEntry, walkdir::Error>| e.ok()) {
        if file.metadata()?.is_file() {
            LocalFile {
                file_path: file.path().display().to_string(),
                modified: file.metadata()?.modified()?
            }.insert(conn);
        }
    }

    // CHECK GLACIER STATE
    // If glacier_state is empty, populate it from Glacier.
    if glacier_state_is_empty(conn) {
        // TODO Load glacier_state from AWS.
        println!("glacier_file_count: 0");
        load_from_s3(conn, &s3_client).await;
    }
    else {
        println!("glacier_file_count: >0");
    }

    // COMPARE LOCAL STATE WITH GLACIER STATE

    // UPSERT ALL FILES IN GLACIER STATE WITH MISMATCHED MODIFIED AND UPLOADED ROWS
    fix_pending_upserts(conn, &s3_client).await;

    // DELETE ALL FILES IN GLACIER PENDING DELETION
    fix_pending_deletes(conn, &s3_client).await;

    // UPLOAD ALL NEW FILES
    upload_new_files(conn, &s3_client).await;
    
    // UPDATE ALL CHANGED FILES
    update_changed_files(conn, &s3_client).await;
    
    // ADD DELETE MARKERS TO MISSING FILES
    delete_missing_files(conn, &s3_client).await;
    
    // CLEAR LOCAL STATE FROM DATABASE
    clear_local_state(conn);


    //println!("Loading into db: {file_path} last modified at {modified_str}");

    Ok(())
}

async fn load_from_s3(conn: &mut PgConnection, s3_client: &S3Client) {
    let mut s3_paginator = s3::list(&s3_client, BUCKET_NAME).send();

    while let Some(result) = s3_paginator.next().await {
        match result {
            Ok(output) => {
                for object in output.contents() {
                    let last_modified: SystemTime = SystemTime::try_from(*object.last_modified().unwrap()).expect("msg");

                    GlacierFile {
                        file_path: object.key().unwrap_or("Unknown").to_string(),
                        modified: last_modified,
                        uploaded: Some(last_modified),
                        pending_delete: false,
                    }.insert(conn);
                }
            }
            Err(err) => {
                eprintln!("{err:?}")
            }
        }
    }
}

async fn fix_pending_upserts(conn: &mut PgConnection, s3_client: &S3Client) -> isize {
    let mut failures: isize = 0;
    let pending_upsert_files: Vec<GlacierFile> = get_pending_upsert_files(conn);

    let length = pending_upsert_files.len();
    println!("pending_upsert_files: {length}");

    for mut file in pending_upsert_files {
        // TODO Upsert to glacier.
        match s3::upsert(s3_client, BUCKET_NAME, &file.file_path, &file.file_path).await {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                file.uploaded = Some(file.modified);
                file.update(conn);
            }
        };
    };

    failures
}

async fn fix_pending_deletes(conn: &mut PgConnection, s3_client: &S3Client) -> isize {
    let mut failures: isize = 0;
    let pending_delete_files: Vec<GlacierFile> = get_pending_delete_files(conn);

    let length = pending_delete_files.len();
    println!("pending_delete_files: {length}");

    for file in pending_delete_files {
        // Delete from glacier.
        match s3::delete(s3_client, BUCKET_NAME, &file.file_path).await {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                file.delete(conn);
            }
        };
    };

    failures
}

async fn upload_new_files(conn: &mut PgConnection, s3_client: &S3Client) -> isize {
    let mut failures: isize = 0;
    let new_files: Vec<LocalFile> = get_new_files(conn);

    let length = new_files.len();
    println!("new_files: {length}");

    for file in new_files {
        // Copy from local_state to glacier state, leaving uploaded null.
        let mut file = GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.insert(conn);

        // Upload to glacier.
        match s3::upsert(s3_client, BUCKET_NAME, &file.file_path, &file.file_path).await {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                file.uploaded = Some(file.modified);
                file.update(conn);
            }
        };
    };

    failures
}

async fn update_changed_files(conn: &mut PgConnection, s3_client: &S3Client) -> isize {
    let mut failures: isize = 0;
    let updated_files: Vec<LocalFile> = get_changed_files(conn);

    let length = updated_files.len();
    println!("updated_files: {length}");

    for file in updated_files {
        // Copy from local_state to glacier state, leaving uploaded as it was.
        let mut file = GlacierFile {
            file_path: file.file_path,
            modified: file.modified,
            uploaded: None,
            pending_delete: false
        }.update(conn);

        // Upload to glacier.
        match s3::upsert(s3_client, BUCKET_NAME, &file.file_path, &file.file_path).await {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Copy file.modified to file.updated.
                file.uploaded = Some(file.modified);
                file.update(conn);
            }
        };
    };

    failures
}

async fn delete_missing_files(conn: &mut PgConnection, s3_client: &S3Client) -> isize {
    let mut failures: isize = 0;
    let deleted_files: Vec<GlacierFile> = get_missing_files(conn);

    let length = deleted_files.len();
    println!("deleted_files: {length}");

    for mut file in deleted_files {
        // Set pending_delete to TRUE.
        file.pending_delete = true;
        file.update(conn);

        // Add delete marker.
        match s3::delete(s3_client, BUCKET_NAME, &file.file_path).await {
            Err(_) => {
                failures += 1;
            },
            Ok(_) => {
                // Delete from glacier_state
                file.delete(conn);
            }
        };
    };

    failures
}

