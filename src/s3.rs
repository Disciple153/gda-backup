use std::{io::Error, path::Path};
use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    error::SdkError,
    operation::{list_objects_v2::paginator::ListObjectsV2Paginator, put_object::{
        PutObjectError,
        PutObjectOutput
    }},
    primitives::ByteStream,
    Client,
};

pub async fn get_buckets(client: &Client) -> Result<Vec<aws_sdk_s3::types::Bucket>, Error> {
    let resp: aws_sdk_s3::operation::list_buckets::ListBucketsOutput = client.list_buckets().send().await.expect("list_buckets failed");
    Ok(resp.buckets().to_vec())
}

pub async fn get_client() -> Client {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config: aws_config::SdkConfig = aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await;
    Client::new(&config)
}

pub async fn upsert(
    client: &Client,
    bucket_name: &str,
    file_name: String,
    key: String,
) -> Result<PutObjectOutput, SdkError<PutObjectError>> {
    let body = ByteStream::from_path(Path::new(&file_name)).await;
    client
        .put_object()
        .bucket(bucket_name)
        .key(key)
        .body(body.unwrap())
        .send()
        .await
}

pub async fn delete(client: &Client, bucket: &str, key: &str) -> Result<(), Error> {
    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await.expect("Object deletion failed.");

    println!("Object deleted.");

    Ok(())
}

pub fn list(client: &Client, bucket: &str) -> ListObjectsV2Paginator {
    client
        .list_objects_v2()
        .bucket(bucket.to_owned())
        .into_paginator()
}