use diesel::prelude::*;
use std::time::SystemTime;
use crate::schema::glacier_state::dsl::*;
use crate::schema::local_state::dsl::*;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::glacier_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Clone)]
pub struct GlacierFile {
    pub file_path: String,
    pub file_hash: Option<String>,
    pub modified: SystemTime,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::local_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Clone)]
pub struct LocalFile {
    pub file_path: String,
    pub modified: SystemTime,
}

impl LocalFile {
    pub fn insert(&self, conn: &mut PgConnection) -> LocalFile {
        diesel::insert_into(local_state)
            .values(self)
            .returning(LocalFile::as_returning())
            .get_result(conn)
            .expect("Error saving new LocalFile.")
    }

    pub fn delete(&self, conn: &mut PgConnection) {
        diesel::delete(local_state.find(&self.file_path))
            .filter(crate::schema::local_state::dsl::file_path.eq(&self.file_path))
            .execute(conn)
            .expect("Error deleting LocalFile.");
    }
}


impl GlacierFile {
    pub fn insert(&self, conn: &mut PgConnection) -> GlacierFile {
        diesel::insert_into(glacier_state)
            .values(self)
            .returning(GlacierFile::as_returning())
            .get_result(conn)
            .expect("Error saving new GlacierFile.")
    }

    pub fn delete(&self, conn: &mut PgConnection) {
        diesel::delete(glacier_state.find(&self.file_path))
            .filter(crate::schema::glacier_state::dsl::file_path.eq(&self.file_path))
            .execute(conn)
            .expect("Error deleting GlacierFile.");
    }
}