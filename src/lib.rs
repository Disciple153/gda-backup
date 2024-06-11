pub mod models;
pub mod schema;
pub mod backup;
pub mod restore;
pub mod aws;
pub mod s3;
pub mod dynamodb;
pub mod environment;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::Error;
use dotenvy::dotenv;
use environment::DatabaseArgs;
use models::{GlacierFile, LocalFile};

use crate::schema::glacier_state::dsl::{
    glacier_state,
    file_path as glacier_file_path,
    modified as glacier_modified,
};
use crate::schema::local_state::dsl::{
    local_state,
    file_path as local_file_path,
    modified as local_modified,
};

joinable!(crate::schema::local_state -> crate::schema::glacier_state (file_path));

/// The function `establish_connection` establishes a connection to a PostgreSQL
/// database using the provided arguments.
/// 
/// Arguments:
/// 
/// * `args`: The `establish_connection` function takes a reference to an `Args`
/// struct as a parameter. The `Args` struct likely contains information needed to
/// establish a database connection, such as the database engine, username,
/// password, host, and database name.
/// 
/// Returns:
/// 
/// The function `establish_connection` returns a `PgConnection` object, which
/// represents a connection to a PostgreSQL database.
pub fn establish_connection(args: DatabaseArgs) -> PgConnection {
    dotenv().ok();

    let db_engine = args.db_engine.clone();
    let postgres_user = args.postgres_user.clone();
    let postgres_password = args.postgres_password.clone();
    let postgres_host = args.postgres_host.clone();
    let postgres_db = args.postgres_db.clone();

    let postgres_url: String = format!("{db_engine}://{postgres_user}:{postgres_password}@{postgres_host}/{postgres_db}");
    PgConnection::establish(&postgres_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", postgres_url))
}

/// The function checks if the glacier_state table is empty in a Rust application.
/// 
/// Arguments:
/// 
/// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`, which
/// represents a connection to a PostgreSQL database. This connection will be used
/// to execute a query to check if a table named `glacier_state` is empty.
/// 
/// Returns:
/// 
/// The function `glacier_state_is_empty` returns a boolean value indicating whether
/// the `glacier_state` table in the PostgreSQL database is empty or not. If the
/// count of records in the `glacier_state` table is equal to 0, then it returns
/// `true`, indicating that the table is empty. Otherwise, it returns `false`,
/// indicating that the table is not empty.
pub fn glacier_state_is_empty(conn: &mut PgConnection) -> bool {
    let glacier_file_count: usize = glacier_state.limit(1)
        .execute(conn)
        .expect("Error when checking if glacier_state is populated.");

    glacier_file_count == 0
}

/// The function `clear_local_state` deletes all records from the `local_state`
/// table in a PostgreSQL database using Diesel in Rust.
/// 
/// Arguments:
/// 
/// * `conn`: The `conn` parameter in the `clear_local_state` function is a mutable
/// reference to a `PgConnection` object. This object represents a connection to a
/// PostgreSQL database and is used to execute database operations such as querying
/// or modifying data.
pub fn clear_local_state(conn: &mut PgConnection) {
    diesel::delete(local_state)
        .execute(conn)
        .expect("Error clearing local_state.");
}

/// The function `clear_glacier_state` deletes all records from the `glacier_state`
/// table in a PostgreSQL database using Diesel in Rust.
/// 
/// Arguments:
/// 
/// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`
/// object, which represents a connection to a PostgreSQL database. This connection
/// is used to interact with the database and perform operations such as deleting
/// records from the `glacier_state` table in this case.
pub fn clear_glacier_state(conn: &mut PgConnection) {
    diesel::delete(glacier_state)
        .execute(conn)
        .expect("Error clearing glacier_state.");
}

/// This Rust function retrieves a GlacierFile from a database connection based on a
/// given file path.
/// 
/// Arguments:
/// 
/// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`
/// object, which is a connection to a PostgreSQL database. This connection is used
/// to interact with the database to retrieve information about a Glacier file based
/// on the provided `file_path`.
/// * `file_path`: The `file_path` parameter is a `String` type that represents the
/// path of the file you want to retrieve from Glacier.
/// 
/// Returns:
/// 
/// a Result type with either a GlacierFile or an Error.
pub fn get_glacier_file(conn: &mut PgConnection, file_path: String) -> Result<GlacierFile, Error> {
    glacier_state
        .find(file_path)
        .first(conn)
}

/// The function `get_new_files` retrieves local files that do not have a
/// corresponding entry in the glacier state table.
/// 
/// Arguments:
/// 
/// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`, which
/// is a connection to a PostgreSQL database. The function `get_new_files` is using
/// this connection to query the database for new files that have not been archived
/// in the Glacier storage.
/// 
/// Returns:
/// 
/// A vector of `LocalFile` instances is being returned.
pub fn get_new_files(conn: &mut PgConnection) -> Vec<LocalFile> {
    let join = local_state.left_join(glacier_state);

    join
        .filter(glacier_file_path.is_null())
        .select(LocalFile::as_select())
        .load(conn)
        .expect("Error getting new files.")
}

/// This Rust function retrieves a list of local files that have been modified more
/// recently than their corresponding files in a glacier state.
/// 
/// Arguments:
/// 
/// * `conn`: The `conn` parameter in the `get_changed_files` function is a mutable
/// reference to a `PgConnection` object, which represents a connection to a
/// PostgreSQL database. This connection is used to interact with the database to
/// retrieve information about changed files.
/// 
/// Returns:
/// 
/// A vector of `LocalFile` instances representing the files that have been changed
/// locally compared to their corresponding files in the glacier state.
pub fn get_changed_files(conn: &mut PgConnection) -> Vec<LocalFile> {
    local_state
        .inner_join(glacier_state.on(glacier_file_path.eq(local_file_path)))
        .filter(glacier_modified.lt(local_modified))
        .select(LocalFile::as_select())
        .load(conn)
        .expect("Error getting updated files.")
}

/// This Rust function retrieves missing files by performing a left join and
/// filtering for null local file paths.
/// 
/// Arguments:
/// 
/// * `conn`: The `conn` parameter is a mutable reference to a `PgConnection`, which
/// is a connection to a PostgreSQL database. The function `get_missing_files` is
/// using this connection to query the database for missing files.
/// 
/// Returns:
/// 
/// A vector of `GlacierFile` objects representing the missing files is being
/// returned.
pub fn get_missing_files(conn: &mut PgConnection) -> Vec<GlacierFile> {
    let join = glacier_state.left_join(local_state);

    join
        .filter(local_file_path.is_null())
        .select(GlacierFile::as_select())
        .load(conn)
        .expect("Error getting deleted files.")
}