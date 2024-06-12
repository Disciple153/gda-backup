use assert_cmd::Command;
use std::{fs::{self}, path::Path};
use serial_test::serial;

// importing common module.
mod common;

#[test]
#[serial]
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
        .args(&["--db-engine", common::DB_ENGINE])
        .args(&["--postgres-user", common::POSTGRES_USER])
        .args(&["--postgres-password", common::POSTGRES_PASSWORD])
        .args(&["--postgres-host", common::POSTGRES_HOST])
        .args(&["--postgres-db", common::POSTGRES_DB])
        .args(&["--min-storage-duration", "1"]);

    let mut restore = Command::cargo_bin("gda_backup").unwrap();

    let restore = restore
        .arg("restore")
        .args(&["--target-dir", common::TEST_DIR_RESTORE])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"]);

    let assert_backup = backup.assert();
    dbg!(assert_backup.get_output());

    assert_backup.success();
    
    let assert_restore = restore.assert();
    dbg!(assert_restore.get_output());

    assert_restore.success();

    dbg!(common::build_restore_path(backup_test_file_1));
    dbg!(common::build_restore_path(backup_test_file_2));
    dbg!(common::build_restore_path(backup_test_file_3));

    let restore_test1 = common::read_file(backup_test_file_1).unwrap();
    let restore_test2 = common::read_file(backup_test_file_2).unwrap();
    let restore_test3 = common::read_file(backup_test_file_3).unwrap();

    assert_eq!(backup_test_1, restore_test1);
    assert_eq!(backup_test_2, restore_test2);
    assert_eq!(backup_test_3, restore_test3);
}


#[test]
#[serial]
fn regex_test() {
    // using common code.
    common::setup();

    fs::create_dir_all(common::TEST_DIR_BACKUP).unwrap();
    fs::create_dir_all(common::TEST_DIR_RESTORE).unwrap();

    let backup_test_file_1 = "file.txt";
    let backup_test_file_2 = "txt_file.md";

    let backup_test_1 = "hello world";
    let backup_test_2 = "hello world";

    common::create_file(backup_test_file_1, backup_test_1);
    common::create_file(backup_test_file_2, backup_test_2);

    let mut backup = Command::cargo_bin("gda_backup").unwrap();

    let backup = backup
        .arg("backup")
        .args(&["--target-dir", common::TEST_DIR_BACKUP])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"])
        .args(&["--db-engine", common::DB_ENGINE])
        .args(&["--postgres-user", common::POSTGRES_USER])
        .args(&["--postgres-password", common::POSTGRES_PASSWORD])
        .args(&["--postgres-host", common::POSTGRES_HOST])
        .args(&["--postgres-db", common::POSTGRES_DB])
        .args(&["--min-storage-duration", "1"])
        .args(&["--filter", r".txt$"]);

    let mut restore = Command::cargo_bin("gda_backup").unwrap();

    let restore = restore
        .arg("restore")
        .args(&["--target-dir", common::TEST_DIR_RESTORE])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"]);

    let assert_backup = backup.assert();
    dbg!(assert_backup.get_output());

    assert_backup.success();
    
    let assert_restore = restore.assert();
    dbg!(assert_restore.get_output());

    assert_restore.success();

    dbg!(common::build_restore_path(backup_test_file_1));
    dbg!(common::build_restore_path(backup_test_file_2));

    assert!(!Path::new(&common::build_restore_path(backup_test_file_1)).exists());
    assert!(Path::new(&common::build_restore_path(backup_test_file_2)).exists());
}

