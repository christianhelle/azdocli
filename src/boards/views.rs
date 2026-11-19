//! Result models for work item commands and how each one prints.

use crate::output::{md_table, Render};
use azure_devops_rust_api::wit::models::WorkItem;
use serde::Serialize;
use std::io::{self, Write};

/// A titled list of work items, from a listing, a query or a link lookup.
#[derive(Serialize)]
#[serde(transparent)]
pub(crate) struct WorkItemList<'a> {
    #[serde(skip)]
    pub heading: &'a str,
    pub work_items: &'a [WorkItem],
}

/// A work item's type, state and title, as the list shows them.
fn summary(work_item: &WorkItem) -> (&str, &str, &str) {
    (
        field_str(work_item, "System.WorkItemType").unwrap_or("Unknown"),
        field_str(work_item, "System.State").unwrap_or("Unknown"),
        field_str(work_item, "System.Title").unwrap_or("No Title"),
    )
}

impl Render for WorkItemList<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w)?;
        writeln!(w, "📋 {} ({} items)", self.heading, self.work_items.len())?;
        writeln!(w, "{}", "=".repeat(80))?;
        writeln!(
            w,
            "{:<8} {:<15} {:<20} {:<30}",
            "ID", "Type", "State", "Title"
        )?;
        writeln!(w, "{}", "-".repeat(80))?;

        if self.work_items.is_empty() {
            writeln!(w, "No work items found.")?;
        }
        for work_item in self.work_items {
            let (work_item_type, state, title) = summary(work_item);
            // Truncate title if too long, on a character boundary
            let truncated_title = if title.chars().count() > 30 {
                format!("{}...", title.chars().take(27).collect::<String>())
            } else {
                title.to_string()
            };
            writeln!(
                w,
                "{:<8} {work_item_type:<15} {state:<20} {truncated_title:<30}",
                work_item.id
            )?;
        }

        writeln!(w)?;
        writeln!(
            w,
            "💡 Use 'azdocli boards work-item show --id <ID>' for detailed information"
        )?;
        writeln!(
            w,
            "💡 Use 'azdocli boards work-item show --id <ID> --web' to open in browser"
        )
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "## {} ({} items)\n", self.heading, self.work_items.len())?;
        let rows: Vec<Vec<String>> = self
            .work_items
            .iter()
            .map(|work_item| {
                let (work_item_type, state, title) = summary(work_item);
                vec![
                    work_item.id.to_string(),
                    work_item_type.to_string(),
                    state.to_string(),
                    title.to_string(),
                ]
            })
            .collect();
        write!(w, "{}", md_table(&["ID", "Type", "State", "Title"], &rows))
    }
}

/// One work item's details.
#[derive(Serialize)]
#[serde(transparent)]
pub(crate) struct WorkItemDetails<'a>(pub &'a WorkItem);

/// The string fields the details show, in order, as (field, console label,
/// Markdown label).
const DETAIL_FIELDS: [(&str, &str, &str); 16] = [
    ("System.State", "🔄 State", "State"),
    ("System.WorkItemType", "📌 Type", "Type"),
    ("System.AssignedTo", "🙋 Assigned To", "Assigned to"),
    ("System.IterationPath", "🗓️ Iteration", "Iteration"),
    ("System.AreaPath", "🗂️ Area", "Area"),
    ("System.Tags", "🏷️ Tags", "Tags"),
    ("Microsoft.VSTS.Scheduling.Effort", "📏 Effort", "Effort"),
    (
        "Microsoft.VSTS.Scheduling.StoryPoints",
        "📏 Story Points",
        "Story points",
    ),
    ("Microsoft.VSTS.Scheduling.Size", "📏 Size", "Size"),
    (
        "Microsoft.VSTS.Scheduling.OriginalEstimate",
        "⌛ Original Estimate",
        "Original estimate",
    ),
    (
        "Microsoft.VSTS.Scheduling.RemainingWork",
        "⏳ Remaining Work",
        "Remaining work",
    ),
    (
        "Microsoft.VSTS.Scheduling.CompletedWork",
        "✔️ Completed Work",
        "Completed work",
    ),
    ("System.CreatedBy", "👤 Created By", "Created by"),
    ("System.CreatedDate", "📅 Created Date", "Created"),
    ("System.ChangedBy", "🔄 Changed By", "Changed by"),
    ("System.ChangedDate", "📅 Changed Date", "Changed"),
];

const ACCEPTANCE_CRITERIA: &str = "Microsoft.VSTS.Common.AcceptanceCriteria";

impl WorkItemDetails<'_> {
    fn priority(&self) -> Option<i64> {
        self.0
            .fields
            .get("Microsoft.VSTS.Common.Priority")
            .and_then(serde_json::Value::as_i64)
    }
}

