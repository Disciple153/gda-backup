# TODO

## Road map
- Use regex to filter out files from being backed up
- Encryption
- Environment variables
    - AWS credentials
- Docker
    - image
    - compose file
- Document recommended bucket settings
- Add messages to restore and delete warning of potential charges

- Compress files to be uploaded 
- Restore specific files
- Document functions (for real)

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
