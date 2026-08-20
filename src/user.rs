use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::url::{is_default_cloud_host, user_entitlements_url};
use crate::auth::{get_credentials, Credentials};
use anyhow::{anyhow, Result};
use azure_devops_rust_api::member_entitlement_management::{self, models};
use clap::{Subcommand, ValueEnum};
use colored::Colorize;
use reqwest::header::CONTENT_TYPE;
use serde_json::json;

#[derive(Subcommand, Clone)]
pub enum UserSubCommands {
    /// Add user.
    Add {
        /// Email (principal name) of the user to add
        #[clap(long)]
        email: String,
        /// License type for the user
        #[clap(long, value_enum)]
        license: UserLicenseType,
    },
    /// List users in an organization [except for users which are added via AAD groups].
    List,
    /// Remove user from an organization.
    Remove {
        /// User ID
        #[clap(long, conflicts_with = "email", required_unless_present = "email")]
        id: Option<String>,
        /// User email (principal name)
        #[clap(long, conflicts_with = "id", required_unless_present = "id")]
        email: Option<String>,
    },
    /// Show user details.
    Show {
        /// User ID
        #[clap(long, conflicts_with = "email", required_unless_present = "email")]
        id: Option<String>,
        /// User email (principal name)
        #[clap(long, conflicts_with = "id", required_unless_present = "id")]
        email: Option<String>,
    },
    /// Update license type for a user.
    Update {
        /// User ID
        #[clap(long, conflicts_with = "email", required_unless_present = "email")]
        id: Option<String>,
        /// User email (principal name)
        #[clap(long, conflicts_with = "id", required_unless_present = "id")]
        email: Option<String>,
        /// New license type
        #[clap(long, value_enum)]
        license: UserLicenseType,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum UserLicenseType {
    #[value(name = "none")]
    None,
    #[value(name = "earlyAdopter")]
    EarlyAdopter,
    #[value(name = "express")]
    Express,
    #[value(name = "professional")]
    Professional,
    #[value(name = "advanced")]
    Advanced,
    #[value(name = "stakeholder")]
    Stakeholder,
}

impl UserLicenseType {
    fn as_api_value(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::EarlyAdopter => "earlyAdopter",
            Self::Express => "express",
            Self::Professional => "professional",
            Self::Advanced => "advanced",
            Self::Stakeholder => "stakeholder",
        }
    }

    fn as_account_license_type(self) -> models::access_level::AccountLicenseType {
        match self {
            Self::None => models::access_level::AccountLicenseType::None,
            Self::EarlyAdopter => models::access_level::AccountLicenseType::EarlyAdopter,
            Self::Express => models::access_level::AccountLicenseType::Express,
            Self::Professional => models::access_level::AccountLicenseType::Professional,
            Self::Advanced => models::access_level::AccountLicenseType::Advanced,
            Self::Stakeholder => models::access_level::AccountLicenseType::Stakeholder,
        }
    }
}

impl std::fmt::Display for UserLicenseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_api_value())
    }
}

pub async fn handle_command(subcommand: &UserSubCommands) -> Result<()> {
    let creds = get_credentials()?;

    if !is_default_cloud_host(&creds.base_url) {
        return Err(anyhow!(
            "User management is only supported on the default Azure DevOps cloud host ({}).",
            crate::auth::url::default_base_url()
        ));
    }

    let client = create_client(&creds);

    match subcommand {
        UserSubCommands::Add { email, license } => {
            let created = add_user(&client, &creds.organization, email, *license).await?;
            println!("{}", "✅ User added successfully!".green());
            println!("ID: {}", user_id(&created).unwrap_or("-"));
            println!("Email: {}", user_email(&created).unwrap_or("-"));
            println!(
                "Display name: {}",
                user_display_name(&created).unwrap_or("-")
            );
            println!("License: {}", user_license(&created));
        }
        UserSubCommands::List => {
            let users = search_user_entitlements(&client, &creds.organization, None).await?;
            display_users(&users);
        }
        UserSubCommands::Remove { id, email } => {
            let resolved_id = resolve_user_id(&client, &creds.organization, id, email).await?;
            client
                .user_entitlements_client()
                .delete(creds.organization.clone(), resolved_id.clone())
                .await?;

            println!("{}", "✅ User removed successfully!".green());
            println!("Removed user ID: {resolved_id}");
        }
        UserSubCommands::Show { id, email } => {
            let resolved_id = resolve_user_id(&client, &creds.organization, id, email).await?;
            let user = client
                .user_entitlements_client()
                .get(creds.organization.clone(), resolved_id)
                .await?;
            display_user_details(&user);
        }
        UserSubCommands::Update { id, email, license } => {
            let resolved_id = resolve_user_id(&client, &creds.organization, id, email).await?;
            update_user_license(
                &creds.base_url,
                &creds.organization,
                &creds.pat,
                &resolved_id,
                *license,
            )
            .await?;
            let user = client
                .user_entitlements_client()
                .get(creds.organization.clone(), resolved_id)
                .await?;

            println!("{}", "✅ User license updated successfully!".green());
            println!("Email: {}", user_email(&user).unwrap_or("-"));
            println!("License: {}", user_license(&user));
        }
    }

    Ok(())
}

