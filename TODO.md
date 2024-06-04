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
    "current": [ "file path" ],
    "expiration": "time"
}
```

### Tasks

- create object_life argument
- get hashes of possible files
- upserting:
    - if hash in db: 
        - if dynamo.hash.current.len == 0:
            - add filename to dynamo.hash.current
            - if not expired:
                - remove delete marker
            - else:
                - upload file
        - else:
            - add filename to dynamo.hash.current
        - remove filename from dynamo.old_hash.current
        - if dynamo.current.len == 0
            - delete from s3
    - else:
        - create new hash entry
            - dynamo.hash.current = [ filename ]
            - dynamo.hash.expiration = now + object_life
        - upload to dynamodb
- deleting:
    - remove filename from dynamo.old_hash.current
    - if dynamo.current.len == 0
        - delete from s3
- clean up: 
    - delete all entries with:
        - dynamo.hash.current.len == 0 and 
        - dynamo.hash.expiration < now