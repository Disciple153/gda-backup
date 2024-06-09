use std::collections::hash_set::Iter as SetIter;
use std::{
    collections::HashMap, fs::{self, File, create_dir_all}, io::{Error as IoError, Write}, path::Path, time::SystemTime
};
use aws_sdk_s3::{
    error::SdkError,
    operation::{
        delete_object::{
            DeleteObjectError, 
            DeleteObjectOutput
        }, get_object::GetObjectError, list_objects_v2::{
            ListObjectsV2Error, 
            ListObjectsV2Output
        }, put_object::{
            PutObjectError,
            PutObjectOutput
        }, restore_object::{RestoreObjectError, RestoreObjectOutput}
    },
    primitives::ByteStream,
    Client,
};
use aws_smithy_runtime_api::http::Response;
use aws_sdk_s3::primitives::SdkBody;
use aws_smithy_types::byte_stream::error::Error as AwsSmithyError;

use crate::aws;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum S3GetError {
    #[error("S3GetObjectError")]
    S3GetObjectError(#[from] SdkError<GetObjectError, Response<SdkBody>>),

    #[error("IoError")]
    IoError(#[from] IoError),

    #[error("AwsSmithyError")]
    AwsSmithyError(#[from] AwsSmithyError),

    #[error("S3GetError")]
    S3GetError(String),
}

/// The function `get_buckets` retrieves a list of S3 buckets using the AWS SDK for
/// Rust.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is of type `&Client`, which is a reference to
/// an AWS SDK S3 client. This client is used to interact with the AWS S3 service to
/// list buckets.
/// 
/// Returns:
/// 
/// The function `get_buckets` returns a `Result` containing a vector of
/// `aws_sdk_s3::types::Bucket` objects or an `Error` in case of failure.
pub async fn get_buckets(client: &Client) -> Result<Vec<aws_sdk_s3::types::Bucket>, AwsSmithyError> {
    let resp: aws_sdk_s3::operation::list_buckets::ListBucketsOutput = client.list_buckets().send().await.expect("list_buckets failed");
    Ok(resp.buckets().to_vec())
}

/// The function `get_client` asynchronously retrieves a client using AWS
/// configuration.
/// 
/// Returns:
/// 
/// The `get_client` function is returning a `Client` instance. The function first
/// awaits the result of `aws::get_config()` to get the AWS configuration, then
/// creates a new `Client` instance using that configuration and returns it.
pub async fn get_client() -> Client {
    let config = aws::get_config().await;
    Client::new(&config)
}

/// The function `put` asynchronously uploads a file to a specified bucket in Rust
/// using the AWS SDK.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is an instance of the AWS SDK `Client` that
/// is used to interact with AWS services. It is typically initialized with AWS
/// credentials and configuration settings to make API calls to AWS services like
/// S3.
/// * `bucket_name`: The `bucket_name` parameter in the `put` function represents
/// the name of the bucket where you want to store the object. It is a String that
/// should contain the name of the S3 bucket where you want to upload the file.
/// * `key`: The `key` parameter in the `put` function represents the unique
/// identifier or name of the object you want to store in the specified bucket. It
/// is used to reference the object within the bucket and is typically a string
/// value.
/// * `file_path`: The `file_path` parameter in the `put` function represents the
/// path to the file that you want to upload to the specified S3 bucket. It should
/// be a string containing the full path to the file on your local filesystem.
/// 
/// Returns:
/// 
/// The `put` function returns a `Result` containing either a `PutObjectOutput` or
/// an `SdkError` with a `PutObjectError`.
pub async fn put(client: &Client, bucket_name: String, key: String, file_path: String) -> Result<PutObjectOutput, SdkError<PutObjectError>> {

    let body = ByteStream::from_path(Path::new(&file_path)).await;
    client
        .put_object()
        .bucket(bucket_name)
        .key(key)
        .body(body.unwrap())
        .send()
        .await
}

/// The function `delete` deletes an object from a specified bucket using the AWS
/// SDK for Rust.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is an instance of the AWS SDK `Client` that
/// is used to interact with AWS services. In this case, it is being used to delete
/// an object from an S3 bucket.
/// * `bucket`: The `bucket` parameter in the `delete` function represents the name
/// of the bucket from which you want to delete an object. Buckets are containers
/// for objects stored in cloud storage services like Amazon S3. When you specify
/// the bucket name, the function knows from which bucket to delete the object.
/// * `key`: The `key` parameter in the `delete` function represents the unique
/// identifier or name of the object you want to delete from the specified `bucket`.
/// It is used to identify the specific object within the bucket that you want to
/// remove.
/// 
/// Returns:
/// 
/// The `delete` function is returning a `Result` type with the success case being
/// `DeleteObjectOutput` and the error case being `SdkError<DeleteObjectError>`.
pub async fn delete(client: &Client, bucket: String, key: String) -> Result<DeleteObjectOutput, SdkError<DeleteObjectError>> {

    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
}


/// The `restore` function in Rust asynchronously restores an object in a bucket
/// using the AWS SDK for Rust.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is an instance of the AWS SDK `Client`
/// struct, which is used to interact with AWS services. In this case, it is being
/// used to restore an object from a specified bucket.
/// * `bucket`: The `bucket` parameter is a string that represents the name of the
/// bucket where the object to be restored is located.
/// * `key`: The `key` parameter in the `restore` function represents the unique
/// identifier or name of the object that you want to restore from the specified
/// bucket. It is used to locate the specific object within the bucket for
/// restoration.
/// 
/// Returns:
/// 
/// The `restore` function returns a `Result` containing either a
/// `RestoreObjectOutput` on success or an `SdkError` on failure, which includes a
/// `RestoreObjectError` and a `Response`.
pub async fn restore(client: &Client, bucket: String, key: String) -> Result<RestoreObjectOutput, SdkError<RestoreObjectError, Response>> {
    client.restore_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
}

pub async fn get_object<'a>(client: &Client, bucket: String, key: String, prefix: String, file_paths: SetIter<'a, String>) -> Result<Vec<String>, S3GetError> {

    if file_paths.len() == 0 {
        return Ok(vec![]);
    }
    
    let files: Vec<String> = file_paths.map(|s| s.clone()).collect();
    
    let first_file = prefix.clone() + &files[0];
    let (first_dir, _) = first_file.rsplit_once('/').unwrap();
    
    create_dir_all(first_dir)?;
    let mut file = File::create(first_file.clone())?;
    
    let mut object = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;
    
    while let Some(bytes) = object.body.try_next().await? {
        file.write_all(&bytes)?;
    }
    
    for i in 1..files.len() {
        let file = prefix.clone() + &files[i];
        let (dir, _) = file.rsplit_once('/').unwrap();

        create_dir_all(dir)?;
        fs::copy(first_file.clone(), file)?;
    }

    Ok(files)
}

/// The function `list_objects` asynchronously lists objects in a specified bucket
/// with a maximum of 10 objects at a time.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is an instance of a client that allows you to
/// interact with a service, such as an AWS S3 client for working with objects in an
/// S3 bucket. It provides methods for listing objects, uploading files, downloading
/// files, and other operations related to the service it is connected
/// * `bucket`: The `bucket` parameter in the `list_objects` function is a reference
/// to a string that represents the name of the bucket from which you want to list
/// objects. Buckets are containers for storing objects in cloud storage services
/// like Amazon S3 or Google Cloud Storage. The function uses this parameter to
/// specify
/// 
/// Returns:
/// 
/// The `list_objects` function returns a `Result<(), Error>`.
pub async fn list_objects(client: &Client, bucket: &str) -> Result<(), AwsSmithyError> {
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

/// The function asynchronously lists objects in a bucket and returns a HashMap of
/// file keys and their last modified times.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is an instance of the AWS SDK client that is
/// used to interact with AWS services. In this function, it is specifically used to
/// list objects in a bucket.
/// * `bucket`: The `bucket` parameter in the `list` function represents the name of
/// the bucket from which you want to list objects. This function uses the AWS SDK
/// for Rust to list objects in the specified bucket and returns a HashMap
/// containing the keys of the objects and their last modified times.
/// 
/// Returns:
/// 
/// The `list` function returns a `Result` containing a `HashMap<String,
/// SystemTime>` on success or a `SdkError<ListObjectsV2Error>` on failure. The
/// `HashMap` contains file keys as strings and their corresponding `SystemTime`
/// values representing the last modified time of each file in the specified bucket.
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