fn create_client(creds: &Credentials) -> member_entitlement_management::Client {
    let factory = CredentialClientFactory::new(creds);
    factory.build_entitlements()
}

async fn add_user(
    client: &member_entitlement_management::Client,
    organization: &str,
    email: &str,
    license: UserLicenseType,
) -> Result<models::UserEntitlement> {
    let mut access_level = models::AccessLevel::new();
    access_level.account_license_type = Some(license.as_account_license_type());
    access_level.licensing_source = Some(models::access_level::LicensingSource::Account);
    access_level.msdn_license_type = Some(models::access_level::MsdnLicenseType::None);
    access_level.license_display_name = Some(license.to_string());

    let mut user = models::GraphUser::new();
    user.aad_graph_member.graph_member.mail_address = Some(email.to_string());
    user.aad_graph_member.graph_member.principal_name = Some(email.to_string());
    user.aad_graph_member
        .graph_member
        .graph_subject
        .subject_kind = Some("user".to_string());

    let mut entitlement = models::UserEntitlement::new();
    entitlement.entitlement_base.access_level = Some(access_level);
    entitlement.user = Some(user);

    let response = client
        .user_entitlements_client()
        .add(organization.to_string(), entitlement)
        .await?;

    response
        .user_entitlements_response_base
        .user_entitlement
        .ok_or_else(|| {
            anyhow!("User was created, but response did not include entitlement details")
        })
}

async fn resolve_user_id(
    client: &member_entitlement_management::Client,
    organization: &str,
    id: &Option<String>,
    email: &Option<String>,
) -> Result<String> {
    match (id.as_deref(), email.as_deref()) {
        (Some(value), None) => Ok(value.to_string()),
        (None, Some(value)) => {
            let user = find_user_by_email(client, organization, value).await?;
            user_id(&user)
                .map(str::to_string)
                .ok_or_else(|| anyhow!("Matched user is missing an entitlement ID"))
        }
        (Some(_), Some(_)) => Err(anyhow!("Use either --id or --email, not both")),
        (None, None) => Err(anyhow!("Either --id or --email must be provided")),
    }
}

