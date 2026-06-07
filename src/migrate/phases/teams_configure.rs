use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::wit::{
    models::{work_item_classification_node, WorkItemClassificationNode},
    ClientBuilder as WitClientBuilder,
};
use azure_devops_rust_api::work::{
    models::{
        team_setting, team_settings_patch, BoardColumn, BoardReference, BoardRow, TeamFieldValue,
        TeamFieldValuesPatch, TeamSetting, TeamSettingsIteration, TeamSettingsPatch,
    },
    Client as WorkClient, ClientBuilder as WorkClientBuilder,
};
use std::collections::{HashMap, HashSet};

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

const CLASSIFICATION_DEPTH: i32 = 50;
const SETTINGS_PER_TEAM: u64 = 4;

pub struct TeamsConfigurePhase;

#[async_trait]
impl Phase for TeamsConfigurePhase {
    fn name(&self) -> &'static str {
        "teams_configure"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let team_map = ctx
            .state
            .id_map("teams")
            .map(|id_map| id_map.map.clone())
            .unwrap_or_default();
        let mut teams = team_map.into_iter().collect::<Vec<_>>();
        teams.sort_by(|a, b| a.0.cmp(&b.0));

        let mut summary = PhaseSummary {
            items_total: teams.len() as u64 * SETTINGS_PER_TEAM,
            ..Default::default()
        };

        if teams.is_empty() {
            return Ok(summary);
        }

        let source_client = WorkClientBuilder::new(ctx.source_credential.clone()).build();
        let target_client = WorkClientBuilder::new(ctx.target_credential.clone()).build();
        let target_iteration_ids = match target_iteration_ids_by_path(ctx).await {
            Ok(ids) => ids,
            Err(e) => {
                println!("  ⚠ unable to pre-load target iteration ids: {e}");
                HashMap::new()
            }
        };

        for (source_team_id, target_team_id) in teams {
            record_setting(
                &mut summary,
                configure_team_field_values(
                    ctx,
                    &source_client,
                    &target_client,
                    &source_team_id,
                    &target_team_id,
                )
                .await,
                &source_team_id,
                "team field values",
            );
            record_setting(
                &mut summary,
                configure_team_iterations(
                    ctx,
                    &source_client,
                    &target_client,
                    &target_iteration_ids,
                    &source_team_id,
                    &target_team_id,
                )
                .await,
                &source_team_id,
                "iterations",
            );
            record_setting(
                &mut summary,
                configure_team_settings(
                    ctx,
                    &source_client,
                    &target_client,
                    &target_iteration_ids,
                    &source_team_id,
                    &target_team_id,
                )
                .await,
                &source_team_id,
                "team settings",
            );
            record_setting(
                &mut summary,
                configure_board_settings(
                    ctx,
                    &source_client,
                    &target_client,
                    &source_team_id,
                    &target_team_id,
                )
                .await,
                &source_team_id,
                "board settings",
            );
        }

        Ok(summary)
    }
}

fn record_setting(
    summary: &mut PhaseSummary,
    result: Result<()>,
    source_team_id: &str,
    setting_name: &str,
) {
    match result {
        Ok(()) => summary.record_success(),
        Err(e) => {
            let message =
                format!("Configuring {setting_name} for source team '{source_team_id}': {e}");
            println!("  ⚠ {message}");
            summary.record_failure(message);
        }
    }
}

async fn configure_team_field_values(
    ctx: &MigrationContext,
    source_client: &WorkClient,
    target_client: &WorkClient,
    source_team_id: &str,
    target_team_id: &str,
) -> Result<()> {
    let source_values = source_client
        .teamfieldvalues_client()
        .get(
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
            source_team_id,
        )
        .await
        .map_err(|e| anyhow!("reading source team field values: {e}"))?;

    let mut patch = TeamFieldValuesPatch::new();
    patch.default_value = remap_optional_path(
        source_values.default_value.as_deref(),
        ctx,
        "areas",
        "default area path",
    )?;
    patch.values = source_values
        .values
        .iter()
        .map(|value| remap_team_field_value(value, ctx))
        .collect::<Result<Vec<_>>>()?;

    if ctx.opts.dry_run {
        return Ok(());
    }

    target_client
        .teamfieldvalues_client()
        .update(
            &ctx.target_creds.organization,
            patch,
            &ctx.opts.target_project,
            target_team_id,
        )
        .await
        .map_err(|e| anyhow!("updating target team field values: {e}"))?;

    Ok(())
}

