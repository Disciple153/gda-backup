use std::{io::Error, path::Path};
use aws_sdk_s3::{
    error::SdkError,
    operation::{delete_object::{DeleteObjectError, DeleteObjectOutput}, list_object_versions::ListObjectVersionsError, list_objects_v2::paginator::ListObjectsV2Paginator, put_object::{
        PutObjectError,
        PutObjectOutput
    }},
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

pub fn list(client: &Client, bucket: String) -> ListObjectsV2Paginator {

    client
        .list_objects_v2()
        .bucket(bucket.to_owned())
        .into_paginator()
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