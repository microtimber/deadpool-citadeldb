#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(
    nonstandard_style,
    rust_2018_idioms,
    rustdoc::broken_intra_doc_links,
    rustdoc::private_intra_doc_links
)]
#![forbid(non_ascii_idents, unsafe_code)]
#![warn(
    deprecated_in_future,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    unreachable_pub,
    unused_import_braces,
    unused_labels,
    unused_lifetimes,
    unused_qualifications,
    unused_results
)]
#![allow(clippy::uninlined_format_args)]

use std::sync::atomic::{AtomicIsize, Ordering};

use deadpool::managed::{self, RecycleError};
use deadpool_sync::SyncWrapper;

/// Configuration support.
pub mod config;
pub use config::{Config, ConfigError};

pub use citadel;
pub use citadel_sql;

pub use deadpool::managed::reexports::*;
pub use deadpool_sync::reexports::*;

deadpool::managed_reexports!(
    "citadeldb",
    Manager,
    managed::Object<Manager>,
    Error,
    ConfigError
);

/// Type alias for [`Object`]
pub type Connection = Object;

/// [`Manager`] for creating and recycling CitadelDB connections.
///
/// [`Manager`]: managed::Manager
#[derive(Debug)]
pub struct Manager {
    db: &'static citadel::Database,
    recycle_count: AtomicIsize,
    runtime: Runtime,
}

impl Manager {
    /// Creates a new [`Manager`] using the given [`Config`] backed by the
    /// specified [`Runtime`].
    #[must_use]
    pub fn from_config(config: &Config, runtime: Runtime) -> Self {
        Self {
            db: Box::leak(Box::new(config.create_database().unwrap())),
            recycle_count: AtomicIsize::new(0),
            runtime,
        }
    }
}

/// Error type for [`deadpool-citadeldb`](crate).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The error was reported by the CitadelDB storage engine.
    #[error("CitadelDB error: {0}")]
    Citadel(#[from] citadel::Error),
    /// The test query was executed but the database returned
    /// an unexpected response.
    #[error("Test query failed: {0}")]
    TestQueryFailed(String),
}

impl managed::Manager for Manager {
    type Type = SyncWrapper<citadel_sql::Connection<'static>>;
    type Error = citadel_sql::error::SqlError;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        // Box::leak gives us a 'static reference, satisfying SyncWrapper's T: 'static bound.
        let db: &'static citadel::Database = self.db;
        SyncWrapper::new(self.runtime, move || citadel_sql::Connection::open(db)).await
    }

    async fn recycle(
        &self,
        conn: &mut Self::Type,
        _: &Metrics,
    ) -> managed::RecycleResult<Self::Error> {
        if conn.is_mutex_poisoned() {
            return Err(RecycleError::Message(
                "Mutex is poisoned. Connection is considered unusable.".into(),
            ));
        }
        let recycle_count = self.recycle_count.fetch_add(1, Ordering::Relaxed);
        let n: isize = conn
            .interact(move |conn| {
                let result = conn.query_params(
                    "SELECT $1",
                    &[citadel_sql::Value::Integer(recycle_count as i64)],
                )?;
                result
                    .rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(|val| match val {
                        citadel_sql::Value::Integer(i) => Some(*i as isize),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        citadel_sql::error::SqlError::InvalidValue("expected integer".into())
                    })
            })
            .await
            .map_err(|e| RecycleError::message(format!("{}", e)))??;
        if n == recycle_count {
            Ok(())
        } else {
            Err(RecycleError::message("Recycle count mismatch"))
        }
    }
}
