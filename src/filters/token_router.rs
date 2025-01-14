/*
 * Copyright 2020 Google LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *       http://www.apache.org/licenses/LICENSE-2.0
 *
 *  Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 */

mod metrics;

crate::include_proto!("quilkin.filters.token_router.v1alpha1");

use std::convert::TryFrom;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::error;

use crate::{
    config::LOG_SAMPLING_RATE,
    endpoint::RetainedItems,
    filters::{metadata::CAPTURED_BYTES, prelude::*},
    metadata::Value,
};

use metrics::Metrics;

use self::quilkin::filters::token_router::v1alpha1 as proto;

/// Filter that only allows packets to be passed to Endpoints that have a matching
/// connection_id to the token stored in the Filter's dynamic metadata.
pub struct TokenRouter {
    metadata_key: Arc<String>,
    metrics: Metrics,
}

impl TokenRouter {
    fn new(config: Config, metrics: Metrics) -> Self {
        Self {
            metadata_key: Arc::new(config.metadata_key),
            metrics,
        }
    }
}

impl StaticFilter for TokenRouter {
    const NAME: &'static str = "quilkin.filters.token_router.v1alpha1.TokenRouter";
    type Configuration = Config;
    type BinaryConfiguration = proto::TokenRouter;

    fn try_from_config(config: Option<Self::Configuration>) -> Result<Self, Error> {
        Ok(TokenRouter::new(
            config.unwrap_or_default(),
            Metrics::new()?,
        ))
    }
}

impl Filter for TokenRouter {
    #[cfg_attr(feature = "instrument", tracing::instrument(skip(self, ctx)))]
    fn read(&self, mut ctx: ReadContext) -> Option<ReadResponse> {
        match ctx.metadata.get(self.metadata_key.as_ref()) {
            None => {
                if self.metrics.packets_dropped_no_token_found.get() % LOG_SAMPLING_RATE == 0 {
                    error!(
                        count = ?self.metrics.packets_dropped_no_token_found.get(),
                        metadata_key = ?self.metadata_key.clone(),
                        "Packets are being dropped as no routing token was found in filter dynamic metadata"
                    );
                }
                self.metrics.packets_dropped_no_token_found.inc();
                None
            }
            Some(value) => match value {
                Value::Bytes(token) => match ctx
                    .endpoints
                    .retain(|e| e.metadata.known.tokens.contains(&**token))
                {
                    RetainedItems::None => {
                        self.metrics.packets_dropped_no_endpoint_match.inc();
                        None
                    }
                    _ => Some(ctx.into()),
                },
                _ => {
                    if self.metrics.packets_dropped_invalid_token.get() % LOG_SAMPLING_RATE == 0 {
                        error!(
                            count = ?self.metrics.packets_dropped_invalid_token.get(),
                            metadata_key = ?self.metadata_key.clone(),
                            "Packets are being dropped as routing token has invalid type: expected Value::Bytes"
                        );
                    }
                    self.metrics.packets_dropped_invalid_token.inc();
                    None
                }
            },
        }
    }

    fn write(&self, ctx: WriteContext) -> Option<WriteResponse> {
        Some(ctx.into())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, schemars::JsonSchema)]
#[serde(default)]
pub struct Config {
    /// the key to use when retrieving the token from the Filter's dynamic metadata
    #[serde(rename = "metadataKey", default = "default_metadata_key")]
    pub metadata_key: String,
}

/// Default value for [`Config::metadata_key`]
fn default_metadata_key() -> String {
    CAPTURED_BYTES.into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            metadata_key: default_metadata_key(),
        }
    }
}

impl From<Config> for proto::TokenRouter {
    fn from(config: Config) -> Self {
        Self {
            metadata_key: Some(config.metadata_key),
        }
    }
}

impl TryFrom<proto::TokenRouter> for Config {
    type Error = ConvertProtoConfigError;

