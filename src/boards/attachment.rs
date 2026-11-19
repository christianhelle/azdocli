//! Work item attachments: uploading files to attach on create and update,
//! and listing and downloading what is attached.

use super::{add_relation_op, create_wit_client, parse_work_item_id};
use crate::auth::{get_credentials, Credentials};
use crate::migrate::http_client;
use crate::output::{self, md_table, OutputFormat, Render};
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{anyhow, bail, Context, Result};
use azure_devops_rust_api::wit::models::{JsonPatchOperation, WorkItem};
use clap::Subcommand;
use colored::Colorize;
use serde::Serialize;
use serde_json::json;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The relation type Azure DevOps uses for an attached file.
const ATTACHED_FILE: &str = "AttachedFile";

#[derive(Subcommand, Clone)]
pub enum AttachmentSubCommands {
    /// List the files attached to a work item
    List {
        /// ID of the work item
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Download the files attached to a work item
    Download {
        /// ID of the work item
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Only download the attachment with this file name (default: all)
        #[clap(long)]
        name: Option<String>,
        /// Directory to save the files to (default: current directory)
        #[clap(long, default_value = ".")]
        dir: PathBuf,
    },
}

/// A file attached to a work item.
#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct Attachment {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub url: String,
}

/// The attachments among a work item's relations.
fn attachments(work_item: &WorkItem) -> Vec<Attachment> {
    work_item
        .relations
        .iter()
        .map(|relation| &relation.link)
        .filter(|link| link.rel == ATTACHED_FILE)
        .filter_map(|link| {
            let id = attachment_id_from_url(&link.url)?;
            let name = link
                .attributes
                .get("name")
                .and_then(|name| name.as_str())
                .unwrap_or(&id)
                .to_string();
            Some(Attachment {
                size: link
                    .attributes
                    .get("resourceSize")
                    .and_then(|size| size.as_u64()),
                url: link.url.clone(),
                id,
                name,
            })
        })
        .collect()
}

/// The attachment id in an `.../_apis/wit/attachments/{id}` URL.
fn attachment_id_from_url(url: &str) -> Option<String> {
    // The full route, so a collection or organization named "attachments"
    // earlier in the URL is not mistaken for it.
    let marker = "/_apis/wit/attachments/";
    let start = url.rfind(marker)? + marker.len();
    let rest = &url[start..];
    let id = &rest[..rest.find(['?', '/', '&']).unwrap_or(rest.len())];
    (!id.is_empty()).then(|| id.to_string())
}

/// The file name to save an attachment under: its last path component, so a
/// name like `../../x` cannot write outside the target directory. Separators
/// follow the host's path rules, so a backslash is kept in a Unix file name.
fn safe_file_name(name: &str) -> Result<String> {
    Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Attachment name '{name}' is not a valid file name"))
}

/// The patch operation that attaches an uploaded file.
pub(crate) fn attach_op(url: &str, name: &str) -> JsonPatchOperation {
    add_relation_op(ATTACHED_FILE, url, Some(json!({ "name": name })))
}

/// Uploads each file and returns the patch operations that attach them.
pub(crate) async fn upload_all(
    project: &str,
    paths: &[PathBuf],
    format: OutputFormat,
) -> Result<Vec<JsonPatchOperation>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let creds = get_credentials()?;
    let http = http_client::client();
    let mut ops = Vec::new();
    for path in paths {
        let (name, url) = upload(&http, &creds, project, path).await?;
        if format == OutputFormat::Text {
            println!("📎 Uploaded {name}");
        }
        ops.push(attach_op(&url, &name));
    }
    Ok(ops)
}

async fn upload(
    http: &reqwest::Client,
    creds: &Credentials,
    project: &str,
    path: &Path,
) -> Result<(String, String)> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("'{}' is not a file", path.display()))?
        .to_string();
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("Failed to read '{}'", path.display()))?;
    let stream = futures::stream::once(async move { Ok::<_, io::Error>(bytes) });
    let uploaded = http_client::upload_attachment_stream(http, creds, project, &name, stream)
        .await
        .with_context(|| format!("Failed to upload '{}'", path.display()))?;
    let url = uploaded
        .url
        .ok_or_else(|| anyhow!("Upload of '{name}' did not return a URL"))?;
    Ok((name, url))
}

