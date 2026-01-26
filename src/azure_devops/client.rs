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
        let pat = config.get_pat()?;

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

        let status = response.status();
        if !status.is_success() || status == reqwest::StatusCode::NON_AUTHORITATIVE_INFORMATION {
            return Err(self.extract_api_error(response, id).await);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse work item response")?;

        WorkItem::from_json(&json, id)
    }

    async fn extract_api_error(&self, response: reqwest::Response, id: u32) -> anyhow::Error {
        let status = response.status();

        if status == reqwest::StatusCode::NOT_FOUND {
            return anyhow::anyhow!("Work Item #{} not found", id);
        }

        if status == reqwest::StatusCode::NON_AUTHORITATIVE_INFORMATION {
            return anyhow::anyhow!(
                "Authentication failed (Status 203). This is most likely due to an invalid PAT. Please check your PAT and Azure DevOps organization URL."
            );
        }

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return anyhow::anyhow!(
                    "Azure DevOps API error ({}): failed to read body: {}",
                    status,
                    e
                );
            }
        };

        // Try to extract clean message from JSON error response
        let error_msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|json| {
                json.get("message")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("Azure DevOps API error ({})", status));

        anyhow::anyhow!("{}", error_msg)
    }
}
