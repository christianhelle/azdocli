#![allow(dead_code)]

use anyhow::{Context, Result};
use futures::TryStream;
use reqwest::{Body, Client, Response};
use serde::Deserialize;

use crate::auth::url::{normalize_base_url, percent_encode_path_segment};
use crate::auth::Credentials;

#[derive(Debug, Clone, Deserialize)]
pub struct UploadedAttachment {
    pub id: Option<String>,
    pub url: Option<String>,
}

pub fn client() -> Client {
    Client::new()
}

pub async fn download_attachment(
    client: &Client,
    creds: &Credentials,
    project: &str,
    attachment_id: &str,
) -> Result<Response> {
    let url = format!(
        "{}/{}/{}/_apis/wit/attachments/{}",
        normalize_base_url(&creds.base_url),
        percent_encode_path_segment(&creds.organization),
        percent_encode_path_segment(project),
        percent_encode_path_segment(attachment_id)
    );

    client
        .get(url)
        .query(&[("api-version", "7.1"), ("download", "true")])
        .basic_auth("", Some(&creds.pat))
        .send()
        .await
        .context("Downloading attachment")?
        .error_for_status()
        .context("Downloading attachment")
}

pub async fn upload_attachment_stream<S, E>(
    client: &Client,
    creds: &Credentials,
    project: &str,
    file_name: &str,
    stream: S,
) -> Result<UploadedAttachment>
where
    S: TryStream<Ok = Vec<u8>, Error = E> + Send + 'static,
    E: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
    let url = format!(
        "{}/{}/{}/_apis/wit/attachments",
        normalize_base_url(&creds.base_url),
        percent_encode_path_segment(&creds.organization),
        percent_encode_path_segment(project)
    );

    client
        .post(url)
        .query(&[("api-version", "7.1"), ("fileName", file_name)])
        .basic_auth("", Some(&creds.pat))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::wrap_stream(stream))
        .send()
        .await
        .context("Uploading attachment")?
        .error_for_status()
        .context("Uploading attachment")?
        .json::<UploadedAttachment>()
        .await
        .context("Parsing attachment upload response")
}