async fn get_attachments(project: &str, id: &str) -> Result<Vec<Attachment>> {
    let creds = get_credentials()?;
    let work_item = create_wit_client()?
        .work_items_client()
        .get_work_item(creds.organization, parse_work_item_id(id)?, project)
        .expand("relations")
        .await?;
    Ok(attachments(&work_item))
}

/// The attachments `--name` picks, or all of them.
fn select<'a>(attachments: &'a [Attachment], name: Option<&str>) -> Result<Vec<&'a Attachment>> {
    let Some(name) = name else {
        return Ok(attachments.iter().collect());
    };
    let selected: Vec<_> = attachments
        .iter()
        .filter(|attachment| attachment.name.eq_ignore_ascii_case(name))
        .collect();
    if selected.is_empty() {
        bail!("No attachment named '{name}'");
    }
    Ok(selected)
}

async fn download(project: &str, id: &str, name: Option<&str>, dir: &Path) -> Result<Vec<PathBuf>> {
    let attachments = get_attachments(project, id).await?;
    let creds = get_credentials()?;
    let http = http_client::client();
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("Failed to create '{}'", dir.display()))?;

    let mut saved = Vec::new();
    for attachment in select(&attachments, name)? {
        let target = dir.join(safe_file_name(&attachment.name)?);
        let bytes = http_client::download_attachment(&http, &creds, project, &attachment.id)
            .await?
            .bytes()
            .await
            .with_context(|| format!("Failed to download '{}'", attachment.name))?;
        save_new(&target, &bytes).await?;
        saved.push(target);
    }
    Ok(saved)
}

/// Writes `bytes` to a file that must not exist yet. Creating it exclusively
/// never follows a symlink at `target` and never overwrites a local file or
/// an earlier attachment with the same name.
async fn save_new(target: &Path, bytes: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .await
        .map_err(|e| match e.kind() {
            io::ErrorKind::AlreadyExists => anyhow!(
                "'{}' already exists; remove it or pick another --dir",
                target.display()
            ),
            _ => anyhow!("Failed to create '{}': {e}", target.display()),
        })?;
    file.write_all(bytes)
        .await
        .with_context(|| format!("Failed to write '{}'", target.display()))?;
    file.flush().await?;
    Ok(())
}

/// The attachments of a work item, as `attachment list` prints them.
#[derive(Serialize)]
#[serde(transparent)]
struct AttachmentList(Vec<Attachment>);

fn size_text(size: Option<u64>) -> String {
    size.map(|size| size.to_string())
        .unwrap_or_else(|| "-".into())
}

impl Render for AttachmentList {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.0.is_empty() {
            return writeln!(w, "No attachments found.");
        }
        writeln!(w, "{:<40} {:>12}  ID", "Name", "Size (bytes)")?;
        writeln!(w, "{}", "-".repeat(92))?;
        for attachment in &self.0 {
            writeln!(
                w,
                "{:<40} {:>12}  {}",
                escape_control_characters(&attachment.name),
                size_text(attachment.size),
                attachment.id
            )?;
        }
        writeln!(w, "\n{} attachment(s)", self.0.len())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let rows: Vec<Vec<String>> = self
            .0
            .iter()
            .map(|a| vec![a.name.clone(), size_text(a.size), a.id.clone()])
            .collect();
        write!(w, "{}", md_table(&["Name", "Size (bytes)", "ID"], &rows))
    }
}

