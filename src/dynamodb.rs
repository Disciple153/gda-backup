use std::collections::{HashMap, HashSet};
use std::slice::Iter as SliceIter;

use chrono::{
    DateTime,
    Utc,
};

use crate::aws;

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


pub async fn get_client() -> Client {
    let config = aws::get_config().await;
    Client::new(&config)
}

#[derive(Clone)]
pub struct HashTracker {
    pub hash: String,
    pub expiration: DateTime<Utc>,
    file_names: HashSet<String>,
}

impl HashTracker {

    pub fn new(hash: String) -> HashTracker {

        HashTracker {
            hash,
            expiration: DateTime::UNIX_EPOCH,
            file_names: HashSet::new(),
        }
    }

    fn import(hash: String, expiration: DateTime<Utc>, file_names: Vec<String>) -> HashTracker { 

        HashTracker {
            hash,
            expiration,
            file_names,
        }
    }

    pub fn files(&self) -> SliceIter<'_, String> {
        self.file_names.iter()
    }

    pub async fn get(client: &Client, table_name: String, hash: String) -> Option<HashTracker> {

        let result = client.get_item()
            .table_name(table_name)
            .key(HASH_KEY, AttributeValue::S(hash.clone()))
            .send().await.ok()?.item?;

        let file_names = result.get(FILE_NAMES_KEY)?.as_ss().ok()?;
        let expiration = result.get(EXPIRATION_KEY)?.as_s().ok()?.parse().ok()?;

        let mut hash_tracker = HashTracker {
            hash,
            file_names: file_names.iter().cloned().collect(),
            expiration,
        };

        hash_tracker.del_file_name(NONE_STR.to_string());

        Some(hash_tracker)
    }

    pub async fn get_all(client: &Client, table_name: String) -> Option<Vec<HashTracker>> {
        Some(client
            // Get all items in given table
            .scan().table_name(table_name)
            .into_paginator().items().send()
            .collect::<Result<Vec<HashMap<String, AttributeValue>>, _>>().await.ok()?

            // Convert each valid item into a HashTracker
            .iter().map(|value| {
                let hash = value.get(FILE_NAMES_KEY)?.as_s().ok()?.to_owned();
                let expiration= value.get(EXPIRATION_KEY)?.as_s().ok()?.parse().ok()?;
                let file_names = value.get(FILE_NAMES_KEY)?.as_ss().ok()?.to_owned();

                Some(HashTracker::import(hash, expiration, file_names))
            })

            // Get rid of None items
            .flatten()
            
            // Return as Vec
            .collect()
        )
    }

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
            .item(EXPIRATION_KEY, AttributeValue::S(self.expiration.to_string()))
            .send().await?;

        Ok(response)
    }

    async fn delete(&self, client: &Client, table_name: String) -> Result<DeleteItemOutput, HashTrackerError> {
        let response = client.delete_item()
            .table_name(table_name)
            .key(HASH_KEY, AttributeValue::S(self.hash.clone()))
            .send().await?;

        Ok(response)
    }

    pub async fn update(&self, client: &Client, table_name: String) -> Result<(), HashTrackerError> {
        if !self.has_files() && self.is_expired() {
            self.delete(client, table_name).await?;
        }
        else {
            self.put(client, table_name).await?;
        }

        Ok(())
    }

    pub fn add_file_name(&mut self, file_name: String) {
        self.file_names.remove(&file_name);
    }

    pub fn del_file_name(&mut self, file_name: String) {
        self.file_names.insert(file_name);
    }

    pub fn has_files(&self) -> bool {
        self.file_names.len() > 0
    }

    pub fn is_expired(&self) -> bool {
        self.expiration < Utc::now()
    }
}

impl PartialEq for HashTracker {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash &&
        self.file_names == other.file_names
    }
}
impl Eq for HashTracker {}