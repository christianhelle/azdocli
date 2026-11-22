//! A work item's change history: who changed which field, from what to what,
//! and when.

use super::parse_work_item_id;
use crate::connection::Connection;
use crate::output::{self, md_table, OutputFormat, Render};
use crate::text::escape_control_characters;
use anyhow::Result;
use azure_devops_rust_api::wit::models::{WorkItemRelation, WorkItemUpdate};
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

/// Fields that change on every revision and say nothing about the work.
const NOISE_FIELDS: &[&str] = &[
    "System.Rev",
    "System.ChangedDate",
    "System.ChangedBy",
    "System.AuthorizedDate",
    "System.RevisedDate",
    "System.Watermark",
    "System.AuthorizedAs",
    "System.PersonId",
    "System.CommentCount",
];

/// How many updates one request asks for; the API caps a page at 200.
const PAGE_SIZE: i32 = 200;

/// The longest value shown in text output before it is cut short.
const MAX_TEXT_VALUE: usize = 80;

/// One field an update changed.
#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct FieldChange {
    /// Reference name, such as `Microsoft.VSTS.Scheduling.RemainingWork`.
    pub field: String,
    /// Readable name, such as `Remaining Work`.
    pub name: String,
    #[serde(rename = "oldValue", skip_serializing_if = "Option::is_none")]
    pub old_value: Option<Value>,
    #[serde(rename = "newValue", skip_serializing_if = "Option::is_none")]
    pub new_value: Option<Value>,
}

/// A link an update added or removed.
#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct LinkChange {
    pub rel: String,
    pub name: String,
    pub target: String,
}

/// One revision of the work item.
#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct HistoryEntry {
    pub rev: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "changedBy", skip_serializing_if = "Option::is_none")]
    pub changed_by: Option<String>,
    pub changes: Vec<FieldChange>,
    #[serde(rename = "linksAdded", skip_serializing_if = "Vec::is_empty")]
    pub links_added: Vec<LinkChange>,
    #[serde(rename = "linksRemoved", skip_serializing_if = "Vec::is_empty")]
    pub links_removed: Vec<LinkChange>,
}

/// The history of a work item, newest update first.
#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct History {
    #[serde(rename = "workItemId")]
    pub work_item_id: i32,
    pub updates: Vec<HistoryEntry>,
}

/// `Microsoft.VSTS.Scheduling.RemainingWork` -> `Remaining Work`.
fn display_name(reference: &str) -> String {
    let last = reference.rsplit('.').next().unwrap_or(reference);
    let mut name = String::new();
    let mut previous: Option<char> = None;
    for c in last.chars() {
        if c.is_uppercase() && previous.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            name.push(' ');
        }
        name.push(c);
        previous = Some(c);
    }
    name
}

/// Whether `query` names the field, by reference name, short name or the
/// readable name, ignoring case.
fn field_matches(query: &str, reference: &str) -> bool {
    let query = query.trim();
    let last = reference.rsplit('.').next().unwrap_or(reference);
    query.eq_ignore_ascii_case(reference)
        || query.eq_ignore_ascii_case(last)
        || query.eq_ignore_ascii_case(&display_name(reference))
        // A label such as `API Key` for the reference `Custom.APIKey`.
        || query.replace(' ', "").eq_ignore_ascii_case(last)
}

/// Fields whose values are HTML.
const HTML_FIELDS: &[&str] = &[
    "System.Description",
    "System.History",
    "Microsoft.VSTS.TCM.ReproSteps",
    "Microsoft.VSTS.TCM.SystemInfo",
    "Microsoft.VSTS.Common.AcceptanceCriteria",
];

fn is_html_field(reference: &str) -> bool {
    HTML_FIELDS
        .iter()
        .any(|field| field.eq_ignore_ascii_case(reference))
}

/// A value with absent, null and empty text all treated as nothing.
fn meaningful(value: &Option<Value>) -> Option<&Value> {
    value
        .as_ref()
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
}

fn identity_name(value: &Value) -> Option<String> {
    value
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `2026-11-20T10:00:01.123Z` -> `2026-11-20 10:00:01`.
fn format_date(raw: &str) -> String {
    let trimmed = raw.split(['.', 'Z', '+']).next().unwrap_or(raw);
    trimmed.replace('T', " ")
}

fn link_name(relation: &WorkItemRelation) -> String {
    let link = &relation.link;
    link.attributes
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| link.rel.clone())
}