fn remap_team_field_value(
    value: &TeamFieldValue,
    ctx: &MigrationContext,
) -> Result<TeamFieldValue> {
    let mut mapped = value.clone();
    mapped.value = remap_optional_path(value.value.as_deref(), ctx, "areas", "area path")?;
    Ok(mapped)
}

async fn configure_team_iterations(
    ctx: &MigrationContext,
    source_client: &WorkClient,
    target_client: &WorkClient,
    target_iteration_ids: &HashMap<String, String>,
    source_team_id: &str,
    target_team_id: &str,
) -> Result<()> {
    let source_iterations = source_client
        .iterations_client()
        .list(
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
            source_team_id,
        )
        .await
        .map_err(|e| anyhow!("reading source team iterations: {e}"))?
        .value;

    if ctx.opts.dry_run {
        source_iterations.iter().try_for_each(|iteration| {
            target_iteration_for_source(iteration, ctx, target_iteration_ids).map(|_| ())
        })?;
        return Ok(());
    }

    let target_iterations = target_client
        .iterations_client()
        .list(
            &ctx.target_creds.organization,
            &ctx.opts.target_project,
            target_team_id,
        )
        .await
        .map_err(|e| anyhow!("reading target team iterations: {e}"))?
        .value;
    let existing_target_ids = target_iterations
        .iter()
        .filter_map(|iteration| iteration.id.as_deref())
        .collect::<HashSet<_>>();

    for source_iteration in source_iterations {
        let target_iteration =
            target_iteration_for_source(&source_iteration, ctx, target_iteration_ids)?;
        if target_iteration
            .id
            .as_deref()
            .is_some_and(|id| existing_target_ids.contains(id))
        {
            continue;
        }

        target_client
            .iterations_client()
            .post_team_iteration(
                &ctx.target_creds.organization,
                target_iteration,
                &ctx.opts.target_project,
                target_team_id,
            )
            .await
            .map_err(|e| anyhow!("adding target team iteration: {e}"))?;
    }

    Ok(())
}

async fn configure_team_settings(
    ctx: &MigrationContext,
    source_client: &WorkClient,
    target_client: &WorkClient,
    target_iteration_ids: &HashMap<String, String>,
    source_team_id: &str,
    target_team_id: &str,
) -> Result<()> {
    let source_settings = source_client
        .teamsettings_client()
        .get(
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
            source_team_id,
        )
        .await
        .map_err(|e| anyhow!("reading source team settings: {e}"))?;
    let patch = team_settings_patch(&source_settings, ctx, target_iteration_ids)?;

    if ctx.opts.dry_run {
        return Ok(());
    }

    target_client
        .teamsettings_client()
        .update(
            &ctx.target_creds.organization,
            patch,
            &ctx.opts.target_project,
            target_team_id,
        )
        .await
        .map_err(|e| anyhow!("updating target team settings: {e}"))?;

    Ok(())
}

fn team_settings_patch(
    source_settings: &TeamSetting,
    ctx: &MigrationContext,
    target_iteration_ids: &HashMap<String, String>,
) -> Result<TeamSettingsPatch> {
    let mut patch = TeamSettingsPatch::new();
    patch.backlog_iteration = source_settings
        .backlog_iteration
        .as_ref()
        .map(|iteration| target_iteration_id_for_source(iteration, ctx, target_iteration_ids))
        .transpose()?;
    patch.backlog_visibilities = source_settings.backlog_visibilities.clone();
    patch.bugs_behavior = source_settings
        .bugs_behavior
        .clone()
        .map(convert_bugs_behavior);
    patch.default_iteration = source_settings
        .default_iteration
        .as_ref()
        .map(|iteration| target_iteration_id_for_source(iteration, ctx, target_iteration_ids))
        .transpose()?;
    patch.default_iteration_macro = source_settings.default_iteration_macro.clone();
    patch.working_days = source_settings.working_days.clone();
    Ok(patch)
}

