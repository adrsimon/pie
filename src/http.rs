use crate::errors::CommandError;
use crate::types::{PackageData, VersionData};
use bytes::Bytes;
use reqwest::Client;

pub struct HttpRequest;
impl HttpRequest {
    async fn registry(client: Client, route: String) -> Result<String, CommandError> {
        client
            .get(format!("{}/{}", crate::utils::REGISTRY_URL, route))
            .header(
                "Accept",
                "application/vnd.npm.install-v1+json; q=1.0, application/json; q=0.8, */*",
            )
            .send()
            .await
            .map_err(CommandError::HTTPFailed)?
            .text()
            .await
            .map_err(CommandError::FailedResponseText)
    }

    pub async fn get_bytes(client: Client, url: String) -> Result<Bytes, CommandError> {
        client
            .get(url)
            .send()
            .await
            .map_err(CommandError::HTTPFailed)?
            .bytes()
            .await
            .map_err(CommandError::FailedResponseBytes)
    }

    pub async fn version_data(
        client: Client,
        package_name: &String,
        version: &String,
    ) -> Result<VersionData, CommandError> {
        let response = Self::registry(client, format!("{package_name}/{version}")).await?;
        serde_json::from_str::<VersionData>(&response).map_err(CommandError::ParsingFailed)
    }

    pub async fn package_data(
        client: Client,
        package_name: &String,
    ) -> Result<PackageData, CommandError> {
        let response = Self::registry(client, format!("{package_name}")).await?;
        serde_json::from_str::<PackageData>(&response).map_err(CommandError::ParsingFailed)
    }
}
