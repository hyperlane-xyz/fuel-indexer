use crate::{ops::forc_index_deploy, utils::defaults};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Deploy an index asset bundle to a remote or locally running indexer server.
#[derive(Debug, Parser)]
pub struct Command {
    /// URL at which to upload index assets
    #[clap(long, default_value = defaults::INDEXER_SERVICE_URL, help = "URL at which to upload index assets.")]
    pub url: String,

    /// Path of the index manifest to upload
    #[clap(short, long, help = "Path of the index manifest to upload.")]
    pub manifest: PathBuf,

    /// Authentication header value
    #[clap(long, help = "Authentication header value.")]
    pub auth: Option<String>,
}

pub fn exec(command: Command) -> Result<()> {
    forc_index_deploy::init(command)?;
    Ok(())
}
