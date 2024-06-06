use chrono::{
    DateTime,
    Utc,
};
use ordered_vec::OrdVec;

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
const NONE_STR: &str = "NONE";

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
    pub expiration: DateTime<Utc>,
    file_names: Vec<String>,
}

impl HashTracker {

    pub fn new(hash: String, expiration: DateTime<Utc>, file_name: String) -> HashTracker { 

        HashTracker {
            hash,
            expiration,
            file_names: vec![file_name],
        }
    }

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

        let mut hash_tracker = HashTracker {
            hash,
            file_names: file_names.clone(),
            expiration,
        };

        hash_tracker.del_file_name(NONE_STR.to_string());

        Ok(hash_tracker)
    }

    pub async fn put(&self, client: &Client, table_name: String) -> Result<PutItemOutput, HashTrackerError> {

        let file_names;

        if self.has_files() {
            file_names = self.file_names.clone();
        }
        else {
            file_names = vec![NONE_STR.to_string()];
        }

        dbg!(self.file_names.clone());

        let response = client.put_item()
            .table_name(table_name)
            .item(HASH_KEY, AttributeValue::S(self.hash.clone()))
            .item(FILE_NAMES_KEY, AttributeValue::Ss(file_names))
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

    pub fn add_file_name(&mut self, file_name: String) {
        
        match self.file_names.iter().position(|x| *x == file_name) {

            // If file_name is already in file_names
            Some(_) => (),

            // If file_name is not in file_names
            None => {
                let _ = self.file_names.push_ord_ascending(file_name);
            },
        };
    }

    pub fn del_file_name(&mut self, file_name: String) {
        match self.file_names.iter().position(|x| *x == file_name) {

            // If file_name is already in file_names
            Some(index) => {
                self.file_names.remove(index);
            },

            // If file_name is not in file_names
            None => (),
        };
    }

    pub fn has_files(&self) -> bool {
        self.file_names.len() > 0
    }
}