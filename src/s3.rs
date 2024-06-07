use std::{
    collections::HashMap, 
    io::Error, 
    path::Path, 
    time::SystemTime
};
use aws_sdk_s3::{
    error::SdkError,
    operation::{
        delete_object::{
            DeleteObjectError, 
            DeleteObjectOutput
        }, 
        list_object_versions::ListObjectVersionsError, 
        list_objects_v2::{
            ListObjectsV2Error, 
            ListObjectsV2Output
        },
        put_object::{
            PutObjectError,
            PutObjectOutput
        }
    },
    primitives::ByteStream,
    Client,
};
use aws_smithy_runtime_api::http::Response;
use futures::{stream::FuturesUnordered, StreamExt};

use crate::aws;

pub async fn get_buckets(client: &Client) -> Result<Vec<aws_sdk_s3::types::Bucket>, Error> {
    let resp: aws_sdk_s3::operation::list_buckets::ListBucketsOutput = client.list_buckets().send().await.expect("list_buckets failed");
    Ok(resp.buckets().to_vec())
}

pub async fn get_client() -> Client {
    let config = aws::get_config().await;
    Client::new(&config)
}

pub async fn put(client: &Client, bucket_name: String, file_path: String, key: String) -> Result<PutObjectOutput, SdkError<PutObjectError>> {

    let body = ByteStream::from_path(Path::new(&file_path)).await;
    client
        .put_object()
        .bucket(bucket_name)
        .key(key)
        .body(body.unwrap())
        .send()
        .await
}

pub async fn delete(client: &Client, bucket: String, key: String) -> Result<DeleteObjectOutput, SdkError<DeleteObjectError>> {

    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
}

pub async fn list_objects(client: &Client, bucket: &str) -> Result<(), Error> {
    let mut response = client
        .list_objects_v2()
        .bucket(bucket.to_owned())
        .max_keys(10) // In this example, go 10 at a time.
        .into_paginator()
        .send();

    while let Some(result) = response.next().await {
        match result {
            Ok(output) => {
                for object in output.contents() {
                    println!(" - {}", object.key().unwrap_or("Unknown"));
                }
            }
            Err(err) => {
                eprintln!("{err:?}")
            }
        }
    }

    Ok(())
}

pub async fn list(client: &Client, bucket: String) -> Result<HashMap<String, SystemTime>, SdkError<ListObjectsV2Error>> {

    let mut output = HashMap::new();

    let _ = client
        .list_objects_v2()
        .bucket(bucket.to_owned())
        .into_paginator()
        .send()
        .collect::<Result<Vec<ListObjectsV2Output>, SdkError<ListObjectsV2Error>>>()
        .await?
        
        // For every page in the results
        .iter().map(|page| {

            // For every file in the page
            let _ = page.contents().iter().map(|file| {
                output.insert(
                    file.key()?.to_owned(), 
                    SystemTime::try_from(*file.last_modified()?).ok()?
                );

                Some(())
            });
        });
    
    Ok(output)
}

async fn get_delete_marker_versions(client: &Client, bucket: String, key: String,) -> Result<Vec<String>, SdkError<ListObjectVersionsError, Response>> {
    let empty_vec = vec![];

    let mut output = Vec::new();

    let result = client
        .list_object_versions()
        .bucket(bucket.to_owned())
        .prefix(key)
        .max_keys(1)
        .send().await?;

    for delete_marker in result.delete_markers.unwrap_or(empty_vec) {
        match delete_marker.version_id {
            Some(version_id) => output.push(version_id),
            None => (),
        };
    };

    Ok(output)
}

pub async fn undelete(client: &Client, bucket: String, key: String) -> Result<(), SdkError<ListObjectVersionsError, Response>> {
    let delete_futures = FuturesUnordered::new();
    
    let dm_versions = get_delete_marker_versions(client, bucket.clone(), key.clone()).await?;

    for version in dm_versions {
        delete_futures.push(client
            .delete_object()
            .bucket(bucket.clone())
            .key(key.clone())
            .version_id(version)
            .send());
    };

    let _: Vec<_> = delete_futures.collect().await;

    Ok(())
}