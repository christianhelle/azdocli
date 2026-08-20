use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::wit::models::{
    work_item_classification_node, WorkItemClassificationNode, WorkItemTrackingResource,
    WorkItemTrackingResourceReference,
};

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

const CLASSIFICATION_DEPTH: i32 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClassificationGroup {
    Areas,
    Iterations,
}

impl ClassificationGroup {
    fn state_key(self) -> &'static str {
        match self {
            Self::Areas => "areas",
            Self::Iterations => "iterations",
        }
    }

    fn structure_type(self) -> work_item_classification_node::StructureType {
        match self {
            Self::Areas => work_item_classification_node::StructureType::Area,
            Self::Iterations => work_item_classification_node::StructureType::Iteration,
        }
    }
}

#[derive(Debug)]
struct ClassificationNodeToCreate {
    name: String,
    parent_route_path: String,
    source_path: String,
    target_path: String,
    attributes: Option<serde_json::Value>,
}

pub struct AreasPhase;

#[async_trait]
impl Phase for AreasPhase {
    fn name(&self) -> &'static str {
        "areas"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        execute_classification_phase(ctx, ClassificationGroup::Areas, "areas").await
    }
}

pub(super) async fn execute_classification_phase(
    ctx: &mut MigrationContext,
    group: ClassificationGroup,
    route_segment: &'static str,
) -> Result<PhaseSummary> {
    let source_client = ctx.source_factory().build_wit();
    let target_client = ctx.target_factory().build_wit();

    let source_roots = source_client
        .classification_nodes_client()
        .get_root_nodes(&ctx.source_creds.organization, &ctx.opts.source_project)
        .depth(CLASSIFICATION_DEPTH)
        .await
        .map_err(|e| anyhow!("Fetching source {route_segment} classification tree: {e}"))?;

    let root = source_roots
        .value
        .iter()
        .find(|node| node.structure_type.as_ref() == Some(&group.structure_type()))
        .ok_or_else(|| anyhow!("Source {route_segment} classification root not found"))?;

    let mut nodes = Vec::new();
    for child in &root.children {
        collect_nodes(
            child,
            group,
            Vec::new(),
            String::new(),
            &ctx.opts.source_project,
            &ctx.opts.target_project,
            &mut nodes,
        )?;
    }

    let mut summary = PhaseSummary {
        items_total: nodes.len() as u64,
        ..Default::default()
    };

    let mut mappings = vec![(
        ctx.opts.source_project.clone(),
        ctx.opts.target_project.clone(),
    )];

    for node in nodes {
        if ctx.opts.dry_run {
            mappings.push((node.source_path, node.target_path));
            summary.record_success();
            continue;
        }

        let payload = new_classification_node(&node.name, group, node.attributes.clone());
        match target_client
            .classification_nodes_client()
            .create_or_update(
                &ctx.target_creds.organization,
                payload,
                &ctx.opts.target_project,
                route_segment,
                &node.parent_route_path,
            )
            .await
        {
            Ok(created) => {
                let target_path =
                    logical_path_from_node(&created).unwrap_or_else(|| node.target_path.clone());
                mappings.push((node.source_path, target_path));
                summary.record_success();
            }
            Err(e) => {
                summary.record_failure(format!(
                    "Creating target {route_segment} node '{}': {e}",
                    node.target_path
                ));
            }
        }
    }

    let id_map = ctx.state.id_map_mut(group.state_key());
    for (source_path, target_path) in mappings {
        id_map.map.insert(source_path, target_path);
    }

    Ok(summary)
}

fn collect_nodes(
    node: &WorkItemClassificationNode,
    group: ClassificationGroup,
    parent_segments: Vec<String>,
    parent_route_path: String,
    source_project: &str,
    target_project: &str,
    nodes: &mut Vec<ClassificationNodeToCreate>,
) -> Result<()> {
    let name = node
        .name
        .clone()
        .ok_or_else(|| anyhow!("Source classification node is missing a name"))?;

    let mut relative_segments = parent_segments;
    relative_segments.push(name.clone());

    let source_path = logical_path(source_project, &relative_segments);
    let target_path = logical_path(target_project, &relative_segments);
    let next_parent_route_path = route_path(&relative_segments);
    let children = node.children.clone();

    nodes.push(ClassificationNodeToCreate {
        name,
        parent_route_path,
        source_path,
        target_path,
        attributes: attributes_for_group(node, group),
    });

    for child in &children {
        collect_nodes(
            child,
            group,
            relative_segments.clone(),
            next_parent_route_path.clone(),
            source_project,
            target_project,
            nodes,
        )?;
    }

    Ok(())
}

fn new_classification_node(
    name: &str,
    group: ClassificationGroup,
    attributes: Option<serde_json::Value>,
) -> WorkItemClassificationNode {
    let mut node = WorkItemClassificationNode::new(WorkItemTrackingResource::new(
        WorkItemTrackingResourceReference::new(String::new()),
    ));
    node.name = Some(name.to_string());
    node.structure_type = Some(group.structure_type());
    node.attributes = attributes;
    node
}

fn attributes_for_group(
    node: &WorkItemClassificationNode,
    group: ClassificationGroup,
) -> Option<serde_json::Value> {
    match group {
        ClassificationGroup::Areas => None,
        ClassificationGroup::Iterations => node.attributes.clone(),
    }
}

fn logical_path(project: &str, relative_segments: &[String]) -> String {
    let mut parts = Vec::with_capacity(relative_segments.len() + 1);
    parts.push(project.to_string());
    parts.extend(relative_segments.iter().cloned());
    parts.join("\\")
}

fn logical_path_from_node(node: &WorkItemClassificationNode) -> Option<String> {
    node.path.as_ref().map(|path| {
        path.trim_start_matches('\\')
            .replace("\\Area\\", "\\")
            .replace("\\Iteration\\", "\\")
    })
}

fn route_path(relative_segments: &[String]) -> String {
    relative_segments
        .iter()
        .map(|segment| percent_encode_path_segment(segment))
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode_path_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
