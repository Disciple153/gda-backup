use std::collections::hash_set::Iter as SetIter;
use std::{
    collections::HashMap,
    fs::{
        self,
        File,
        create_dir_all
    }, 
    io::{
        Error as IoError,
        Write,
        ErrorKind
    },
    path::Path,
    time::SystemTime,
};
use aws_sdk_s3::operation::list_object_versions::ListObjectVersionsError;
use aws_sdk_s3::operation::upload_part::UploadPartError;
use aws_sdk_s3::types::{
    CompletedMultipartUpload,
    CompletedPart,
    Delete,
    ObjectIdentifier
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
        }, put_object::PutObjectError,
        restore_object::{
            RestoreObjectError,
            RestoreObjectOutput
        }
    },
    primitives::ByteStream,
    Client,
};
use aws_smithy_runtime_api::http::Response;
use aws_sdk_s3::primitives::SdkBody;
use aws_smithy_types::byte_stream::error::Error as AwsSmithyError;
use aws_smithy_runtime_api::client::result::SdkError as AwsSmithySdkError;
use aws_smithy_types::byte_stream::Length;

use crate::aws;
use crate::environment::{
    AwsArgs,
    Cli
};
use thiserror::Error;

use aws_sdk_s3::error::BuildError;
use aws_sdk_s3::operation::delete_objects::DeleteObjectsError;
use log::info;

// Use multipart upload if file is greater than 100 Mib
const MULTIPART_UPLOAD_THRESHOLD: u64 = 1024 * 1024 * 100;
//In bytes, minimum chunk size of 5MiB. Increase CHUNK_SIZE to send larger chunks.
const MIN_CHUNK_SIZE: u64 = 1024 * 1024 * 5;
const MAX_CHUNKS: u64 = 10000;
//Set max S3 object size to 5TiB
const MAX_S3_OBJECT_SIZE: u64 = 1024 * 1024 * 1024 * 1024 * 5;

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

