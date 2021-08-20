use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{error::TestSuiteDecodeError, Config};

/// The configuration of a Quilkin testsuite.
pub struct TestSuite {
    pub config: Config,
    pub options: TestConfig,
}

#[derive(Deserialize, Serialize)]
pub struct TestConfig {
    pub config: Option<PathBuf>,
    pub tests: std::collections::HashMap<String, TestCase>,
}

impl TestSuite {
    pub fn find<P: AsRef<Path>>(
        log: &slog::Logger,
        path: Option<P>,
    ) -> Result<Self, TestSuiteDecodeError> {
        super::find_config_file(log, path)
            .map_err(From::from)
            .and_then(|s| Self::from_yaml(&s))
    }

    /// Attempts to deserialize [`Self`] from a YAML document. A valid source is
    /// either a combination of [`Config`] document followed by [`TestConfig`]
    /// document separated by a `---` (YAML document separator), or a
    /// `TestConfig` document containing a `config` key that points to a valid
    /// `Config` file.
    pub fn from_yaml(src: &str) -> Result<Self, TestSuiteDecodeError> {
        Ok(
            if let Ok(options) = serde_yaml::from_str::<TestConfig>(src) {
                let path = options
                    .config
                    .as_deref()
                    .ok_or(TestSuiteDecodeError::MissingConfigInTestOptions)?;
                let config = serde_yaml::from_reader(std::fs::File::open(path)?)?;
                Self { config, options }
            } else {
                let mut de = serde_yaml::Deserializer::from_str(src);
                let config =
                    Config::deserialize(de.next().ok_or(TestSuiteDecodeError::MissingConfig)?)?;
                let options = TestConfig::deserialize(
                    de.next().ok_or(TestSuiteDecodeError::MissingTestOptions)?,
                )?;
                Self { config, options }
            },
        )
    }
}

#[derive(Deserialize, Serialize)]
pub struct TestCase {
    /// The data to be given to Quilkin.
    pub input: String,
    /// What we expect Quilkin to send to the game server.
    pub output: String,
}