impl Render for WorkItemDetails<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        let work_item = self.0;
        writeln!(w, "📋 Work Item Details")?;
        writeln!(w, "=====================")?;
        writeln!(w, "🆔 ID: {}", work_item.id)?;
        if let Some(rev) = work_item.rev {
            writeln!(w, "📚 Revision: {rev}")?;
        }
        if let Some(title) = field_str(work_item, "System.Title") {
            writeln!(w, "📝 Title: {title}")?;
        }
        for (field, label, _) in DETAIL_FIELDS {
            if let Some(value) = field_display(work_item, field) {
                writeln!(w, "{label}: {value}")?;
            }
        }
        if let Some(priority) = self.priority() {
            writeln!(w, "🔝 Priority: {priority}")?;
        }
        if let Some(description) = field_str(work_item, "System.Description") {
            writeln!(w, "\n📄 Description:")?;
            writeln!(w, "{description}")?;
        }
        if let Some(criteria) = field_str(work_item, ACCEPTANCE_CRITERIA) {
            writeln!(w, "\n✅ Acceptance Criteria:")?;
            writeln!(w, "{criteria}")?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let work_item = self.0;
        writeln!(
            w,
            "## #{} {}\n",
            work_item.id,
            field_str(work_item, "System.Title").unwrap_or("No Title")
        )?;
        if let Some(rev) = work_item.rev {
            writeln!(w, "- **Revision:** {rev}")?;
        }
        for (field, _, label) in DETAIL_FIELDS {
            if let Some(value) = field_display(work_item, field) {
                writeln!(w, "- **{label}:** {value}")?;
            }
        }
        if let Some(priority) = self.priority() {
            writeln!(w, "- **Priority:** {priority}")?;
        }
        if let Some(description) = field_str(work_item, "System.Description") {
            writeln!(w, "\n### Description\n\n{description}")?;
        }
        if let Some(criteria) = field_str(work_item, ACCEPTANCE_CRITERIA) {
            writeln!(w, "\n### Acceptance Criteria\n\n{criteria}")?;
        }
        Ok(())
    }
}

