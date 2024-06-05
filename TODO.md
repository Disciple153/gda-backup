# TODO

## Road map
- Document functions
- Use regex to filter out files from being backed up
- Encryption
- Environment variables
    - AWS credentials
- Docker
    - image
    - compose file
- Document recommended bucket settings

- Supporting moving files
    - Use DynamoDB to store map from hash to filename.
    - Files are stored in S3, named by their hash.
    - If local db is gone, replace it using DynamoDB.
    - Only check hash if modified dates do not match.
    - Only upload/update if hash has changed.
    - If hash has not changed, but filename has, update DynamoDB.
    - Make sure Dynamo is the cheapest key-value store.
    - Handle multiple copies of one file which share hash
    - Support undeleting files with hashes that came back?
    - Verify that uploaded files and db entries refer to the same file
- Compress files to be uploaded 

## Stretch
- Use hashes instead of file names

## Hashing

### DB Structure

```json
hash: {
    "file_names": [ "file path" ],
    "expiration": "time"
}
```

### Tasks

- create min_storage_duration argument
- get hashes of possible files
- upserting:
    - if hash in db: 
        - if dynamo.hash.file_names.len == 0:
            - if not expired:
                - remove delete marker
                - on failure, upload file
            - else:
                - upload file
        - move filenames from dynamo.old_hash.file_names to dynamo.hash.file_names
        - if dynamo.old_hash.file_names.len == 0
            - delete from s3
    - else:
        - upload file
        - create new hash entry
            - dynamo.hash.file_names = [ filename ]
            - dynamo.hash.expiration = now + min_storage_duration
- deleting:
    - remove filename from dynamo.old_hash.file_names
    - if dynamo.file_names.len == 0
        - delete from s3
    - on delete failure
        - File not found
            - delete local entry
            - delete all filenames from dynamo entry
        - other
            - continue
- clean up: 
    - delete all entries with:
        - dynamo.hash.file_names.len == 0 and 
        - dynamo.hash.expiration < now

## DynamoDB
- key:
    - hash
- Table class:
    - start with Standard
    - switch to Standard-IA after initial upload
- Read/write capacity settings:
    - On-demand
- Deletion protection:
    - on