async fn find_user_by_email(
    client: &member_entitlement_management::Client,
    organization: &str,
    email: &str,
) -> Result<models::UserEntitlement> {
    let escaped_email = email.replace('\'', "''");
    let filter = format!("name eq '{escaped_email}'");
    let users = search_user_entitlements(client, organization, Some(filter)).await?;
    let expected = normalize(email);

    let mut matches = users
        .into_iter()
        .filter(|user| {
            user_email(user)
                .map(normalize)
                .map(|current| current == expected)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if matches.is_empty() {
        return Err(anyhow!("No user found for email '{email}'"));
    }

    if matches.len() > 1 {
        let candidate_ids = matches
            .iter()
            .map(|user| user_id(user).unwrap_or("-"))
            .collect::<Vec<_>>();
        return Err(anyhow!(
            "Multiple users matched email '{email}'. Use --id instead. Candidates: {}",
            candidate_ids.join(", ")
        ));
    }

    Ok(matches.remove(0))
}

async fn search_user_entitlements(
    client: &member_entitlement_management::Client,
    organization: &str,
    filter: Option<String>,
) -> Result<Vec<models::UserEntitlement>> {
    let mut users = Vec::new();
    let mut continuation_token = None::<String>;

    loop {
        let mut request = client
            .user_entitlements_client()
            .search_user_entitlements(organization.to_string());

        if let Some(token) = continuation_token.clone() {
            request = request.continuation_token(token);
        }

        if let Some(ref filter_value) = filter {
            request = request.filter(filter_value.clone());
        }

        let response = request.await?;
        continuation_token = response.continuation_token.clone();
        users.extend(response.items);

        if continuation_token.is_none() {
            break;
        }
    }

    Ok(users)
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn is_aad_group_added_user(user: &models::UserEntitlement) -> bool {
    matches!(
        user.entitlement_base
            .access_level
            .as_ref()
            .and_then(|access| access.assignment_source.as_ref()),
        Some(models::access_level::AssignmentSource::GroupRule)
    )
}

fn user_id(user: &models::UserEntitlement) -> Option<&str> {
    user.entitlement_base.id.as_deref()
}

fn user_email(user: &models::UserEntitlement) -> Option<&str> {
    user.user
        .as_ref()
        .and_then(|graph_user| {
            graph_user
                .aad_graph_member
                .graph_member
                .principal_name
                .as_deref()
        })
        .or_else(|| {
            user.user.as_ref().and_then(|graph_user| {
                graph_user
                    .aad_graph_member
                    .graph_member
                    .mail_address
                    .as_deref()
            })
        })
}

fn user_display_name(user: &models::UserEntitlement) -> Option<&str> {
    user.user.as_ref().and_then(|graph_user| {
        graph_user
            .aad_graph_member
            .graph_member
            .graph_subject
            .graph_subject_base
            .display_name
            .as_deref()
    })
}

fn user_license(user: &models::UserEntitlement) -> String {
    user.entitlement_base
        .access_level
        .as_ref()
        .and_then(|access| access.account_license_type.as_ref())
        .map(account_license_type_name)
        .or_else(|| {
            user.entitlement_base
                .access_level
                .as_ref()
                .and_then(|access| access.license_display_name.clone())
        })
        .unwrap_or_else(|| "-".to_string())
}

fn account_license_type_name(value: &models::access_level::AccountLicenseType) -> String {
    match value {
        models::access_level::AccountLicenseType::None => "none".to_string(),
        models::access_level::AccountLicenseType::EarlyAdopter => "earlyAdopter".to_string(),
        models::access_level::AccountLicenseType::Express => "express".to_string(),
        models::access_level::AccountLicenseType::Professional => "professional".to_string(),
        models::access_level::AccountLicenseType::Advanced => "advanced".to_string(),
        models::access_level::AccountLicenseType::Stakeholder => "stakeholder".to_string(),
    }
}

fn display_users(users: &[models::UserEntitlement]) {
    let filtered = users
        .iter()
        .filter(|user| !is_aad_group_added_user(user))
        .collect::<Vec<_>>();

    if filtered.is_empty() {
        println!("No users found.");
        return;
    }

    println!(
        "{:<38} {:<35} {:<30} {:<14}",
        "ID".bold(),
        "Email".bold(),
        "Display Name".bold(),
        "License".bold()
    );
    println!("{}", "-".repeat(122));

    for user in filtered {
        println!(
            "{:<38} {:<35} {:<30} {:<14}",
            user_id(user).unwrap_or("-"),
            user_email(user).unwrap_or("-"),
            user_display_name(user).unwrap_or("-"),
            user_license(user),
        );
    }
}

fn display_user_details(user: &models::UserEntitlement) {
    println!("{}", "User Details".bold().underline());
    println!("ID: {}", user_id(user).unwrap_or("-"));
    println!("Email: {}", user_email(user).unwrap_or("-"));
    println!("Display name: {}", user_display_name(user).unwrap_or("-"));
    println!("License: {}", user_license(user));

    if let Some(access_level) = user.entitlement_base.access_level.as_ref() {
        if let Some(source) = access_level.assignment_source.as_ref() {
            println!("Assignment source: {source:?}");
        }
        if let Some(status) = access_level.status.as_ref() {
            println!("Status: {status:?}");
        }
    }

    if let Some(date_created) = user.entitlement_base.date_created.as_ref() {
        println!("Created: {date_created}");
    }

    if let Some(last_accessed) = user.entitlement_base.last_accessed_date.as_ref() {
        println!("Last accessed: {last_accessed}");
    }
}

async fn update_user_license(
    base_url: &str,
    organization: &str,
    pat: &str,
    user_id: &str,
    license: UserLicenseType,
) -> Result<()> {
    let payload = json!([
        {
            "op": "replace",
            "path": "/accessLevel",
            "value": {
                "accountLicenseType": license.as_api_value(),
                "licensingSource": "account",
                "msdnLicenseType": "none",
                "licenseDisplayName": license.as_api_value()
            }
        }
    ]);

    let url = user_entitlements_url(base_url, organization, user_id);

    let response = reqwest::Client::new()
        .patch(url)
        .basic_auth("", Some(pat))
        .header(CONTENT_TYPE, "application/json-patch+json")
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(anyhow!(
        "Failed to update license for user '{user_id}' ({status}): {body}"
    ))
}
