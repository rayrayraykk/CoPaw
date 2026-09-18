//! Prepared native Cron writes with exact rollback for Agent publication.

use super::{ApiError, AppServer, internal};

pub(crate) struct CronChange {
    before: String,
    after: String,
}

impl CronChange {
    pub(super) fn new(before: String, after: String) -> Self {
        Self { before, after }
    }

    pub(crate) fn apply(&self, server: &AppServer) -> Result<(), ApiError> {
        server
            .inner
            .core
            .write_cron_data(&self.after)
            .map_err(internal)
    }

    pub(crate) fn rollback(&self, server: &AppServer) -> Result<(), ApiError> {
        server
            .inner
            .core
            .write_cron_data(&self.before)
            .map_err(internal)
    }
}
