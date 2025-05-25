use crate::auth::Credentials;
use anyhow::Result;
use reqwest::{header, Client};

// Creates a reqwest Client configured with Azure DevOps credentials
pub fn create_client(credentials: &Credentials) -> Result<Client> {
    let mut headers = header::HeaderMap::new();

    // Add Basic auth header with PAT
    let auth_value = format!("Basic {}", base64::encode(format!(":{}", credentials.pat)));
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&auth_value)?,
    );

    // Add common headers
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );

    let client = Client::builder().default_headers(headers).build()?;

    Ok(client)
}

// Constructs the base URL for Azure DevOps API calls
pub fn get_base_url(credentials: &Credentials) -> String {
    format!("https://dev.azure.com/{}", credentials.organization)
}