fn link_target(rel: &str, url: &str) -> String {
    if rel == "Hyperlink" {
        return url.to_string();
    }
    let marker = "/_apis/wit/workItems/";
    match url.rfind(marker) {
        Some(at) if url[at + marker.len()..].chars().all(|c| c.is_ascii_digit()) => {
            format!("#{}", &url[at + marker.len()..])
        }
        _ => url.to_string(),
    }
}

fn link_changes(relations: &[WorkItemRelation]) -> Vec<LinkChange> {
    relations
        .iter()
        .map(|relation| LinkChange {
            rel: relation.link.rel.clone(),
            name: link_name(relation),
            target: link_target(&relation.link.rel, &relation.link.url),
        })
        .collect()
}

/// Turns the updates the API returned (oldest first) into history entries,
/// newest first. With `fields`, only changes to those fields are kept and
/// updates without any are dropped; `top` keeps the newest that many.
pub(crate) fn build_entries(
    updates: &[WorkItemUpdate],
    fields: &[String],
    top: Option<usize>,
) -> Vec<HistoryEntry> {
    let mut entries: Vec<HistoryEntry> = updates
        .iter()
        .filter_map(|update| entry(update, fields))
        .collect();
    entries.reverse();
    if let Some(top) = top {
        entries.truncate(top);
    }
    entries
}

fn entry(update: &WorkItemUpdate, filter: &[String]) -> Option<HistoryEntry> {
    let raw = update.fields.as_ref().and_then(Value::as_object);
    let changes: Vec<FieldChange> = raw
        .into_iter()
        .flatten()
        .filter(|(field, _)| !NOISE_FIELDS.contains(&field.as_str()))
        .filter(|(field, _)| {
            filter.is_empty() || filter.iter().any(|query| field_matches(query, field))
        })
        .map(|(field, change)| FieldChange {
            field: field.clone(),
            name: display_name(field),
            old_value: change.get("oldValue").filter(|v| !v.is_null()).cloned(),
            new_value: change.get("newValue").filter(|v| !v.is_null()).cloned(),
        })
        .filter(|change| meaningful(&change.old_value) != meaningful(&change.new_value))
        .collect();

    // Link changes belong to no field, so a field filter leaves them out.
    let (links_added, links_removed) = match (&update.relations, filter.is_empty()) {
        (Some(relations), true) => (
            link_changes(&relations.added),
            link_changes(&relations.removed),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    if changes.is_empty() && links_added.is_empty() && links_removed.is_empty() {
        return None;
    }

    let changed = |field: &str| raw.and_then(|f| f.get(field)?.get("newValue"));
    let date = changed("System.ChangedDate")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            // The API marks a work item's latest revision as revised in 9999.
            let d = update.revised_date?;
            (d.year() < 9000 && d.offset().whole_seconds() == 0).then(|| {
                format!(
                    "{}T{:02}:{:02}:{:02}Z",
                    d.date(),
                    d.time().hour(),
                    d.time().minute(),
                    d.time().second()
                )
            })
        });
    let changed_by = update
        .revised_by
        .as_ref()
        .and_then(|by| by.graph_subject_base.display_name.clone())
        .or_else(|| changed("System.ChangedBy").and_then(identity_name));

    Some(HistoryEntry {
        rev: update.rev.unwrap_or_default(),
        date,
        changed_by,
        changes,
        links_added,
        links_removed,
    })
}

