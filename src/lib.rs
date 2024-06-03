pub mod models;
pub mod schema;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenvy::dotenv;
use models::{GlacierFile, LocalFile};

use crate::schema::glacier_state::dsl::{
    glacier_state,
    file_path as glacier_file_path,
    modified as glacier_modified,
    uploaded as glacier_uploaded,
    pending_delete,
};
use crate::schema::local_state::dsl::{
    local_state,
    file_path as local_file_path,
    modified as local_modified,
};

const DB_ENGINE: &str = "postgres";
const DB_USER: &str = "username";
const DB_PASSWORD: &str = "password";
const DB_HOST: &str = "localhost";
const DB_DB: &str = "backup_state";

joinable!(crate::schema::local_state -> crate::schema::glacier_state (file_path));
// allow_tables_to_appear_in_same_query!(glacier_state, local_state);

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let db_url: String = format!("{DB_ENGINE}://{DB_USER}:{DB_PASSWORD}@{DB_HOST}/{DB_DB}");
    PgConnection::establish(&db_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", db_url))
}

pub fn glacier_state_is_empty(conn: &mut PgConnection) -> bool {
    let glacier_file_count: usize = glacier_state.limit(1)
        .execute(conn)
        .expect("Error when checking if glacier_state is populated.");

    glacier_file_count == 0
}

pub fn clear_local_state(conn: &mut PgConnection) {
    diesel::delete(local_state)
        .execute(conn)
        .expect("Error clearing local_state.");
}

pub fn clear_glacier_state(conn: &mut PgConnection) {
    diesel::delete(glacier_state)
        .execute(conn)
        .expect("Error clearing glacier_state.");
}

pub fn get_pending_upload_files(conn: &mut PgConnection) -> Vec<GlacierFile> {
    glacier_state
        .filter(glacier_uploaded.is_null())
        .select(GlacierFile::as_select())
        .load(conn)
        .expect("Error getting pending upserts.")
}

pub fn get_pending_update_files(conn: &mut PgConnection) -> Vec<GlacierFile> {
    glacier_state
        .filter(glacier_uploaded.is_not_null())
        .filter(glacier_modified.nullable().ne(glacier_uploaded))
        .select(GlacierFile::as_select())
        .load(conn)
        .expect("Error getting pending upserts.")
}

pub fn get_pending_delete_files(conn: &mut PgConnection) -> Vec<GlacierFile> {
    glacier_state
        .filter(pending_delete)
        .select(GlacierFile::as_select())
        .load(conn)
        .expect("Error getting pending deletions.")
}

pub fn get_new_files(conn: &mut PgConnection) -> Vec<LocalFile> {
    let join = local_state.left_join(glacier_state);

    join
        .filter(glacier_file_path.is_null())
        .select(LocalFile::as_select())
        .load(conn)
        .expect("Error getting new files.")
}

pub fn get_changed_files(conn: &mut PgConnection) -> Vec<LocalFile> {
    local_state
        .inner_join(glacier_state.on(glacier_file_path.eq(local_file_path)))
        .filter(glacier_modified.lt(local_modified))
        .select(LocalFile::as_select())
        .load(conn)
        .expect("Error getting updated files.")
}

pub fn get_missing_files(conn: &mut PgConnection) -> Vec<GlacierFile> {
    let join = glacier_state.left_join(local_state);

    join
        .filter(local_file_path.is_null())
        .select(GlacierFile::as_select())
        .load(conn)
        .expect("Error getting deleted files.")
}

