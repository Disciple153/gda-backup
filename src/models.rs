use diesel::prelude::*;
use diesel::result::Error;
use std::time::SystemTime;
use crate::schema::glacier_state::dsl::*;
use crate::schema::local_state::dsl::*;

use crate::schema::glacier_state::dsl::file_path as glacier_file_path;

/// The `GlacierFile` struct represents a file with its path, hash, and modification
/// time in a Rust application using Diesel ORM.
/// 
/// Properties:
/// 
/// * `file_path`: The `file_path` property in the `GlacierFile` struct represents
/// the path to the file in the file system. It is of type `String` and stores the
/// file path as a string value.
/// * `file_hash`: The `file_hash` property in the `GlacierFile` struct is of type
/// `Option<String>`. This means that it can either contain a `String` value or be
/// `None`. It is used to store the hash value of the file, which can be useful for
/// verifying the integrity of
/// * `modified`: The `modified` field in the `GlacierFile` struct represents the
/// last modified timestamp of the file. It is of type `SystemTime`, which is a
/// struct representing a point in time. This field is used to store the timestamp
/// when the file was last modified.
#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::glacier_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Clone, Debug)]
pub struct GlacierFile {
    pub file_path: String,
    pub file_hash: Option<String>,
    pub modified: SystemTime,
}

/// The `LocalFile` struct represents a file with its path and modification time in
/// Rust.
/// 
/// Properties:
/// 
/// * `file_path`: The `file_path` property in the `LocalFile` struct represents the
/// path to a local file. It is of type `String` and stores the file path as a
/// string value.
/// * `modified`: The `modified` field in the `LocalFile` struct represents the last
/// modified timestamp of the file. It is of type `SystemTime`, which is a struct
/// representing a point in time. This field will store the timestamp when the file
/// was last modified.
#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::local_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Clone, Debug)]
pub struct LocalFile {
    pub file_path: String,
    pub modified: SystemTime,
}

impl LocalFile {

    /// The function inserts a record into a PostgreSQL database table using Diesel
    /// ORM in Rust and returns the inserted record.
    /// 
    /// Arguments:
    /// 
    /// * `conn`: The `conn` parameter in the `insert` function is a mutable
    /// reference to a `PgConnection` object. This object represents a connection to
    /// a PostgreSQL database and is used to execute database operations like
    /// inserting data into tables.
    /// 
    /// Returns:
    /// 
    /// The `insert` function is returning a `Result` containing either a
    /// `LocalFile` or an `Error`.
    pub fn insert(&self, conn: &mut PgConnection) -> Result<LocalFile, Error> {
        diesel::insert_into(local_state)
            .values(self)
            .returning(LocalFile::as_returning())
            .get_result(conn)
    }

    /// The function deletes a record from a PostgreSQL database table based on the
    /// file path provided.
    /// 
    /// Arguments:
    /// 
    /// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`
    /// object, which represents a connection to a PostgreSQL database. This
    /// parameter is used to execute the delete operation on the database table.
    /// 
    /// Returns:
    /// 
    /// The `delete` function is returning a `Result` enum with the success type
    /// `usize` (indicating the number of rows affected) and the error type
    /// `diesel::result::Error`.
    pub fn delete(&self, conn: &mut PgConnection) -> Result<usize, diesel::result::Error> {
        diesel::delete(local_state.find(&self.file_path))
            .filter(crate::schema::local_state::dsl::file_path.eq(&self.file_path))
            .execute(conn)
    }
}

impl GlacierFile {
    /// The function inserts a GlacierFile object into a PostgreSQL database table,
    /// handling conflicts by updating existing records.
    /// 
    /// Arguments:
    /// 
    /// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`
    /// object, which represents a connection to a PostgreSQL database. This
    /// connection is used to interact with the database in order to insert a
    /// `GlacierFile` object into the `glacier_state` table.
    /// 
    /// Returns:
    /// 
    /// The `insert` function is returning a `Result` containing a `GlacierFile` or
    /// an `Error`.
    pub fn insert(&self, conn: &mut PgConnection) -> Result<GlacierFile, Error> {
        diesel::insert_into(glacier_state)
            .values(self)
            .on_conflict(glacier_file_path)
            .do_update()
            .set(self)
            .returning(GlacierFile::as_returning())
            .get_result(conn)
    }

    /// The function deletes a record from a database table based on the file path.
    /// 
    /// Arguments:
    /// 
    /// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`
    /// object, which represents a connection to a PostgreSQL database. This
    /// connection is used to execute the delete operation on the database table
    /// `glacier_state`.
    /// 
    /// Returns:
    /// 
    /// The `delete` function is returning a `Result` enum with the success type
    /// `usize` (indicating the number of rows affected) and the error type
    /// `diesel::result::Error`.
    pub fn delete(&self, conn: &mut PgConnection) -> Result<usize, diesel::result::Error> {
        diesel::delete(glacier_state.find(&self.file_path))
            .filter(crate::schema::glacier_state::dsl::file_path.eq(&self.file_path))
            .execute(conn)
    }
}