fn strip_html(value: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// Strips HTML from HTML fields, collapses whitespace and shortens.
fn plain_text(value: &str, html: bool) -> String {
    let text = if html {
        strip_html(value)
    } else {
        value.to_string()
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > MAX_TEXT_VALUE {
        let cut: String = text.chars().take(MAX_TEXT_VALUE).collect();
        format!("{cut}…")
    } else {
        text
    }
}

/// A field value as text output shows it.
fn value_text(value: Option<&Value>, html: bool) -> String {
    let text = match value {
        None | Some(Value::Null) => return "(none)".to_string(),
        Some(Value::String(s)) => plain_text(s, html),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        Some(other) => identity_name(other).unwrap_or_else(|| other.to_string()),
    };
    if text.is_empty() {
        "(none)".to_string()
    } else {
        text
    }
}

fn change_line(change: &FieldChange) -> String {
    let html = is_html_field(&change.field);
    format!(
        "{}: {} → {}",
        change.name,
        value_text(change.old_value.as_ref(), html),
        value_text(change.new_value.as_ref(), html)
    )
}

/// Escapes control characters and angle brackets, so remote text cannot drive
/// the terminal or render as HTML in Markdown.
fn md_text(text: &str) -> String {
    escape_control_characters(text)
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn link_line(sign: char, link: &LinkChange) -> String {
    format!("{sign} {}: {}", link.name, link.target)
}

/// Every change of an entry as a line of text.
fn lines(entry: &HistoryEntry) -> Vec<String> {
    entry
        .changes
        .iter()
        .map(change_line)
        .chain(entry.links_added.iter().map(|l| link_line('+', l)))
        .chain(entry.links_removed.iter().map(|l| link_line('-', l)))
        .collect()
}

impl Render for History {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.updates.is_empty() {
            return writeln!(w, "No history found for work item {}.", self.work_item_id);
        }
        for entry in &self.updates {
            writeln!(
                w,
                "Rev {}  {}  {}",
                entry.rev,
                entry.date.as_deref().map_or("-".into(), format_date),
                escape_control_characters(entry.changed_by.as_deref().unwrap_or("-"))
            )?;
            for line in lines(entry) {
                writeln!(w, "  {}", escape_control_characters(&line))?;
            }
        }
        writeln!(w, "\n{} update(s)", self.updates.len())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let rows: Vec<Vec<String>> = self
            .updates
            .iter()
            .flat_map(|entry| {
                lines(entry).into_iter().map(move |line| {
                    vec![
                        entry.rev.to_string(),
                        entry.date.as_deref().map_or("-".into(), format_date),
                        md_text(entry.changed_by.as_deref().unwrap_or("-")),
                        md_text(&line),
                    ]
                })
            })
            .collect();
        write!(
            w,
            "{}",
            md_table(&["Rev", "Date", "Changed by", "Change"], &rows)
        )
    }
}

/// Every update of the work item, oldest first, fetched page by page.
async fn fetch_updates(conn: &Connection, project: &str, id: i32) -> Result<Vec<WorkItemUpdate>> {
    let client = conn.wit();
    let mut all = Vec::new();
    loop {
        let page = client
            .updates_client()
            .list(conn.organization(), id, project)
            .top(PAGE_SIZE)
            .skip(all.len() as i32)
            .await?
            .value;
        let count = page.len();
        all.extend(page);
        if count < PAGE_SIZE as usize {
            return Ok(all);
        }
    }
}

pub async fn show(
    conn: &Connection,
    id: &str,
    project: &str,
    fields: &[String],
    top: Option<usize>,
    format: OutputFormat,
) -> Result<()> {
    let result = async {
        let work_item_id = parse_work_item_id(id)?;
        let updates = fetch_updates(conn, project, work_item_id).await?;
        Ok::<_, anyhow::Error>(History {
            work_item_id,
            updates: build_entries(&updates, fields, top),
        })
    }
    .await;
    match result {
        Ok(history) => output::emit(&history, format),
        Err(e) => {
            eprintln!("❌ Failed to get the history of work item {id}");
            eprintln!("   {e}");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn updates(value: Value) -> Vec<WorkItemUpdate> {
        serde_json::from_value(value).unwrap()
    }

    fn sample() -> Vec<WorkItemUpdate> {
        updates(json!([
            {
                "id": 1, "workItemId": 7, "url": "https://x/updates/1", "rev": 1,
                "revisedBy": {"displayName": "Jane Doe"},
                "revisedDate": "2026-11-01T09:00:00Z",
                "fields": {
                    "System.Rev": {"newValue": 1},
                    "System.State": {"newValue": "New"},
                    "System.ChangedDate": {"newValue": "2026-11-01T09:00:00.5Z"}
                }
            },
            {
                "id": 2, "workItemId": 7, "url": "https://x/updates/2", "rev": 2,
                "revisedBy": {"displayName": "Jane Doe"},
                "revisedDate": "2026-11-02T10:30:15Z",
                "fields": {
                    "System.Rev": {"oldValue": 1, "newValue": 2},
                    "System.State": {"oldValue": "New", "newValue": "Active"},
                    "Microsoft.VSTS.Scheduling.RemainingWork":
                        {"oldValue": 8.0, "newValue": 5.0},
                    "System.AssignedTo":
                        {"newValue": {"displayName": "Jane Doe", "uniqueName": "j@x"}}
                }
            },
            {
                "id": 3, "workItemId": 7, "url": "https://x/updates/3", "rev": 3,
                "revisedBy": {"displayName": "Bob"},
                "revisedDate": "2026-11-03T08:00:00Z",
                "fields": {"System.Watermark": {"oldValue": 1, "newValue": 2}},
                "relations": {"added": [{
                    "rel": "System.LinkTypes.Hierarchy-Reverse",
                    "url": "https://dev.azure.com/o/_apis/wit/workItems/12",
                    "attributes": {"name": "Parent"}
                }]}
            }
        ]))
    }

    #[test]
    fn display_name_splits_camel_case_of_the_last_segment() {
        assert_eq!(
            display_name("Microsoft.VSTS.Scheduling.RemainingWork"),
            "Remaining Work"
        );
        assert_eq!(display_name("System.State"), "State");
        assert_eq!(display_name("System.AssignedTo"), "Assigned To");
    }

    #[test]
    fn field_matches_reference_short_and_readable_names() {
        let r = "Microsoft.VSTS.Scheduling.RemainingWork";
        assert!(field_matches("microsoft.vsts.scheduling.remainingwork", r));
        assert!(field_matches("RemainingWork", r));
        assert!(field_matches("remaining work", r));
        assert!(!field_matches("State", r));
    }

    #[test]
    fn entries_are_newest_first_and_skip_noise() {
        let entries = build_entries(&sample(), &[], None);
        let revs: Vec<i32> = entries.iter().map(|e| e.rev).collect();
        assert_eq!(revs, vec![3, 2, 1]);
        let fields: Vec<&str> = entries[1]
            .changes
            .iter()
            .map(|c| c.field.as_str())
            .collect();
        assert!(!fields.contains(&"System.Rev"));
        assert_eq!(fields.len(), 3);
    }

    #[test]
    fn entry_uses_change_date_and_revised_by() {
        let entries = build_entries(&sample(), &[], None);
        assert_eq!(entries[2].date.as_deref(), Some("2026-11-01T09:00:00.5Z"));
        assert_eq!(entries[1].date.as_deref(), Some("2026-11-02T10:30:15Z"));
        assert_eq!(entries[1].changed_by.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn changes_from_nothing_to_nothing_are_skipped() {
        let updates = updates(json!([{
            "id": 1, "workItemId": 7, "url": "https://x/u/1", "rev": 1,
            "fields": {
                "System.Tags": {"newValue": ""},
                "System.State": {"newValue": "New"}
            }
        }]));
        let entries = build_entries(&updates, &[], None);
        assert_eq!(entries[0].changes.len(), 1);
    }

    #[test]
    fn plain_text_fields_keep_angle_brackets() {
        assert_eq!(plain_text("Latency < 5 ms", false), "Latency < 5 ms");
    }

    #[test]
    fn changes_past_the_shortened_text_are_kept() {
        let a = format!("{}a", "x".repeat(100));
        let b = format!("{}b", "x".repeat(100));
        let updates = updates(json!([{
            "id": 1, "workItemId": 7, "url": "https://x/u/1", "rev": 2,
            "fields": {"System.Description": {"oldValue": a, "newValue": b}}
        }]));
        assert_eq!(build_entries(&updates, &[], None)[0].changes.len(), 1);
    }

    #[test]
    fn identities_with_the_same_name_are_still_a_change() {
        let updates = updates(json!([{
            "id": 1, "workItemId": 7, "url": "https://x/u/1", "rev": 2,
            "fields": {"System.AssignedTo": {
                "oldValue": {"displayName": "Jo", "id": "1"},
                "newValue": {"displayName": "Jo", "id": "2"}
            }}
        }]));
        assert_eq!(build_entries(&updates, &[], None)[0].changes.len(), 1);
    }

    #[test]
    fn field_matches_a_label_with_spaces() {
        assert!(field_matches("API Key", "Custom.APIKey"));
    }

    #[test]
    fn hyperlinks_keep_their_url() {
        let url = "https://tracker.example.com/workItems/42";
        assert_eq!(link_target("Hyperlink", url), url);
        assert_eq!(
            link_target(
                "ArtifactLink",
                "https://dev.azure.com/o/_apis/wit/workItems/42"
            ),
            "#42"
        );
    }

    #[test]
    fn markdown_escapes_tag_like_text() {
        assert_eq!(
            md_text("Check <b>this</b>"),
            "Check &lt;b&gt;this&lt;/b&gt;"
        );
    }

    #[test]
    fn markdown_shows_a_tag_like_title_literally() {
        let updates = updates(json!([{
            "id": 1, "workItemId": 7, "url": "https://x/u/1", "rev": 2,
            "fields": {"System.Title": {
                "oldValue": "Old", "newValue": "Check <b>this</b>"
            }}
        }]));
        let history = History {
            work_item_id: 7,
            updates: build_entries(&updates, &[], None),
        };
        let mut out = Vec::new();
        history.markdown(&mut out).unwrap();
        let md = String::from_utf8(out).unwrap();
        assert!(md.contains("Title: Old → Check &lt;b&gt;this&lt;/b&gt;"));
        assert!(!md.contains("<b>"));
    }

    #[test]
    fn markdown_escapes_control_characters() {
        let history = History {
            work_item_id: 7,
            updates: vec![HistoryEntry {
                rev: 1,
                date: None,
                changed_by: Some("\u{1b}[31mEvil".into()),
                changes: vec![],
                links_added: vec![LinkChange {
                    rel: "r".into(),
                    name: "n".into(),
                    target: "\u{1b}[0m".into(),
                }],
                links_removed: vec![],
            }],
        };
        let mut out = Vec::new();
        history.markdown(&mut out).unwrap();
        assert!(!String::from_utf8(out).unwrap().contains('\u{1b}'));
    }

    #[test]
    fn top_keeps_the_newest_updates() {
        let entries = build_entries(&sample(), &[], Some(1));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rev, 3);
    }

    #[test]
    fn field_filter_keeps_only_matching_changes_and_updates() {
        let entries = build_entries(&sample(), &["state".to_string()], None);
        let revs: Vec<i32> = entries.iter().map(|e| e.rev).collect();
        assert_eq!(revs, vec![2, 1]);
        assert_eq!(entries[0].changes.len(), 1);
        assert!(entries[0].links_added.is_empty());
    }

    #[test]
    fn top_applies_after_the_field_filter() {
        let entries = build_entries(&sample(), &["state".to_string()], Some(1));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rev, 2);
    }

    #[test]
    fn link_changes_are_reported() {
        let entries = build_entries(&sample(), &[], None);
        assert_eq!(
            entries[0].links_added,
            vec![LinkChange {
                rel: "System.LinkTypes.Hierarchy-Reverse".into(),
                name: "Parent".into(),
                target: "#12".into(),
            }]
        );
    }

    #[test]
    fn value_text_formats_each_kind_of_value() {
        assert_eq!(value_text(None, false), "(none)");
        assert_eq!(value_text(Some(&json!(null)), false), "(none)");
        assert_eq!(value_text(Some(&json!(8.0)), false), "8");
        assert_eq!(value_text(Some(&json!(2.5)), false), "2.5");
        assert_eq!(value_text(Some(&json!("Active")), false), "Active");
        assert_eq!(
            value_text(Some(&json!({"displayName": "Jane Doe"})), false),
            "Jane Doe"
        );
    }

    #[test]
    fn plain_text_strips_html_and_shortens() {
        assert_eq!(plain_text("<div>a &amp; b</div><p>c</p>", true), "a & b c");
        let long = "x".repeat(200);
        assert_eq!(plain_text(&long, true).chars().count(), MAX_TEXT_VALUE + 1);
    }

    fn render_to_string(format: OutputFormat) -> String {
        let history = History {
            work_item_id: 7,
            updates: build_entries(&sample(), &[], None),
        };
        let mut out = Vec::new();
        output::render(&history, format, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn text_shows_each_change_with_old_and_new_value() {
        let text = render_to_string(OutputFormat::Text);
        assert!(text.contains("Rev 2  2026-11-02 10:30:15  Jane Doe"));
        assert!(text.contains("  State: New → Active"));
        assert!(text.contains("  Remaining Work: 8 → 5"));
        assert!(text.contains("  Assigned To: (none) → Jane Doe"));
        assert!(text.contains("  + Parent: #12"));
        assert!(text.contains("3 update(s)"));
    }

    #[test]
    fn text_escapes_control_characters() {
        let history = History {
            work_item_id: 7,
            updates: vec![HistoryEntry {
                rev: 1,
                date: None,
                changed_by: Some("\u{1b}[31mEvil".into()),
                changes: vec![],
                links_added: vec![],
                links_removed: vec![],
            }],
        };
        let mut out = Vec::new();
        history.text(&mut out).unwrap();
        assert!(!String::from_utf8(out).unwrap().contains('\u{1b}'));
    }

    #[test]
    fn empty_history_says_so() {
        let history = History {
            work_item_id: 7,
            updates: vec![],
        };
        let mut out = Vec::new();
        history.text(&mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "No history found for work item 7.\n"
        );
    }

    #[test]
    fn markdown_has_one_row_per_change() {
        let md = render_to_string(OutputFormat::Markdown);
        assert!(md.starts_with("| Rev | Date | Changed by | Change |\n"));
        assert!(md.contains("| 2 | 2026-11-02 10:30:15 | Jane Doe | State: New → Active |"));
    }

    #[test]
    fn json_keeps_raw_values() {
        let json: Value = serde_json::from_str(&render_to_string(OutputFormat::Json)).unwrap();
        assert_eq!(json["workItemId"], 7);
        let assigned = &json["updates"][1]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["field"] == "System.AssignedTo")
            .unwrap()["newValue"];
        assert_eq!(assigned["uniqueName"], "j@x");
    }
}
