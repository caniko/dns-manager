//! `dns-manager` resolves declarative DNS data (as serialized by its Nix layer)
//! and renders it into BIND zonefiles, generic octoDNS, or Cloudflare octoDNS
//! configuration.
//!
//! The pipeline is [`parse_document`] → [`validate::check`] → [`resolve::resolve`]
//! → [`render`]. The convenience [`resolve_document`] runs validation and
//! resolution together.

mod error;
pub mod model;
pub mod render;
pub mod resolve;
pub mod validate;

pub use error::{Error, Result};

use model::{Config, RawDoc};

/// Parse the raw JSON document emitted by the Nix layer.
pub fn parse_document(json: &str) -> Result<RawDoc> {
    Ok(serde_json::from_str(json)?)
}

/// Validate then resolve a raw document into the final zone [`Config`].
pub fn resolve_document(doc: &RawDoc) -> Result<Config> {
    validate::check(doc)?;
    resolve::resolve(doc)
}
