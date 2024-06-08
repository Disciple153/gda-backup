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
pub async fn get_buckets(client: &Client) -> Result<Vec<aws_sdk_s3::types::Bucket>, Error> {
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
pub async fn put(client: &Client, bucket_name: String, key: String, file_path: String,) -> Result<PutObjectOutput, SdkError<PutObjectError>> {

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

/// The `undelete` function in Rust asynchronously deletes all delete marker
/// versions of an object in a specified bucket.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is an instance of the `Client` struct, which
/// is used to interact with the AWS S3 service. It contains the necessary
/// configuration and credentials to make requests to the S3 API.
/// * `bucket`: The `bucket` parameter in the `undelete` function represents the
/// name of the bucket from which you want to undelete the object. It is a String
/// type that specifies the bucket where the object to be undeleted is located.
/// * `key`: The `key` parameter in the `undelete` function represents the unique
/// identifier of the object that you want to undelete from the specified S3 bucket.
/// It is used to identify the specific object that was previously deleted and needs
/// to be restored.
/// 
/// Returns:
/// 
/// The `undelete` function returns a `Result` with the success case containing an
/// empty tuple `()` and the error case containing a `SdkError` with the specific
/// error type `ListObjectVersionsError` and the response associated with the error.
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

/// The function `get_delete_marker_versions` in Rust retrieves and returns a list
/// of version IDs for delete markers associated with a specific object key in a
/// bucket.
/// 
/// Arguments:
/// 
/// * `client`: The `client` parameter is an instance of the `Client` struct, which
/// is used to interact with the AWS S3 service. It provides methods for performing
/// operations like listing object versions in a bucket.
/// * `bucket`: The `bucket` parameter in the function `get_delete_marker_versions`
/// represents the name of the bucket in which you want to search for delete marker
/// versions. Buckets are containers for objects stored in Amazon S3 or other cloud
/// storage services. In this context, the `bucket` parameter specifies the specific
/// * `key`: The `key` parameter in the function `get_delete_marker_versions`
/// represents the object key for which you want to retrieve delete marker versions.
/// In an object storage system like Amazon S3, the key is a unique identifier for
/// an object within a bucket. It is used to locate and access the specific
/// 
/// Returns:
/// 
/// The function `get_delete_marker_versions` returns a `Result` containing a
/// `Vec<String>` of version IDs or an error of type
/// `SdkError<ListObjectVersionsError, Response>`.
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
