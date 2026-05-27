use std::path::PathBuf;

use crate::{CreatePoolError, Manager, Pool, PoolBuilder, PoolConfig, Runtime};

/// Configuration object.
///
/// # Example (from environment)
///
/// By enabling the `serde` feature you can read the configuration using the
/// [`config`](https://crates.io/crates/config) crate as following:
/// ```env
/// CITADELDB__PATH=citadel.db
/// CITADELDB__PASSPHRASE=secret
/// CITADELDB__POOL__MAX_SIZE=16
/// CITADELDB__POOL__TIMEOUTS__WAIT__SECS=5
/// CITADELDB__POOL__TIMEOUTS__WAIT__NANOS=0
/// ```
/// ```rust,ignore
/// #[derive(serde::Deserialize, serde::Serialize)]
/// struct Config {
///     citadeldb: deadpool_citadeldb::Config,
/// }
/// impl Config {
///     pub fn from_env() -> Result<Self, config::ConfigError> {
///         let mut cfg = config::Config::builder()
///            .add_source(config::Environment::default().separator("__"))
///            .build()?;
///            cfg.try_deserialize()
///     }
/// }
/// ```
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Config {
    /// Path to CitadelDB database file. Empty string creates an in-memory database.
    pub path: PathBuf,
    /// Passphrase for the CitadelDB database.
    #[cfg_attr(feature = "serde", serde(skip_serializing))]
    pub passphrase: Vec<u8>,
    /// [`Pool`] configuration.
    pub pool: Option<PoolConfig>,
}

impl Config {
    /// Create a new [`Config`] with the given `path` of CitadelDB database
    /// file and `passphrase`. Use an empty path for an in-memory database.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, passphrase: &[u8]) -> Self {
        Self {
            path: path.into(),
            passphrase: passphrase.to_vec(),
            pool: None,
        }
    }

    /// Creates a new [`Pool`] using this [`Config`].
    ///
    /// # Errors
    ///
    /// See [`CreatePoolError`] for details.
    pub fn create_pool(&self, runtime: Runtime) -> Result<Pool, CreatePoolError> {
        self.builder(runtime)
            .map_err(CreatePoolError::Config)?
            .build()
            .map_err(CreatePoolError::Build)
    }

    /// Creates a new [`PoolBuilder`] using this [`Config`].
    ///
    /// # Errors
    ///
    /// See [`ConfigError`] for details.
    pub fn builder(&self, runtime: Runtime) -> Result<PoolBuilder, ConfigError> {
        let manager = Manager::from_config(self, runtime);
        Ok(Pool::builder(manager)
            .config(self.get_pool_config())
            .runtime(runtime))
    }

    /// Returns [`PoolConfig`] which can be used to construct
    /// a [`Pool`] instance.
    #[must_use]
    pub fn get_pool_config(&self) -> PoolConfig {
        self.pool.unwrap_or_default()
    }

    /// Create a [`citadel::Database`] from this configuration.
    ///
    /// If `path` is empty, an in-memory database is created.
    /// If the file exists, it is opened; otherwise a new database is created.
    pub(crate) fn create_database(&self) -> Result<citadel::Database, ConfigError> {
        if self.path.as_os_str().is_empty() {
            citadel::DatabaseBuilder::new("")
                .passphrase(&self.passphrase)
                .create_in_memory()
        } else if self.path.exists() {
            citadel::DatabaseBuilder::new(&self.path)
                .passphrase(&self.passphrase)
                .open()
        } else {
            citadel::DatabaseBuilder::new(&self.path)
                .passphrase(&self.passphrase)
                .create()
        }
    }
}
/// This error is returned if there is something wrong with the SQLite configuration.
///
/// This is just a type alias to [`citadel::Error`] at the moment as there
/// is no validation happening at the configuration phase.
pub type ConfigError = citadel::Error;