#[derive(Error, Debug)]
pub enum S3DeleteError {
    #[error("S3ListObjectsError")]
    S3ListObjectsError(#[from] SdkError<ListObjectVersionsError, Response>),

    #[error("S3DeleteObjectsError")]
    S3DeleteObjectsError(#[from] SdkError<DeleteObjectsError, Response>),

    #[error("S3BuildError")]
    S3BuildError(#[from] BuildError),
}

#[derive(Error, Debug)]
pub enum S3PutError {
    #[error("S3PutObjectError")]
    S3PutObjectError(#[from] AwsSmithySdkError<PutObjectError, Response>),

    #[error("PutError")]
    PutError(#[from] IoError),

    #[error("S3PutUploadPartError")]
    S3PutUploadPartError(#[from] AwsSmithySdkError<UploadPartError, Response>),
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
pub async fn put(aws_args: AwsArgs, client: &Client, key: String, file_path: String) -> Result<(), S3PutError> {

    let file_size = tokio::fs::metadata(file_path.clone()).await?.len();

    if file_size > MULTIPART_UPLOAD_THRESHOLD {
        return put_multipart(aws_args, client, key, file_path.clone(), file_size).await;
    }

    let body = ByteStream::from_path(Path::new(&file_path)).await;

    client
        .put_object()
        .bucket(aws_args.bucket_name)
        .key(key)
        .body(body.unwrap())
        .send()
        .await?;

    Ok(())
}


/// The function `put_multipart` in Rust uploads a file in multiple parts to an AWS
/// S3 bucket.
/// 
/// Arguments:
/// 
/// * `aws_args`: - `AwsArgs`: A struct containing AWS credentials and bucket name.
/// * `client`: The `client` parameter in the function `put_multipart` is an
/// instance of the AWS S3 client that is used to interact with the AWS S3 service.
/// It is used to perform operations like creating a multipart upload, uploading
/// parts of a file, and completing the multipart upload.
/// * `key`: The `key` parameter in the `put_multipart` function represents the
/// unique identifier or name of the object you are uploading to the S3 bucket. It
/// is typically a string that specifies the path or name under which the object
/// will be stored in the bucket.
/// * `file_path`: The `file_path` parameter in the `put_multipart` function
/// represents the path to the file that you want to upload to an AWS S3 bucket in
/// multiple parts. It is a string that specifies the location of the file on your
/// local system. For example, it could be something like "/
/// * `file_size`: The `file_size` parameter in the `put_multipart` function
/// represents the size of the file that is being uploaded in bytes. It is used to
/// determine the number of chunks the file will be split into for multipart
/// uploading.
/// 
/// Returns:
/// 
/// The `put_multipart` function returns a `Result<(), S3PutError>`. This means that
/// it returns a `Result` type where the success case contains an empty tuple `()`
/// and the error case contains an `S3PutError`.
/// 
/// https://github.com/awsdocs/aws-doc-sdk-examples/blob/main/rustv1/examples/s3/src/bin/s3-multipart-upload.rs#L136
pub async fn put_multipart(aws_args: AwsArgs, client: &Client, key: String, file_path: String, file_size: u64) -> Result<(), S3PutError> {

    let multipart_upload_res = client
        .create_multipart_upload()
        .bucket(&aws_args.bucket_name)
        .key(&key)
        .send()
        .await
        .unwrap();

    let upload_id = multipart_upload_res.upload_id().unwrap();

    let mut chunk_size = MIN_CHUNK_SIZE;
    let mut chunk_count = MAX_CHUNKS + 1;
    let mut size_of_last_chunk= 0;

    while chunk_count > MAX_CHUNKS {

        chunk_count = (file_size / MIN_CHUNK_SIZE) + 1;
        size_of_last_chunk = file_size % MIN_CHUNK_SIZE;
        if size_of_last_chunk == 0 {
            size_of_last_chunk = MIN_CHUNK_SIZE;
            chunk_count -= 1;
        };
    
        if chunk_count * chunk_size > MAX_S3_OBJECT_SIZE {
            Err(
                IoError::new(ErrorKind::InvalidData, 
                format!("File too large. Max S3 object size is 5TiB: {}", file_path.clone()))
            )?;
        };

        if chunk_count > MAX_CHUNKS {
            chunk_size *= 2;
        };
    };

    let mut upload_parts: Vec<CompletedPart> = Vec::new();

    for chunk_index in 0..chunk_count {
        let this_chunk = if chunk_count - 1 == chunk_index {
            size_of_last_chunk
        } else {
            MIN_CHUNK_SIZE
        };
        let stream = ByteStream::read_from()
            .path(file_path.clone())
            .offset(chunk_index * MIN_CHUNK_SIZE)
            .length(Length::Exact(this_chunk))
            .build()
            .await
            .unwrap();
        //Chunk index needs to start at 0, but part numbers start at 1.
        let part_number = (chunk_index as i32) + 1;
        // snippet-start:[rust.example_code.s3.upload_part]
        let upload_part_res = client
            .upload_part()
            .key(&key)
            .bucket(&aws_args.bucket_name)
            .upload_id(upload_id)
            .body(stream)
            .part_number(part_number)
            .send()
            .await?;
        upload_parts.push(
            CompletedPart::builder()
                .e_tag(upload_part_res.e_tag.unwrap_or_default())
                .part_number(part_number)
                .build(),
        );
    };

    let completed_multipart_upload = CompletedMultipartUpload::builder()
        .set_parts(Some(upload_parts))
        .build();

    let _complete_multipart_upload_res = client
        .complete_multipart_upload()
        .bucket(aws_args.bucket_name)
        .key(key)
        .multipart_upload(completed_multipart_upload)
        .upload_id(upload_id)
        .send()
        .await
        .unwrap();

    Ok(())
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
pub async fn delete(aws_args: AwsArgs, client: &Client, key: String) -> Result<DeleteObjectOutput, SdkError<DeleteObjectError>> {

    client
        .delete_object()
        .bucket(aws_args.bucket_name)
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
pub async fn restore(aws_args: AwsArgs, client: &Client, key: String) -> Result<RestoreObjectOutput, SdkError<RestoreObjectError, Response>> {
    client.restore_object()
        .bucket(aws_args.bucket_name)
        .key(key)
        .send()
        .await
}

pub async fn get_object<'a>(cli: Cli, aws_args: AwsArgs, client: &Client, key: String, prefix: String, file_paths: SetIter<'a, String>) -> Result<Vec<String>, S3GetError> {

    if file_paths.len() == 0 {
        return Ok(vec![]);
    }
    
    let files: Vec<String> = file_paths.map(|s| s.clone()).collect();
    
    let first_file = prefix.clone() + &files[0];
    let (first_dir, _) = first_file.rsplit_once('/').unwrap();
    
    create_dir_all(first_dir)?;
    let mut file = File::create(first_file.clone())?;

    if cli.dry_run {
        return Ok(files);
    }
    
    let mut object = client
        .get_object()
        .bucket(aws_args.bucket_name)
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
pub async fn list(client: &Client, aws_args: AwsArgs) -> Result<HashMap<String, SystemTime>, SdkError<ListObjectsV2Error>> {

    let mut output = HashMap::new();

    let _ = client
        .list_objects_v2()
        .bucket(aws_args.bucket_name)
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

pub async fn permanently_delete_all(client: &Client, aws_args: AwsArgs) -> Result<(), S3DeleteError> {
    let versions = client.list_object_versions()
        .bucket(aws_args.bucket_name.clone())
        .send().await?;

    let delete_objects: Vec<ObjectIdentifier> = versions.versions().iter().flat_map(|version| {
        ObjectIdentifier::builder()
            .set_key(version.key.clone())
            .set_version_id(version.version_id.clone())
            .build()
    }).collect();

    if delete_objects.is_empty() {
        info!("No objects found to delete.");
        return Ok(());
    }

    client
        .delete_objects()
        .bucket(aws_args.bucket_name.clone())
        .delete(Delete::builder().set_objects(Some(delete_objects)).build()?)
        .send()
        .await?;

    Ok(())
}