fn convert_bugs_behavior(value: team_setting::BugsBehavior) -> team_settings_patch::BugsBehavior {
    match value {
        team_setting::BugsBehavior::Off => team_settings_patch::BugsBehavior::Off,
        team_setting::BugsBehavior::AsRequirements => {
            team_settings_patch::BugsBehavior::AsRequirements
        }
        team_setting::BugsBehavior::AsTasks => team_settings_patch::BugsBehavior::AsTasks,
    }
}

fn target_iteration_for_source(
    source_iteration: &TeamSettingsIteration,
    ctx: &MigrationContext,
    target_iteration_ids: &HashMap<String, String>,
) -> Result<TeamSettingsIteration> {
    let mut target_iteration = source_iteration.clone();
    let target_path = source_iteration
        .path
        .as_deref()
        .map(|path| remap_path(path, ctx, "iterations", "iteration path"))
        .transpose()?;
    let target_id = target_iteration_id_for_source(source_iteration, ctx, target_iteration_ids)?;

    target_iteration.id = Some(target_id);
    target_iteration.path = target_path;
    target_iteration.team_settings_data_contract_base.links = None;
    target_iteration.team_settings_data_contract_base.url = None;
    Ok(target_iteration)
}

fn target_iteration_id_for_source(
    source_iteration: &TeamSettingsIteration,
    ctx: &MigrationContext,
    target_iteration_ids: &HashMap<String, String>,
) -> Result<String> {
    let source_path = source_iteration
        .path
        .as_deref()
        .ok_or_else(|| anyhow!("source iteration is missing a path"))?;
    let target_path = remap_path(source_path, ctx, "iterations", "iteration path")?;
    target_iteration_ids
        .get(&normalize_logical_path(&target_path))
        .cloned()
        .ok_or_else(|| anyhow!("target iteration id not found for '{target_path}'"))
}

async fn configure_board_settings(
    ctx: &MigrationContext,
    source_client: &WorkClient,
    target_client: &WorkClient,
    source_team_id: &str,
    target_team_id: &str,
) -> Result<()> {
    let source_boards = source_client
        .boards_client()
        .list(
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
            source_team_id,
        )
        .await
        .map_err(|e| anyhow!("listing source boards: {e}"))?
        .value;

    let target_boards = target_client
        .boards_client()
        .list(
            &ctx.target_creds.organization,
            &ctx.opts.target_project,
            target_team_id,
        )
        .await
        .map_err(|e| anyhow!("listing target boards: {e}"))?
        .value;

    let mut failures = Vec::new();
    for source_board in source_boards {
        let Some(source_board_key) = board_key(&source_board) else {
            failures.push("source board is missing both name and id".to_string());
            continue;
        };
        let Some(target_board_key) = matching_target_board_key(&source_board, &target_boards)
        else {
            failures.push(format!("target board not found for '{source_board_key}'"));
            continue;
        };

        if let Err(e) = configure_single_board(
            ctx,
            source_client,
            target_client,
            source_team_id,
            target_team_id,
            &source_board_key,
            &target_board_key,
        )
        .await
        {
            failures.push(format!("board '{source_board_key}': {e}"));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(failures.join("; ")))
    }
}

async fn configure_single_board(
    ctx: &MigrationContext,
    source_client: &WorkClient,
    target_client: &WorkClient,
    source_team_id: &str,
    target_team_id: &str,
    source_board: &str,
    target_board: &str,
) -> Result<()> {
    let source_columns = source_client
        .columns_client()
        .list(
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
            source_board,
            source_team_id,
        )
        .await
        .map_err(|e| anyhow!("reading source board columns: {e}"))?
        .value
        .into_iter()
        .map(sanitize_board_column)
        .collect::<Vec<_>>();

    let source_rows = source_client
        .rows_client()
        .list(
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
            source_board,
            source_team_id,
        )
        .await
        .map_err(|e| anyhow!("reading source board rows/swimlanes: {e}"))?
        .value
        .into_iter()
        .map(sanitize_board_row)
        .collect::<Vec<_>>();

    if ctx.opts.dry_run {
        return Ok(());
    }

    target_client
        .columns_client()
        .update(
            &ctx.target_creds.organization,
            source_columns,
            &ctx.opts.target_project,
            target_board,
            target_team_id,
        )
        .await
        .map_err(|e| anyhow!("updating target board columns: {e}"))?;

    target_client
        .rows_client()
        .update(
            &ctx.target_creds.organization,
            source_rows,
            &ctx.opts.target_project,
            target_board,
            target_team_id,
        )
        .await
        .map_err(|e| anyhow!("updating target board rows/swimlanes: {e}"))?;

    Ok(())
}

