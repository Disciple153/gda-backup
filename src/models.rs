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
    pub uploaded: Option<SystemTime>,
    pub pending_delete: bool
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::local_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Clone)]
pub struct LocalFile {
    pub file_path: String,
    // pub file_hash: Bytea,
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

    pub fn update(&self, conn: &mut PgConnection) -> LocalFile {
        diesel::update(local_state.find(&self.file_path))
            .set((
                crate::schema::local_state::dsl::file_path.eq(&self.file_path),
                crate::schema::local_state::dsl::modified.eq(&self.modified)
            ))
            .returning(LocalFile::as_returning())
            .get_result(conn)
            .expect("Error updating LocalFile.")
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

    pub fn update(&self, conn: &mut PgConnection) -> GlacierFile {
        diesel::update(glacier_state.find(&self.file_path))
            .filter(crate::schema::glacier_state::dsl::file_path.eq(&self.file_path))
            .set((
                crate::schema::glacier_state::dsl::file_path.eq(&self.file_path),
                crate::schema::glacier_state::dsl::file_hash.eq(&self.file_hash),
                crate::schema::glacier_state::dsl::modified.eq(&self.modified),
                crate::schema::glacier_state::dsl::uploaded.eq(&self.uploaded),
                crate::schema::glacier_state::dsl::pending_delete.eq(&self.pending_delete)
            ))
            .returning(GlacierFile::as_returning())
            .get_result(conn)
            .expect("Error updating GlacierFile.")
    }

    pub fn delete(&self, conn: &mut PgConnection) {
        diesel::delete(glacier_state.find(&self.file_path))
            .filter(crate::schema::glacier_state::dsl::file_path.eq(&self.file_path))
            .execute(conn)
            .expect("Error deleting GlacierFile.");
    }
}