pub async fn handle_command(
    subcommand: &AttachmentSubCommands,
    format: OutputFormat,
) -> Result<()> {
    match subcommand {
        AttachmentSubCommands::List { id, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match get_attachments(&project_name, id).await {
                Ok(attachments) => output::emit(&AttachmentList(attachments), format),
                Err(e) => {
                    eprintln!("❌ Failed to list the attachments of work item {id}");
                    eprintln!("   {e}");
                    Err(e)
                }
            }
        }
        AttachmentSubCommands::Download {
            id,
            project,
            name,
            dir,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match download(&project_name, id, name.as_deref(), dir).await {
                Ok(saved) if saved.is_empty() => {
                    println!("No attachments found.");
                    Ok(())
                }
                Ok(saved) => {
                    for path in &saved {
                        println!("📥 Saved {}", path.display());
                    }
                    println!(
                        "{}",
                        format!("✅ Downloaded {} attachment(s)", saved.len()).green()
                    );
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Failed to download the attachments of work item {id}");
                    eprintln!("   {e}");
                    Err(e)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_item_with_relations(relations: serde_json::Value) -> WorkItem {
        serde_json::from_value(json!({
            "id": 7,
            "url": "https://dev.azure.com/org/_apis/wit/workItems/7",
            "fields": {},
            "relations": relations,
        }))
        .unwrap()
    }

    #[test]
    fn attachments_are_read_from_attached_file_relations() {
        let work_item = work_item_with_relations(json!([
            {
                "rel": "AttachedFile",
                "url": "https://dev.azure.com/org/_apis/wit/attachments/abc-123",
                "attributes": { "name": "spec.md", "resourceSize": 42 }
            },
            {
                "rel": "System.LinkTypes.Hierarchy-Reverse",
                "url": "https://dev.azure.com/org/_apis/wit/workItems/1",
                "attributes": {}
            }
        ]));

        assert_eq!(
            attachments(&work_item),
            [Attachment {
                id: "abc-123".into(),
                name: "spec.md".into(),
                size: Some(42),
                url: "https://dev.azure.com/org/_apis/wit/attachments/abc-123".into(),
            }]
        );
    }

    #[test]
    fn attachment_id_stops_at_the_query() {
        assert_eq!(
            attachment_id_from_url("https://x/_apis/wit/attachments/abc?fileName=a.png"),
            Some("abc".into())
        );
        assert_eq!(
            attachment_id_from_url("https://x/_apis/wit/workItems/1"),
            None
        );
    }

    #[tokio::test]
    async fn save_new_writes_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("spec.md");
        save_new(&target, b"spec").await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"spec");
    }

    #[tokio::test]
    async fn save_new_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("spec.md");
        std::fs::write(&target, b"local").unwrap();

        let err = save_new(&target, b"remote").await.unwrap_err();

        assert!(err.to_string().contains("already exists"));
        assert_eq!(std::fs::read(&target).unwrap(), b"local");
    }

    #[test]
    fn attachment_id_ignores_a_collection_named_attachments() {
        assert_eq!(
            attachment_id_from_url("https://server/tfs/attachments/_apis/wit/attachments/abc"),
            Some("abc".into())
        );
    }

    #[test]
    fn safe_file_name_drops_directories() {
        assert_eq!(safe_file_name("../../etc/passwd").unwrap(), "passwd");
        assert_eq!(safe_file_name("spec.md").unwrap(), "spec.md");
        assert!(safe_file_name("..").is_err());
        assert!(safe_file_name("").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn safe_file_name_treats_backslashes_as_separators_on_windows() {
        assert_eq!(safe_file_name("..\\..\\boot.ini").unwrap(), "boot.ini");
    }

    #[cfg(unix)]
    #[test]
    fn safe_file_name_keeps_backslashes_on_unix() {
        assert_eq!(safe_file_name("weird\\name").unwrap(), "weird\\name");
    }

    #[test]
    fn attach_op_adds_an_attached_file_relation() {
        let op = attach_op("https://x/_apis/wit/attachments/abc", "spec.md");
        assert_eq!(op.path.as_deref(), Some("/relations/-"));
        assert_eq!(
            op.value,
            Some(json!({
                "rel": "AttachedFile",
                "url": "https://x/_apis/wit/attachments/abc",
                "attributes": { "name": "spec.md" }
            }))
        );
    }

    fn attachment(name: &str) -> Attachment {
        Attachment {
            id: name.into(),
            name: name.into(),
            size: None,
            url: String::new(),
        }
    }

    #[test]
    fn select_picks_by_name_or_all() {
        let all = [attachment("a.md"), attachment("b.png")];
        assert_eq!(select(&all, None).unwrap().len(), 2);
        assert_eq!(select(&all, Some("B.PNG")).unwrap()[0].name, "b.png");
        assert!(select(&all, Some("c.txt")).is_err());
    }

    #[test]
    fn attachment_list_text_shows_name_size_and_id() {
        let mut out = Vec::new();
        AttachmentList(vec![Attachment {
            size: Some(42),
            ..attachment("spec.md")
        }])
        .text(&mut out)
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("spec.md"));
        assert!(text.contains("42"));
        assert!(text.contains("1 attachment(s)"));
    }
}
