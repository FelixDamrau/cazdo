//! The single owner of the Azure DevOps work-item JSON shape.
//!
//! `decode` parses an Azure-shaped response into a [`WorkItem`]. Both adapters
//! use it: the live client decodes its HTTP response, and the fixture decodes
//! the Azure-shaped JSON it has on hand. Keeping the field names, the `;` tag
//! separator, the `_links.html.href` location, and the rich-text field mapping
//! here means the schema is defined exactly once.

use anyhow::{Context, Result};
use serde_json::Value;

use super::work_item::{RichTextField, WorkItem, WorkItemParts};

const FIELDS: &str = "fields";
const LINKS: &str = "_links";
const HTML: &str = "html";
const HREF: &str = "href";
const DISPLAY_NAME: &str = "displayName";

const TITLE: &str = "System.Title";
const WORK_ITEM_TYPE: &str = "System.WorkItemType";
const STATE: &str = "System.State";
const ASSIGNED_TO: &str = "System.AssignedTo";
const TAGS: &str = "System.Tags";

const TAG_SPLIT: char = ';';

/// Known rich text fields in Azure DevOps, paired (`azure field name`,
/// `display name`). The display name is what callers see; the azure name is the
/// wire key.
const RICH_TEXT_FIELDS: &[(&str, &str)] = &[
    ("System.Description", "Description"),
    (
        "Microsoft.VSTS.Common.AcceptanceCriteria",
        "Acceptance Criteria",
    ),
    ("Microsoft.VSTS.TCM.ReproSteps", "Repro Steps"),
    ("Microsoft.VSTS.TCM.SystemInfo", "System Info"),
    ("Microsoft.VSTS.Common.Resolution", "Resolution"),
    ("Microsoft.VSTS.Build.FoundIn", "Found In"),
    ("Microsoft.VSTS.Build.IntegrationBuild", "Integration Build"),
];

/// Parse an Azure DevOps work-item response into a [`WorkItem`].
pub(super) fn decode(json: &Value, id: u32) -> Result<WorkItem> {
    let fields = json
        .get(FIELDS)
        .context("Missing 'fields' in work item response")?;

    let title = fields
        .get(TITLE)
        .and_then(|v| v.as_str())
        .context("Missing 'System.Title' field")?
        .to_string();

    let work_item_type = fields
        .get(WORK_ITEM_TYPE)
        .and_then(|v| v.as_str())
        .context("Missing 'System.WorkItemType' field")?;

    let state = fields
        .get(STATE)
        .and_then(|v| v.as_str())
        .context("Missing 'System.State' field")?;

    let assigned_to = fields
        .get(ASSIGNED_TO)
        .and_then(|v| v.get(DISPLAY_NAME))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let url = json
        .get(LINKS)
        .and_then(|l| l.get(HTML))
        .and_then(|h| h.get(HREF))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tags: Vec<String> = fields
        .get(TAGS)
        .and_then(|v| v.as_str())
        .map(|s| {
            s.split(TAG_SPLIT)
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let mut rich_text_fields = Vec::new();
    for (field_name, display_name) in RICH_TEXT_FIELDS {
        if let Some(value) = fields.get(*field_name).and_then(|v| v.as_str())
            && !value.trim().is_empty()
        {
            rich_text_fields.push(RichTextField {
                name: display_name.to_string(),
                value: value.to_string(),
            });
        }
    }

    Ok(WorkItem::from_parts(WorkItemParts {
        id,
        title,
        work_item_type,
        state,
        assigned_to,
        url,
        tags,
        rich_text_fields,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decode_extracts_core_fields_and_rich_text() {
        let json = json!({
            "fields": {
                "System.Title": "Fix login flow",
                "System.WorkItemType": "Bug",
                "System.State": "Active",
                "System.AssignedTo": { "displayName": "Ada Lovelace" },
                "System.Tags": "Auth; Urgent ; ;",
                "System.Description": "<p>Broken on mobile</p>",
                "Microsoft.VSTS.Common.AcceptanceCriteria": "<p>Works again</p>"
            },
            "_links": { "html": { "href": "https://example.test/items/123" } }
        });

        let work_item = decode(&json, 123).expect("work item should parse");

        assert_eq!(work_item.id, 123);
        assert_eq!(work_item.title, "Fix login flow");
        assert_eq!(work_item.work_item_type.display_name(), "Bug");
        assert_eq!(work_item.state.display_name(), "Active");
        assert_eq!(work_item.assigned_to.as_deref(), Some("Ada Lovelace"));
        assert_eq!(
            work_item.url.as_deref(),
            Some("https://example.test/items/123")
        );
        // Trailing/empty tag segments are trimmed away.
        assert_eq!(work_item.tags, vec!["Auth", "Urgent"]);
        assert_eq!(work_item.rich_text_fields.len(), 2);
        assert_eq!(work_item.rich_text_fields[0].name, "Description");
        assert_eq!(work_item.rich_text_fields[1].name, "Acceptance Criteria");
    }

    #[test]
    fn decode_ignores_missing_optional_fields_and_blank_rich_text() {
        let json = json!({
            "fields": {
                "System.Title": "Add reporting",
                "System.WorkItemType": "Feature",
                "System.State": "New",
                "System.Description": "   ",
                "Microsoft.VSTS.Common.AcceptanceCriteria": ""
            }
        });

        let work_item = decode(&json, 55).expect("work item should parse");

        assert_eq!(work_item.assigned_to, None);
        assert_eq!(work_item.url, None);
        assert!(work_item.tags.is_empty());
        assert!(work_item.rich_text_fields.is_empty());
    }

    #[test]
    fn decode_requires_title() {
        let json = json!({
            "fields": {
                "System.WorkItemType": "Task",
                "System.State": "Committed"
            }
        });

        let error = decode(&json, 9).expect_err("missing title should fail");

        assert_eq!(error.to_string(), "Missing 'System.Title' field");
    }
}
