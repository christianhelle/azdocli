use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::core::{
    models::{WebApiTeam, WebApiTeamRef},
    ClientBuilder as CoreClientBuilder,
};
use std::collections::HashMap;

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct TeamsCreatePhase;

#[async_trait]
impl Phase for TeamsCreatePhase {
    fn name(&self) -> &'static str {
        "teams_create"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let source_client = CoreClientBuilder::new(ctx.source_credential.clone()).build();
        let target_client = CoreClientBuilder::new(ctx.target_credential.clone()).build();

        let source_teams_client = source_client.teams_client();
        let target_teams_client = target_client.teams_client();

        let source_teams = list_project_teams(
            &source_teams_client,
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
            "source",
        )
        .await?;

        let target_teams = list_project_teams(
            &target_teams_client,
            &ctx.target_creds.organization,
            &ctx.opts.target_project,
            "target",
        )
        .await?;

        let mut target_team_ids_by_name = team_ids_by_name(&target_teams);
        let mut summary = PhaseSummary {
            items_total: source_teams.len() as u64,
            ..Default::default()
        };

        for source_team in source_teams {
            let (source_id, source_name) = match team_id_and_name(&source_team) {
                Ok(values) => values,
                Err(e) => {
                    summary.record_failure(e.to_string());
                    continue;
                }
            };

            if let Some(target_id) = target_team_ids_by_name.get(&source_name) {
                ctx.state
                    .id_map_mut("teams")
                    .map
                    .insert(source_id, target_id.clone());
                summary.record_success();
                continue;
            }

            if ctx.opts.dry_run {
                summary.record_success();
                continue;
            }

            let mut payload = WebApiTeam::new();
            payload.web_api_team_ref = WebApiTeamRef::new();
            payload.web_api_team_ref.name = Some(source_name.clone());
            payload.description = source_team.description.clone();

            match target_client
                .teams_client()
                .create(
                    &ctx.target_creds.organization,
                    payload,
                    &ctx.opts.target_project,
                )
                .await
            {
                Ok(created) => match created.web_api_team_ref.id {
                    Some(target_id) => {
                        target_team_ids_by_name.insert(source_name, target_id.clone());
                        ctx.state
                            .id_map_mut("teams")
                            .map
                            .insert(source_id, target_id);
                        summary.record_success();
                    }
                    None => summary.record_failure(format!(
                        "Creating team '{}': response did not include a team id",
                        source_name
                    )),
                },
                Err(e) => summary.record_failure(format!("Creating team '{}': {e}", source_name)),
            }
        }

        Ok(summary)
    }
}

async fn list_project_teams(
    client: &azure_devops_rust_api::core::teams::Client,
    organization: &str,
    project: &str,
    label: &str,
) -> Result<Vec<WebApiTeam>> {
    let page_size = 1000;
    let mut skip = 0;
    let mut teams = Vec::new();

    loop {
        let page = client
            .get_teams(organization, project)
            .top(page_size)
            .skip(skip)
            .await
            .map_err(|e| anyhow!("Listing {label} teams: {e}"))?
            .value;
        let page_len = page.len();
        teams.extend(page);

        if page_len < page_size as usize {
            break;
        }
        skip += page_size;
    }

    Ok(teams)
}

fn team_ids_by_name(teams: &[WebApiTeam]) -> HashMap<String, String> {
    teams
        .iter()
        .filter_map(|team| {
            let name = team.web_api_team_ref.name.as_ref()?;
            let id = team.web_api_team_ref.id.as_ref()?;
            Some((name.clone(), id.clone()))
        })
        .collect()
}

fn team_id_and_name(team: &WebApiTeam) -> Result<(String, String)> {
    let name = team
        .web_api_team_ref
        .name
        .clone()
        .ok_or_else(|| anyhow!("Source team is missing a name"))?;
    let id = team
        .web_api_team_ref
        .id
        .clone()
        .ok_or_else(|| anyhow!("Source team '{}' is missing an id", name))?;
    Ok((id, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_ids_by_name_uses_team_ids_not_names() {
        let teams = vec![team("source-id", "Shared Name", None)];

        let ids_by_name = team_ids_by_name(&teams);

        assert_eq!(
            ids_by_name.get("Shared Name"),
            Some(&"source-id".to_string())
        );
    }

    #[test]
    fn team_id_and_name_requires_source_team_id_for_id_map() {
        let mut team = WebApiTeam::new();
        team.web_api_team_ref.name = Some("Team without id".to_string());

        let err = team_id_and_name(&team).expect_err("missing id should fail");

        assert_eq!(
            err.to_string(),
            "Source team 'Team without id' is missing an id"
        );
    }

    fn team(id: &str, name: &str, description: Option<&str>) -> WebApiTeam {
        let mut team = WebApiTeam::new();
        team.web_api_team_ref.id = Some(id.to_string());
        team.web_api_team_ref.name = Some(name.to_string());
        team.description = description.map(str::to_string);
        team
    }
}
