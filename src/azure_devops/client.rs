use super::work_item::WorkItem;
use crate::config::Config;
use anyhow::{Context, Result};
use reqwest::Client;

#[derive(Clone)]
pub struct AzureDevOpsClient {
    client: Client,
    base_url: String,
    pat: String,
}

impl AzureDevOpsClient {
    pub fn new(config: &Config) -> Result<Self> {
        let pat = Config::get_pat()?;

        let client = Client::builder()
            .build()
            .context("Failed to create HTTP client")?;

        // Normalize the base URL (remove trailing slash)
        let base_url = config
            .azure_devops
            .organization_url
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            client,
            base_url,
            pat,
        })
    }

    pub async fn get_work_item(&self, id: u32) -> Result<WorkItem> {
        // Azure DevOps REST API endpoint for work items
        // For cloud: https://dev.azure.com/{org}/_apis/wit/workitems/{id}
        // For server: https://server/tfs/{collection}/_apis/wit/workitems/{id}
        let url = format!(
            "{}/_apis/wit/workitems/{}?api-version=7.0",
            self.base_url, id
        );

        let response = self
            .client
            .get(&url)
            .basic_auth("", Some(&self.pat))
            .send()
            .await
            .context("Failed to send request to Azure DevOps")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|e| format!("(failed to read body: {})", e));
            anyhow::bail!("Azure DevOps API error ({}): {}", status, body);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse work item response")?;

        WorkItem::from_json(&json, id)
    }
}
