use assert_cmd::Command;
use std::{env, fs};
use std::io::{Error, Write};

pub const TEST_DIR: &str = "./test_dir/";
pub const TEST_DIR_BACKUP: &str = "./test_dir/backup/";
pub const TEST_DIR_RESTORE: &str = "./test_dir/restore/";

pub fn setup() {

    let _ = fs::remove_dir_all(TEST_DIR);

    let mut clear_local_db = Command::cargo_bin("gda_backup").unwrap();
    let assert = clear_local_db
        .arg("clear-database")
        .args(&["--db-engine", "postgres"])
        .args(&["--db-user", "username"])
        .args(&["--db-password", "password"])
        .args(&["--db-host", "localhost"])
        .args(&["--db-name", "backup_state"])
        .assert();

    assert.success();

    let mut delete_backup = Command::cargo_bin("gda_backup").unwrap();
    let assert = delete_backup
        .arg("delete-backup")
        .args(&["--bucket-name", "disciple153-test"])
        .args(&["--dynamo-table", "gda-backup-test"])
        .write_stdin("y")
        .assert();

    assert.success();
}

pub fn get_pwd() -> Result<String, Error> {
    Ok(env::current_dir().unwrap().to_str().unwrap().to_string())
}

pub fn create_file(file_name: &str, contents: &str) {
    let mut file = fs::File::create(TEST_DIR_BACKUP.to_owned() + &file_name).unwrap();
    file.write(contents.as_bytes()).unwrap();
}

pub fn build_restore_path(file_name: &str) -> String {
    let pwd = get_pwd().unwrap();

    let restore_dir = match TEST_DIR_RESTORE.strip_suffix("/") {
        Some(s) => s.to_owned(),
        None => TEST_DIR_RESTORE.to_owned()
    };

    let backup_dir = match TEST_DIR_BACKUP.strip_prefix(".") {
        Some(s) => s.to_owned(),
        None => TEST_DIR_BACKUP.to_owned()
    };

    restore_dir + &pwd + &backup_dir + file_name
}

pub fn read_file(file_name: &str) -> Result<String, Error> {
    fs::read_to_string(build_restore_path(file_name))
}