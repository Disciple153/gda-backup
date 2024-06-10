use assert_cmd::Command;
use std::fs;

// importing common module.
mod common;

#[test]
fn backup_test() {
    // using common code.
    common::setup();

    fs::create_dir_all(common::TEST_DIR_BACKUP).unwrap();
    fs::create_dir_all(common::TEST_DIR_RESTORE).unwrap();

    let backup_test_file_1 = "test1.txt";
    let backup_test_file_2 = "test2.txt";
    let backup_test_file_3 = "test3.txt";

    let backup_test_1 = "hello world";
    let backup_test_2 = "hello world";
    let backup_test_3 = "goodbye world";

    common::create_file(backup_test_file_1, backup_test_1);
    common::create_file(backup_test_file_2, backup_test_2);
    common::create_file(backup_test_file_3, backup_test_3);

    let mut backup = Command::cargo_bin("gda_backup").unwrap();

    let backup = backup
        .arg("backup")
        .args(&["--target-dir", common::TEST_DIR_BACKUP])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"])
        .args(&["--db-engine", "postgres"])
        .args(&["--db-user", "username"])
        .args(&["--db-password", "password"])
        .args(&["--db-host", "localhost"])
        .args(&["--db-name", "backup_state"])
        .args(&["--min-storage-duration", "1"]);

    let mut restore = Command::cargo_bin("gda_backup").unwrap();

    let restore = restore
        .arg("restore")
        .args(&["--target-dir", common::TEST_DIR_RESTORE])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"]);

    let assert_backup = backup.assert();

    assert_backup.success();
    
    let assert_restore = restore.assert();

    assert_restore.success();

    let restore_test1 = common::read_file(backup_test_file_1).unwrap();
    let restore_test2 = common::read_file(backup_test_file_2).unwrap();
    let restore_test3 = common::read_file(backup_test_file_3).unwrap();

    assert_eq!(backup_test_1, restore_test1);
    assert_eq!(backup_test_2, restore_test2);
    assert_eq!(backup_test_3, restore_test3);



}