fn board_key(board: &BoardReference) -> Option<String> {
    board.name.clone().or_else(|| board.id.clone())
}

fn matching_target_board_key(
    source_board: &BoardReference,
    target_boards: &[BoardReference],
) -> Option<String> {
    if let Some(source_name) = &source_board.name {
        return target_boards
            .iter()
            .find(|target_board| target_board.name.as_ref() == Some(source_name))
            .and_then(board_key);
    }

    source_board.id.clone()
}

fn sanitize_board_column(mut column: BoardColumn) -> BoardColumn {
    column.id = None;
    column
}

fn sanitize_board_row(mut row: BoardRow) -> BoardRow {
    row.id = None;
    row
}

fn remap_optional_path(
    source_path: Option<&str>,
    ctx: &MigrationContext,
    map_name: &str,
    label: &str,
) -> Result<Option<String>> {
    source_path
        .map(|path| remap_path(path, ctx, map_name, label))
        .transpose()
}

fn remap_path(
    source_path: &str,
    ctx: &MigrationContext,
    map_name: &str,
    label: &str,
) -> Result<String> {
    let normalized = normalize_logical_path(source_path);
    if let Some(mapped) = ctx
        .state
        .id_map(map_name)
        .and_then(|id_map| id_map.map.get(&normalized))
    {
        return Ok(mapped.clone());
    }

    normalized
        .strip_prefix(&ctx.opts.source_project)
        .map(|suffix| format!("{}{}", ctx.opts.target_project, suffix))
        .ok_or_else(|| anyhow!("{label} '{source_path}' is not present in '{map_name}' id-map"))
}

fn normalize_logical_path(path: &str) -> String {
    path.trim_start_matches('\\')
        .replace("\\Area\\", "\\")
        .replace("\\Iteration\\", "\\")
}

async fn target_iteration_ids_by_path(ctx: &MigrationContext) -> Result<HashMap<String, String>> {
    let client = WitClientBuilder::new(ctx.target_credential.clone()).build();
    let target_roots = client
        .classification_nodes_client()
        .get_root_nodes(&ctx.target_creds.organization, &ctx.opts.target_project)
        .depth(CLASSIFICATION_DEPTH)
        .await
        .map_err(|e| anyhow!("fetching target iteration classification tree: {e}"))?;

    let mut ids = HashMap::new();
    for root in &target_roots.value {
        if root.structure_type.as_ref()
            == Some(&work_item_classification_node::StructureType::Iteration)
        {
            collect_iteration_ids(root, &ctx.opts.target_project, &mut ids);
        }
    }

    Ok(ids)
}

fn collect_iteration_ids(
    node: &WorkItemClassificationNode,
    target_project: &str,
    ids: &mut HashMap<String, String>,
) {
    if let Some(identifier) = &node.identifier {
        let path = node
            .path
            .as_ref()
            .map(|path| normalize_logical_path(path))
            .unwrap_or_else(|| target_project.to_string());
        ids.insert(path, identifier.clone());
    }

    for child in &node.children {
        collect_iteration_ids(child, target_project, ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_logical_path_removes_classification_markers() {
        assert_eq!(
            normalize_logical_path("\\TargetProject\\Iteration\\Sprint 1"),
            "TargetProject\\Sprint 1"
        );
        assert_eq!(
            normalize_logical_path("\\TargetProject\\Area\\Platform"),
            "TargetProject\\Platform"
        );
    }

    #[test]
    fn board_matching_prefers_name() {
        let source = board(Some("Stories"), Some("source-id"));
        let targets = vec![board(Some("Stories"), Some("target-id"))];

        assert_eq!(
            matching_target_board_key(&source, &targets),
            Some("Stories".to_string())
        );
    }

    fn board(name: Option<&str>, id: Option<&str>) -> BoardReference {
        let mut board = BoardReference::new();
        board.name = name.map(str::to_string);
        board.id = id.map(str::to_string);
        board
    }
}