/// A field as text: strings as they are, people by display name, numbers as written.
fn field_display(work_item: &WorkItem, field: &str) -> Option<String> {
    match work_item.fields.get(field)? {
        serde_json::Value::String(text) => Some(text.clone()),
        // Whole numbers come back as 5.0; show them as 5.
        serde_json::Value::Number(number) => Some(match number.as_f64() {
            Some(value) if value.fract() == 0.0 && value.abs() < 1e15 => format!("{value:.0}"),
            _ => number.to_string(),
        }),
        serde_json::Value::Object(identity) => identity
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn field_str<'a>(work_item: &'a WorkItem, field: &str) -> Option<&'a str> {
    work_item.fields.get(field).and_then(|value| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Render;
    use serde_json::json;

    fn text(value: &impl Render) -> String {
        let mut out = Vec::new();
        value.text(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn markdown(value: &impl Render) -> String {
        let mut out = Vec::new();
        value.markdown(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn work_item(id: i32, title: &str) -> WorkItem {
        serde_json::from_value(json!({
            "id": id,
            "rev": 4,
            "url": format!("https://dev.azure.com/org/_apis/wit/workItems/{id}"),
            "fields": {
                "System.Title": title,
                "System.State": "Active",
                "System.WorkItemType": "Bug",
                "System.CreatedBy": "Ada",
                "Microsoft.VSTS.Common.Priority": 2,
                "System.Description": "It breaks"
            }
        }))
        .unwrap()
    }

    const FOOTER: &str = "\n💡 Use 'azdocli boards work-item show --id <ID>' for detailed information\n\
                          💡 Use 'azdocli boards work-item show --id <ID> --web' to open in browser\n";

    fn table_top(heading: &str, count: usize) -> String {
        format!(
            "\n📋 {heading} ({count} items)\n{}\n{:<8} {:<15} {:<20} {:<30}\n{}\n",
            "=".repeat(80),
            "ID",
            "Type",
            "State",
            "Title",
            "-".repeat(80)
        )
    }

    #[test]
    fn work_item_list_text_is_a_table_with_hints() {
        let items = [work_item(5, "Crash on start")];
        let list = WorkItemList {
            heading: "My Work Items",
            work_items: &items,
        };
        let expected = table_top("My Work Items", 1)
            + &format!(
                "{:<8} {:<15} {:<20} {:<30}\n",
                5, "Bug", "Active", "Crash on start"
            )
            + FOOTER;
        assert_eq!(text(&list), expected);
    }

    #[test]
    fn work_item_list_text_truncates_long_titles() {
        let items = [work_item(5, &"x".repeat(40))];
        let list = WorkItemList {
            heading: "My Work Items",
            work_items: &items,
        };
        assert!(text(&list).contains(&format!("{}...", "x".repeat(27))));
    }

    #[test]
    fn work_item_list_text_truncates_unicode_titles_on_character_boundaries() {
        let items = [work_item(5, &"🚀".repeat(40))];
        let list = WorkItemList {
            heading: "My Work Items",
            work_items: &items,
        };
        assert!(text(&list).contains(&format!("{}...", "🚀".repeat(27))));
    }

    #[test]
    fn work_item_list_text_says_when_there_are_none() {
        let list = WorkItemList {
            heading: "Query Results",
            work_items: &[],
        };
        let expected = table_top("Query Results", 0) + "No work items found.\n" + FOOTER;
        assert_eq!(text(&list), expected);
    }

    #[test]
    fn work_item_list_markdown_is_a_table_with_full_titles() {
        let title = "x".repeat(40);
        let items = [work_item(5, &title)];
        let list = WorkItemList {
            heading: "My Work Items",
            work_items: &items,
        };
        assert_eq!(
            markdown(&list),
            format!(
                "## My Work Items (1 items)\n\n| ID | Type | State | Title |\n| --- | --- | --- | --- |\n| 5 | Bug | Active | {title} |\n"
            )
        );
    }

    #[test]
    fn work_item_list_serializes_as_an_array() {
        let items = [work_item(5, "Crash")];
        let list = WorkItemList {
            heading: "My Work Items",
            work_items: &items,
        };
        let value = serde_json::to_value(list).unwrap();
        assert_eq!(value[0]["id"], 5);
        assert_eq!(value[0]["fields"]["System.Title"], "Crash");
    }

    #[test]
    fn work_item_details_text_matches_the_console_summary() {
        let item = work_item(5, "Crash on start");
        assert_eq!(
            text(&WorkItemDetails(&item)),
            "📋 Work Item Details\n\
             =====================\n\
             🆔 ID: 5\n\
             📚 Revision: 4\n\
             📝 Title: Crash on start\n\
             🔄 State: Active\n\
             📌 Type: Bug\n\
             👤 Created By: Ada\n\
             🔝 Priority: 2\n\
             \n📄 Description:\n\
             It breaks\n"
        );
    }

    #[test]
    fn work_item_details_markdown_lists_the_fields() {
        let item = work_item(5, "Crash on start");
        assert_eq!(
            markdown(&WorkItemDetails(&item)),
            "## #5 Crash on start\n\n\
             - **Revision:** 4\n\
             - **State:** Active\n\
             - **Type:** Bug\n\
             - **Created by:** Ada\n\
             - **Priority:** 2\n\
             \n### Description\n\n\
             It breaks\n"
        );
    }

    #[test]
    fn work_item_details_show_tags_and_acceptance_criteria() {
        let mut item = work_item(5, "Sign up");
        item.fields["System.Tags"] = json!("api; auth");
        item.fields["Microsoft.VSTS.Common.AcceptanceCriteria"] = json!("Given a user");

        let text = text(&WorkItemDetails(&item));
        assert!(text.contains("🏷️ Tags: api; auth\n"));
        assert!(text.ends_with("\n✅ Acceptance Criteria:\nGiven a user\n"));

        let markdown = markdown(&WorkItemDetails(&item));
        assert!(markdown.contains("- **Tags:** api; auth\n"));
        assert!(markdown.ends_with("\n### Acceptance Criteria\n\nGiven a user\n"));
    }

    #[test]
    fn work_item_details_show_people_by_display_name() {
        let mut item = work_item(5, "Sign up");
        item.fields["System.CreatedBy"] =
            json!({"displayName": "Ada Lovelace", "uniqueName": "ada@example.com"});
        assert!(text(&WorkItemDetails(&item)).contains("👤 Created By: Ada Lovelace\n"));
        assert!(markdown(&WorkItemDetails(&item)).contains("- **Created by:** Ada Lovelace\n"));
    }

    #[test]
    fn work_item_details_show_sprint_fields() {
        let mut item = work_item(5, "Sign up");
        item.fields["System.AssignedTo"] = json!({"displayName": "Ada Lovelace"});
        item.fields["System.IterationPath"] = json!("Proj\\Sprint 3");
        item.fields["Microsoft.VSTS.Scheduling.StoryPoints"] = json!(5.0);
        item.fields["Microsoft.VSTS.Scheduling.RemainingWork"] = json!(2.5);

        let text = text(&WorkItemDetails(&item));
        for line in [
            "🙋 Assigned To: Ada Lovelace\n",
            "🗓️ Iteration: Proj\\Sprint 3\n",
            "📏 Story Points: 5\n",
            "⏳ Remaining Work: 2.5\n",
        ] {
            assert!(text.contains(line), "missing {line:?} in {text}");
        }
        assert!(markdown(&WorkItemDetails(&item)).contains("- **Story points:** 5\n"));
    }

    #[test]
    fn work_item_details_serializes_as_the_work_item() {
        let item = work_item(5, "Crash");
        let value = serde_json::to_value(WorkItemDetails(&item)).unwrap();
        assert_eq!(value["id"], 5);
        assert_eq!(value["fields"]["System.State"], "Active");
    }
}