    fn try_from(p: proto::TokenRouter) -> Result<Self, Self::Error> {
        Ok(Self {
            metadata_key: p.metadata_key.unwrap_or_else(default_metadata_key),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        endpoint::{Endpoint, Endpoints, Metadata},
        metadata::Value,
        test_utils::assert_write_no_change,
    };

    use super::*;

    const TOKEN_KEY: &str = "TOKEN";

    #[test]
    fn convert_proto_config() {
        let test_cases = vec![
            (
                "should succeed when all valid values are provided",
                proto::TokenRouter {
                    metadata_key: Some("foobar".into()),
                },
                Some(Config {
                    metadata_key: "foobar".into(),
                }),
            ),
            (
                "should use correct default values",
                proto::TokenRouter { metadata_key: None },
                Some(Config {
                    metadata_key: default_metadata_key(),
                }),
            ),
        ];
        for (name, proto_config, expected) in test_cases {
            let result = Config::try_from(proto_config);
            assert_eq!(
                result.is_err(),
                expected.is_none(),
                "{}: error expectation does not match",
                name
            );
            if let Some(expected) = expected {
                assert_eq!(expected, result.unwrap(), "{}", name);
            }
        }
    }

    #[test]
    fn factory_custom_tokens() {
        let filter = TokenRouter::from_config(
            Config {
                metadata_key: TOKEN_KEY.to_string(),
            }
            .into(),
        );
        let mut ctx = new_ctx();
        ctx.metadata.insert(
            Arc::new(TOKEN_KEY.into()),
            Value::Bytes(b"123".to_vec().into()),
        );
        assert_read(&filter, ctx);
    }

    #[test]
    fn factory_empty_config() {
        let filter = TokenRouter::from_config(None);
        let mut ctx = new_ctx();
        ctx.metadata.insert(
            Arc::new(CAPTURED_BYTES.into()),
            Value::Bytes(b"123".to_vec().into()),
        );
        assert_read(&filter, ctx);
    }

    #[test]
    fn downstream_receive() {
        // valid key
        let config = Config {
            metadata_key: CAPTURED_BYTES.into(),
        };
        let filter = TokenRouter::from_config(config.into());

        let mut ctx = new_ctx();
        ctx.metadata.insert(
            Arc::new(CAPTURED_BYTES.into()),
            Value::Bytes(b"123".to_vec().into()),
        );
        assert_read(&filter, ctx);

        // invalid key
        let mut ctx = new_ctx();
        ctx.metadata.insert(
            Arc::new(CAPTURED_BYTES.into()),
            Value::Bytes(b"567".to_vec().into()),
        );

        let option = filter.read(ctx);
        assert!(option.is_none());
        assert_eq!(1, filter.metrics.packets_dropped_no_endpoint_match.get());

        // no key
        let ctx = new_ctx();
        assert!(filter.read(ctx).is_none());
        assert_eq!(1, filter.metrics.packets_dropped_no_token_found.get());

        // wrong type key
        let mut ctx = new_ctx();
        ctx.metadata.insert(
            Arc::new(CAPTURED_BYTES.into()),
            Value::String(String::from("wrong")),
        );
        assert!(filter.read(ctx).is_none());
        assert_eq!(1, filter.metrics.packets_dropped_invalid_token.get());
    }

    #[test]
    fn write() {
        let config = Config {
            metadata_key: CAPTURED_BYTES.into(),
        };
        let filter = TokenRouter::from_config(config.into());
        assert_write_no_change(&filter);
    }

    fn new_ctx() -> ReadContext {
        let endpoint1 = Endpoint::with_metadata(
            "127.0.0.1:80".parse().unwrap(),
            Metadata {
                tokens: vec!["123".into()].into_iter().collect(),
            },
        );
        let endpoint2 = Endpoint::with_metadata(
            "127.0.0.1:90".parse().unwrap(),
            Metadata {
                tokens: vec!["456".into()].into_iter().collect(),
            },
        );

        ReadContext::new(
            Endpoints::new(vec![endpoint1, endpoint2]).into(),
            "127.0.0.1:100".parse().unwrap(),
            b"hello".to_vec(),
        )
    }

    fn assert_read<F>(filter: &F, ctx: ReadContext)
    where
        F: Filter + ?Sized,
    {
        let result = filter.read(ctx).unwrap();

        assert_eq!(b"hello".to_vec(), result.contents);
        assert_eq!(1, result.endpoints.size());
    }
}
