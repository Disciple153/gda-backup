use std::collections::hash_set::Iter as SetIter;
use std::collections::{HashMap, HashSet};

use chrono::{
    DateTime, Utc
};

use crate::aws;
use crate::environment::AwsArgs;

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::operation::get_item::GetItemError;
use aws_sdk_dynamodb::operation::scan::ScanError;
use aws_sdk_dynamodb::operation::put_item::{PutItemError, PutItemOutput};
use aws_sdk_dynamodb::operation::delete_item::{DeleteItemError, DeleteItemOutput};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::http::Response;

use thiserror::Error;

const HASH_KEY: &str = "hash";
const FILE_NAMES_KEY: &str = "file_names";
const EXPIRATION_KEY: &str = "expiration";
const NONE_STR: &str = "NONE";

#[derive(Error, Debug)]
pub enum HashTrackerError {
    #[error("DynamoDbSdkErrorGet")]
    DynamoDbSdkErrorGet(#[from] SdkError<GetItemError, Response>),

    #[error("DynamoDbSdkErrorScan")]
    DynamoDbSdkErrorScan(#[from] SdkError<ScanError, Response>),

    #[error("DynamoDbSdkErrorPut")]
    DynamoDbSdkErrorPut(#[from] SdkError<PutItemError, Response>),

    #[error("DynamoDbSdkErrorDelete")]
    DynamoDbSdkErrorDelete(#[from] SdkError<DeleteItemError, Response>),

    #[error("DynamoDbGetItemError")]
    DynamoDbGetItemError(String),
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

/// The `HashTracker` struct in Rust represents a data structure that tracks a hash
/// value, expiration date, and a set of file names.
/// 
/// Properties:
/// 
/// * `hash`: The `hash` property in the `HashTracker` struct is of type `String`.
/// It is used to store a hash value.
/// * `expiration`: The `expiration` property in the `HashTracker` struct represents
/// the date and time when the hash value will expire. It is of type
/// `DateTime<Utc>`, which is a datetime type provided by the `chrono` crate that
/// represents a specific point in time with a timezone of UTC.
/// * `file_names`: The `file_names` property in the `HashTracker` struct is a
/// private field of type `HashSet<String>`. This field is not accessible outside
/// the struct and can only be accessed or modified through the struct's methods.
#[derive(Clone, Debug)]
pub struct HashTracker {
    pub hash: String,
    pub expiration: DateTime<Utc>,
    file_names: HashSet<String>,
}

impl PartialEq for HashTracker {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash &&
        self.file_names == other.file_names
    }
}
impl Eq for HashTracker {}

impl HashTracker {

    /// The function `new` creates a new `HashTracker` instance with the provided
    /// hash, expiration datetime, and an empty set of file names.
    /// 
    /// Arguments:
    /// 
    /// * `hash`: The `hash` parameter is a String that represents a hash value. It
    /// is typically used to uniquely identify data or files by generating a
    /// fixed-size string of characters based on the content of the data.
    /// * `expiration`: The `expiration` parameter in the `new` function is of type
    /// `DateTime<Utc>`. This type represents a specific point in time in the UTC
    /// timezone.
    /// 
    /// Returns:
    /// 
    /// A new instance of the `HashTracker` struct is being returned.
    pub fn new(hash: String, expiration: DateTime<Utc>) -> HashTracker {

        HashTracker {
            hash,
            expiration,
            file_names: HashSet::new(),
        }
    }

    /// The function `import` creates a `HashTracker` object with a given hash,
    /// expiration date, and file names, removing any occurrences of a specific file
    /// name.
    /// 
    /// Arguments:
    /// 
    /// * `hash`: The `hash` parameter is a string that represents a unique
    /// identifier for the imported data.
    /// * `expiration`: The `expiration` parameter in the `import` function
    /// represents the date and time when the hash value will expire. It is of type
    /// `DateTime<Utc>`, which is a datetime type that is timezone-aware and
    /// represents a specific point in time in the UTC timezone.
    /// * `file_names`: The `file_names` parameter in the `import` function is a
    /// vector of strings that contains the names of files to be imported.
    /// 
    /// Returns:
    /// 
    /// a `HashTracker` struct after creating an instance of it and removing a file
    /// name with the value `NONE_STR`.
    fn import(hash: String, expiration: DateTime<Utc>, file_names: Vec<String>) -> HashTracker { 

        let mut hash_tracker = HashTracker {
            hash,
            expiration,
            file_names: file_names.iter().cloned().collect(),
        };

        hash_tracker.del_file_name(NONE_STR.to_string());

        hash_tracker
    }

    /// The function `files` returns an iterator over the file names stored in a
    /// HashTracker.
    /// 
    /// Returns:
    /// 
    /// A `SetIter` iterator over references to strings in the `file_names` set is
    /// being returned.
    pub fn files(&self) -> SetIter<'_, String> {
        self.file_names.iter()
    }

    /// This Rust function retrieves an item from a table using a hash key and
    /// constructs a HashTracker object from the retrieved data.
    /// 
    /// Arguments:
    /// 
    /// * `client`: The `client` parameter is an instance of the `Client` struct,
    /// which is used to interact with a database or service. In this case, it is
    /// likely being used to make a request to retrieve an item from a table in a
    /// database.
    /// * `table_name`: The `table_name` parameter is a String that represents the
    /// name of the table from which you want to retrieve an item.
    /// * `hash`: The `hash` parameter in the code snippet represents a unique
    /// identifier used to retrieve data from a table in a database. It is passed as
    /// a String and is used as a key to fetch specific information related to that
    /// hash from the database table.
    /// 
    /// Returns:
    /// 
    /// The function `get` returns an `Option` containing a `HashTracker` struct.
    pub async fn get(aws_args: AwsArgs, client: &Client, hash: String) -> Option<HashTracker> {

        let result = client.get_item()
            .table_name(aws_args.dynamo_table)
            .key(HASH_KEY, AttributeValue::S(hash.clone()))
            .send().await.ok()?.item?;

        let file_names = result.get(FILE_NAMES_KEY)?.as_ss().ok()?;

    
        // Create a normal DateTime from the NaiveDateTime
        let seconds = result.get(EXPIRATION_KEY)?.as_n().ok()?.parse().ok()?;
        let expiration: DateTime<Utc> = DateTime::from_timestamp(seconds, 0)?;

        let mut hash_tracker = HashTracker::import (
            hash,
            expiration,
            file_names.clone(),
        );

        hash_tracker.del_file_name(NONE_STR.to_string());

        Some(hash_tracker)
    }

    /// The `pub async fn get_all` function in the provided Rust code snippet is
    /// responsible for retrieving all items from a specified table in a DynamoDB
    /// database and converting them into a collection of `HashTracker` instances.
    /// Here is a breakdown of what the function is doing:
    pub async fn get_all(client: &Client, aws_args: AwsArgs) -> Option<Vec<HashTracker>> {
        Some(client
            // Get all items in given table
            .scan().table_name(aws_args.dynamo_table)
            .into_paginator().items().send()
            .collect::<Result<Vec<HashMap<String, AttributeValue>>, _>>().await.ok()?

            // Convert each valid item into a HashTracker
            .iter().map(|value| {
                let hash = value.get(HASH_KEY)?.as_s().ok()?.to_owned();
                let seconds = value.get(EXPIRATION_KEY)?.as_n().ok()?.parse().ok()?;
                let expiration: DateTime<Utc> = DateTime::from_timestamp(seconds, 0)?;
                let file_names = value.get(FILE_NAMES_KEY)?.as_ss().ok()?.to_owned();

                Some(HashTracker::import(hash, expiration, file_names))
            })

            // Get rid of None items
            .flatten()
            
            // Return as Vec
            .collect()
        )
    }

    /// The function `put` in Rust asynchronously puts an item into a table with
    /// specified attributes and returns the result.
    /// 
    /// Arguments:
    /// 
    /// * `client`: The `client` parameter is an instance of the `Client` struct,
    /// which is used to interact with a database or service. In this context, it is
    /// likely being used to make a PUT request to store an item in a table.
    /// * `table_name`: The `table_name` parameter in the `put` function represents
    /// the name of the table in which you want to put an item. This function is
    /// responsible for putting an item into a DynamoDB table using the AWS SDK for
    /// Rust.
    /// 
    /// Returns:
    /// 
    /// The `put` function returns a `Result` containing either a `PutItemOutput` or
    /// a `HashTrackerError`.
    async fn put(&self, client: &Client, table_name: String) -> Result<PutItemOutput, HashTrackerError> {

        let file_names: Vec<String>;

        if self.has_files() {
            file_names = self.file_names.iter().cloned().collect::<Vec<_>>();
        }
        else {
            file_names = vec![NONE_STR.to_string()];
        }

        let response = client.put_item()
            .table_name(table_name)
            .item(HASH_KEY, AttributeValue::S(self.hash.clone()))
            .item(FILE_NAMES_KEY, AttributeValue::Ss(file_names))
            .item(EXPIRATION_KEY, AttributeValue::N(self.expiration.timestamp().to_string()))
            .send().await?;

        Ok(response)
    }

    /// The function `delete` in Rust asynchronously deletes an item from a table
    /// using a provided client and table name.
    /// 
    /// Arguments:
    /// 
    /// * `client`: The `client` parameter is an instance of the `Client` struct,
    /// which is used to interact with the AWS DynamoDB service. It is passed as a
    /// reference to the `delete` function to perform the delete operation on a
    /// specific table in DynamoDB.
    /// * `table_name`: The `table_name` parameter in the `delete` function
    /// represents the name of the table from which you want to delete an item.
    /// 
    /// Returns:
    /// 
    /// The `delete` function returns a `Result` containing either a
    /// `DeleteItemOutput` on success or a `HashTrackerError` on failure.
    async fn delete(&self, client: &Client, table_name: String) -> Result<DeleteItemOutput, HashTrackerError> {
        let response = client.delete_item()
            .table_name(table_name)
            .key(HASH_KEY, AttributeValue::S(self.hash.clone()))
            .send().await?;

        Ok(response)
    }

    /// The `update` function in Rust asynchronously updates a table by either
    /// deleting expired files or putting new files based on certain conditions.
    /// 
    /// Arguments:
    /// 
    /// * `client`: The `client` parameter in the `update` function is of type
    /// `&Client`, which is a reference to an instance of the `Client` struct. This
    /// parameter is used to interact with some external service or resource, such
    /// as a database client, HTTP client, etc.
    /// * `table_name`: The `table_name` parameter in the `update` function
    /// represents the name of the table in which the data will be updated or
    /// modified. It is a String type that is passed as an argument to the function.
    /// 
    /// Returns:
    /// 
    /// The `update` function is returning a `Result<(), HashTrackerError>`.
    pub async fn update(&self, aws_args: AwsArgs, client: &Client) -> Result<(), HashTrackerError> {
        if !self.has_files() && self.is_expired() {
            self.delete(client, aws_args.dynamo_table).await?;
        }
        else {
            self.put(client, aws_args.dynamo_table).await?;
        }

        Ok(())
    }

    /// The function `add_file_name` inserts a file name into a set.
    /// 
    /// Arguments:
    /// 
    /// * `file_name`: The `file_name` parameter is a `String` type that represents
    /// the name of a file to be added to a collection or set within the `self`
    /// object.
    pub fn add_file_name(&mut self, file_name: String) {
        self.file_names.insert(file_name);
    }

    /// The function `del_file_name` removes a file name from a collection in Rust.
    /// 
    /// Arguments:
    /// 
    /// * `file_name`: The `file_name` parameter in the `del_file_name` function is a
    /// `String` type that represents the name of the file to be deleted from the
    /// list of file names stored in the data structure managed by the `self` object.
    pub fn del_file_name(&mut self, file_name: String) {
        self.file_names.remove(&file_name);
    }

    /// The function `has_files` checks if a Rust struct has any file names
    /// associated with it.
    /// 
    /// Returns:
    /// 
    /// The `has_files` function returns a boolean value indicating whether the
    /// `file_names` vector in the current object has any elements. If the length of
    /// the `file_names` vector is greater than 0, the function returns `true`,
    /// indicating that there are files. Otherwise, it returns `false`, indicating
    /// that there are no files.
    pub fn has_files(&self) -> bool {
        self.file_names.len() > 0
    }

    /// The function `is_expired` checks if the expiration time is before the
    /// current time.
    /// 
    /// Returns:
    /// 
    /// A boolean value indicating whether the expiration time of the object is
    /// before the current time.
    pub fn is_expired(&self) -> bool {
        self.expiration < Utc::now()
    }

    /// The function permanently deletes all hash trackers using AWS DynamoDB.
    /// 
    /// Arguments:
    /// 
    /// * `aws_args`: The `aws_args` parameter is of type `AwsArgs` and contains the
    /// necessary arguments for interacting with AWS services. It likely includes
    /// information such as AWS credentials, region, and other configurations
    /// required for accessing AWS resources.
    /// * `client`: The `client` parameter in the function `permanently_delete_all`
    /// is likely an HTTP client or a database client that is used to make requests
    /// to a server or database. It is passed as a reference to the function,
    /// indicating that the function will use this client to interact with some
    /// external service
    /// 
    /// Returns:
    /// 
    /// The function `permanently_delete_all` returns a `Result` with the success
    /// type `()` (unit type) and the error type `HashTrackerError`.
    pub async fn permanently_delete_all(aws_args: AwsArgs, client: &Client) -> Result<(), HashTrackerError> {
        let hash_trackers = HashTracker::get_all(client, aws_args.clone()).await.unwrap();

        for hash_tracker in hash_trackers {
            let _ = hash_tracker.delete(client, aws_args.dynamo_table.clone()).await?;
        }

        Ok(())
    }
}