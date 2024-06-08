use diesel::prelude::*;
use diesel::result::Error;
use std::time::SystemTime;
use crate::schema::glacier_state::dsl::*;
use crate::schema::local_state::dsl::*;

use crate::schema::glacier_state::dsl::file_path as glacier_file_path;

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::glacier_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Clone, Debug)]
pub struct GlacierFile {
    pub file_path: String,
    pub file_hash: Option<String>,
    pub modified: SystemTime,
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::local_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Clone, Debug)]
pub struct LocalFile {
    pub file_path: String,
    pub modified: SystemTime,
}

impl LocalFile {
    pub fn insert(&self, conn: &mut PgConnection) -> Result<LocalFile, Error> {
        diesel::insert_into(local_state)
            .values(self)
            .returning(LocalFile::as_returning())
            .get_result(conn)
    }

    pub fn delete(&self, conn: &mut PgConnection) -> Result<usize, diesel::result::Error> {
        diesel::delete(local_state.find(&self.file_path))
            .filter(crate::schema::local_state::dsl::file_path.eq(&self.file_path))
            .execute(conn)
    }
}

impl GlacierFile {
    pub fn insert(&self, conn: &mut PgConnection) -> Result<GlacierFile, Error> {
        diesel::insert_into(glacier_state)
            .values(self)
            .on_conflict(glacier_file_path)
            .do_update()
            .set(self)
            .returning(GlacierFile::as_returning())
            .get_result(conn)
    }

    pub fn delete(&self, conn: &mut PgConnection) -> Result<usize, diesel::result::Error> {
        diesel::delete(glacier_state.find(&self.file_path))
            .filter(crate::schema::glacier_state::dsl::file_path.eq(&self.file_path))
            .execute(conn)
    }
}