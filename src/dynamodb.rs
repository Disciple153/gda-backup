use chrono::{
    DateTime,
    Utc,
};

use crate::aws;

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::operation::get_item::GetItemError;
use aws_sdk_dynamodb::operation::put_item::{PutItemError, PutItemOutput};
use aws_sdk_dynamodb::operation::delete_item::{DeleteItemError, DeleteItemOutput};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::http::Response;

use thiserror::Error;

const HASH_KEY: &str = "hash";
const FILE_NAMES_KEY: &str = "file_names";
const EXPIRATION_KEY: &str = "expiration";

#[derive(Error, Debug)]
pub enum HashTrackerError {
    #[error("DynamoDbSdkErrorGet")]
    DynamoDbSdkErrorGet(#[from] SdkError<GetItemError, Response>),

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
    pub file_names: Vec<String>,
    pub expiration: DateTime<Utc>,
}

impl HashTracker {
    pub async fn get(client: &Client, table_name: String, hash: String) -> Result<HashTracker, HashTrackerError> {
        let empty_vec = vec![];

        let result = client.get_item()
            .table_name(table_name)
            .key(HASH_KEY, AttributeValue::S(hash.clone()))
            .send().await?;

        let map = match result.item {
            Some(m) => m,
            None => return Err(HashTrackerError::DynamoDbGetItemError("Error getting HashTracker value".to_string())),
        };

        let file_names = map.get(FILE_NAMES_KEY)
            .unwrap_or(&AttributeValue::Null(true))
            .as_ss().unwrap_or(&empty_vec);

        let expiration = match map.get(EXPIRATION_KEY).unwrap_or(&AttributeValue::Null(true)).as_s() {
            Ok(expiration_string) => expiration_string.parse().unwrap_or(DateTime::UNIX_EPOCH),
            Err(_) => DateTime::UNIX_EPOCH,
        };

        let hash_tracker = HashTracker {
            hash,
            file_names: file_names.clone(),
            expiration,
        };

        Ok(hash_tracker)
    }

    pub async fn put(&self, client: &Client, table_name: String) -> Result<PutItemOutput, HashTrackerError> {
        let response = client.put_item()
            .table_name(table_name)
            .item(HASH_KEY, AttributeValue::S(self.hash.clone()))
            .item(FILE_NAMES_KEY, AttributeValue::Ss(self.file_names.clone()))
            .item(EXPIRATION_KEY, AttributeValue::S(self.expiration.to_string()))
            .send().await?;

        Ok(response)
    }

    pub async fn delete(&self, client: &Client, table_name: String) -> Result<DeleteItemOutput, HashTrackerError> {
        let response = client.delete_item()
            .table_name(table_name)
            .key(HASH_KEY, AttributeValue::S(self.hash.clone()))
            .send().await?;

        Ok(response)
    }

    pub async fn move_filename_from(&mut self, client: &Client, table_name: String, file_name: String, old_hash: Option<String>) -> Result<bool, HashTrackerError> {
        
        self.file_names.push(file_name.clone());
        self.put(client, table_name.clone()).await?;

        let Some(old_hash) = old_hash else { return Ok(false) };
        let Ok(mut old) = HashTracker::get(client, table_name.clone(), old_hash).await else { return Ok(false) };
        let Some(index) = old.file_names.iter().position(|x| *x == file_name) else { return Ok(false) };

        old.file_names.remove(index);

        if old.file_names.len() == 0 && old.expiration < Utc::now() {
            let _ = old.delete(client, table_name.clone()).await;
        }
        else {
            let _ = old.put(client, table_name.clone()).await;
        };

        Ok(old.file_names.len() == 0)
    }
}