#[test]
#[serial]
fn regex_test_env() {
    // using common code.
    common::setup();

    fs::create_dir_all(common::TEST_DIR_BACKUP).unwrap();
    fs::create_dir_all(common::TEST_DIR_RESTORE).unwrap();

    let backup_test_file_1 = "file.txt";
    let backup_test_file_2 = "txt_file.md";

    let backup_test_1 = "hello world";
    let backup_test_2 = "hello world";

    common::create_file(backup_test_file_1, backup_test_1);
    common::create_file(backup_test_file_2, backup_test_2);

    let mut backup = Command::cargo_bin("gda_backup").unwrap();

    let backup = backup
        .arg("backup-with-env")
        .env("TARGET_DIR", common::TEST_DIR_BACKUP)
        .env("BUCKET_NAME", "disciple153-test")
        .env("DYNAMO_TABLE", "gda-backup-test")
        .env("DB_ENGINE", common::DB_ENGINE)
        .env("POSTGRES_USER", common::POSTGRES_USER)
        .env("POSTGRES_PASSWORD", common::POSTGRES_PASSWORD)
        .env("POSTGRES_HOST", common::POSTGRES_HOST)
        .env("POSTGRES_DB", common::POSTGRES_DB)
        .env("MIN_STORAGE_DURATION", "1")
        .env("FILTER", r"asd:\.txt$")
        .env("FILTER_DELIMITER", ":");

    let mut restore = Command::cargo_bin("gda_backup").unwrap();

    let restore = restore
        .arg("restore")
        .args(&["--target-dir", common::TEST_DIR_RESTORE])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"]);

    let assert_backup = backup.assert();
    dbg!(assert_backup.get_output());

    assert_backup.success();
    
    let assert_restore = restore.assert();
    dbg!(assert_restore.get_output());

    assert_restore.success();

    dbg!(common::build_restore_path(backup_test_file_1));
    dbg!(common::build_restore_path(backup_test_file_2));

    assert!(!Path::new(&common::build_restore_path(backup_test_file_1)).exists());
    assert!(Path::new(&common::build_restore_path(backup_test_file_2)).exists());
}

#[test]
#[serial]
fn config_file_test() {
    // using common code.
    common::setup();

    fs::create_dir_all(common::TEST_DIR_BACKUP).unwrap();
    fs::create_dir_all(common::TEST_DIR_RESTORE).unwrap();

    let backup_test_file_1 = "file.txt";
    let backup_test_file_2 = "txt_file.md";

    let backup_test_1 = "hello world";
    let backup_test_2 = "goodbye world";

    common::create_file(backup_test_file_1, backup_test_1);
    common::create_file(backup_test_file_2, backup_test_2);

    let mut backup = Command::cargo_bin("gda_backup").unwrap();

    let backup = backup
        .arg("backup-with-env")
        .arg("./tests/config.yml");

    let mut restore = Command::cargo_bin("gda_backup").unwrap();

    let restore = restore
        .arg("restore")
        .args(&["--target-dir", common::TEST_DIR_RESTORE])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"]);

    let assert_backup = backup.assert();
    dbg!(assert_backup.get_output());

    assert_backup.success();
    
    let assert_restore = restore.assert();
    dbg!(assert_restore.get_output());

    assert_restore.success();
    
    dbg!(common::build_restore_path(backup_test_file_1));
    dbg!(common::build_restore_path(backup_test_file_2));

    assert!(!Path::new(&common::build_restore_path(backup_test_file_1)).exists());
    assert!(Path::new(&common::build_restore_path(backup_test_file_2)).exists());
}

#[test]
#[serial]
fn config_file_test_dry() {
    // using common code.
    common::setup();

    fs::create_dir_all(common::TEST_DIR_BACKUP).unwrap();
    fs::create_dir_all(common::TEST_DIR_RESTORE).unwrap();

    let backup_test_file_1 = "file.txt";
    let backup_test_file_2 = "txt_file.md";

    let backup_test_1 = "hello world";
    let backup_test_2 = "goodbye world";

    common::create_file(backup_test_file_1, backup_test_1);
    common::create_file(backup_test_file_2, backup_test_2);

    let mut backup = Command::cargo_bin("gda_backup").unwrap();

    let backup = backup
        .arg("backup-with-env")
        .arg("./tests/config_dry.yml");

    let mut restore = Command::cargo_bin("gda_backup").unwrap();

    let restore = restore
        .arg("restore")
        .args(&["--target-dir", common::TEST_DIR_RESTORE])
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"]);

    let assert_backup = backup.assert();
    dbg!(assert_backup.get_output());

    assert_backup.success();
    
    let assert_restore = restore.assert();
    dbg!(assert_restore.get_output());

    assert_restore.success();

    dbg!(common::build_restore_path(backup_test_file_1));
    dbg!(common::build_restore_path(backup_test_file_2));

    assert!(!Path::new(&common::build_restore_path(backup_test_file_1)).exists());
    assert!(!Path::new(&common::build_restore_path(backup_test_file_2)).exists());
}