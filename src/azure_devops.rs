mod client;
mod codec;
mod fixture;
mod live;
mod work_item;

use std::path::PathBuf;

use anyhow::Result;

use crate::config::Config;

pub use client::AzureDevOpsClient;
pub use work_item::{FieldFormat, WorkItem};
#[cfg(test)]
pub use work_item::{RichTextField, WorkItemState, WorkItemType};

pub fn work_item_client() -> Result<AzureDevOpsClient> {
    if let Some(path) = std::env::var_os("CAZDO_DEMO_WORK_ITEMS") {
        return AzureDevOpsClient::new_fixture(PathBuf::from(path));
    }

    AzureDevOpsClient::new_live(&Config::load()?)
}
