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

    pub async fn verify_connection(&self) -> Result<()> {
        let url = format!("{}/_apis/connectionData", self.base_url);

        let response = self
            .client
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/json")
            .basic_auth("", Some(&self.pat))
            .send()
            .await
            .context("Failed to send verification request to Azure DevOps")?;

        let status = response.status();
        if status.is_success() {
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            if !content_type.contains("application/json") {
                return Err(anyhow::anyhow!(
                    "Verification returned non-JSON response ({}). This often means the request was redirected to a login page; check PAT/auth setup.",
                    content_type
                ));
            }

            let json: serde_json::Value = response
                .json()
                .await
                .context("Failed to parse Azure DevOps verification response")?;

            let has_authenticated_user = json
                .get("authenticatedUser")
                .and_then(|u| u.get("id"))
                .and_then(|id| id.as_str())
                .is_some();

            if !has_authenticated_user {
                return Err(anyhow::anyhow!(
                    "Verification response missing authenticated user details"
                ));
            }

            return Ok(());
        }

        Err(self.extract_verification_error(response).await)
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

    async fn extract_verification_error(&self, response: reqwest::Response) -> anyhow::Error {
        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::NON_AUTHORITATIVE_INFORMATION
        {
            return anyhow::anyhow!(
                "Authentication failed (status {}). Check your PAT and organization URL.",
                status
            );
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return anyhow::anyhow!(
                "Verification endpoint not found (404). Check the organization URL (including collection/path for Azure DevOps Server)."
            );
        }

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return anyhow::anyhow!(
                    "Azure DevOps verification error ({}): failed to read body: {}",
                    status,
                    e
                );
            }
        };

        let error_msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|json| {
                json.get("message")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("Azure DevOps verification failed ({})", status));

        anyhow::anyhow!("{}", error_msg)
    }
}
