use std::path::Path;

use anyhow::Result;

use super::fixture::FixtureAzureDevOpsClient;
use super::live::LiveAzureDevOpsClient;
use super::work_item::WorkItem;
use crate::config::Config;

#[derive(Clone)]
pub struct AzureDevOpsClient {
    provider: WorkItemProvider,
}

#[derive(Clone)]
enum WorkItemProvider {
    Live(LiveAzureDevOpsClient),
    Fixture(FixtureAzureDevOpsClient),
}

impl AzureDevOpsClient {
    pub fn new_live(config: &Config) -> Result<Self> {
        Ok(Self {
            provider: WorkItemProvider::Live(LiveAzureDevOpsClient::new(config)?),
        })
    }

    pub fn new_fixture(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            provider: WorkItemProvider::Fixture(FixtureAzureDevOpsClient::from_path(
                path.as_ref(),
            )?),
        })
    }

    #[cfg(test)]
    pub fn uses_demo_fixture(&self) -> bool {
        matches!(self.provider, WorkItemProvider::Fixture(_))
    }

    pub async fn get_work_item(&self, id: u32) -> Result<WorkItem> {
        match &self.provider {
            WorkItemProvider::Live(client) => client.get_work_item(id).await,
            WorkItemProvider::Fixture(client) => client.get_work_item(id),
        }
    }

    pub async fn verify_connection(&self) -> Result<()> {
        match &self.provider {
            WorkItemProvider::Live(client) => client.verify_connection().await,
            WorkItemProvider::Fixture(client) => client.verify_connection(),
        }
    }
}
