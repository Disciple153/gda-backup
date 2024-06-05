use std::time::SystemTime;

use crate::environment::Args;
use crate::models::GlacierFile;

use crate::s3;
use aws_sdk_s3::Client as S3Client;
use diesel::prelude::PgConnection;

pub async fn db_from_s3(args: &Args, conn: &mut PgConnection, s3_client: &S3Client) {
    let mut s3_paginator = s3::list(&s3_client, args.bucket_name.clone()).send();

    if args.dry_run {
        return ()
    }

    while let Some(result) = s3_paginator.next().await {
        match result {
            Ok(output) => {
                for object in output.contents() {
                    let last_modified: SystemTime = SystemTime::try_from(*object.last_modified().unwrap()).expect("msg");

                    GlacierFile {
                        file_path: object.key().unwrap_or("Unknown").to_string(),
                        file_hash: None,
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