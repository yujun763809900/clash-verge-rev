//! Client-scoped TunnelMesh Agent discovery for the settings picker.

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const AGENTS_PATH: &str = "/api/v1/client/agents";
const MAX_PAGES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TunnelMeshAgent {
    pub id: String,
    pub name: String,
    pub online: bool,
}

#[derive(Debug, Deserialize)]
struct AgentPage {
    #[serde(default)]
    data: AgentPageData,
}

#[derive(Debug, Default, Deserialize)]
struct AgentPageData {
    #[serde(default)]
    items: Vec<TunnelMeshAgent>,
    #[serde(rename = "nextCursor", default)]
    next_cursor: String,
    #[serde(rename = "hasMore", default)]
    has_more: bool,
}

pub async fn list_agents(server_url: &str, token: &str) -> Result<Vec<TunnelMeshAgent>> {
    let endpoint = agents_endpoint(server_url)?;
    let token = token.trim();
    if token.is_empty() {
        bail!("TunnelMesh client token is required to list agents");
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to create TunnelMesh agent client")?;
    let mut cursor = None;
    let mut agents = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_cursors = std::collections::HashSet::new();

    for _ in 0..MAX_PAGES {
        let mut request = client.get(&endpoint).query(&[("limit", "500")]).bearer_auth(token);
        if let Some(cursor) = cursor.as_deref() {
            request = request.query(&[("cursor", cursor)]);
        }

        let response = request
            .send()
            .await
            .context("failed to contact TunnelMesh agent endpoint")?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .context("failed to read TunnelMesh agent response")?;
        if !status.is_success() {
            let detail = String::from_utf8_lossy(&body);
            bail!("TunnelMesh agent endpoint returned {status}: {detail}");
        }

        let page: AgentPage = serde_json::from_slice(&body).context("invalid TunnelMesh agent response")?;
        for agent in page.data.items {
            let id = agent.id.trim();
            if id.is_empty() || !seen_ids.insert(id.to_owned()) {
                continue;
            }
            agents.push(TunnelMeshAgent {
                id: id.to_owned(),
                name: agent.name,
                online: agent.online,
            });
        }

        if !page.data.has_more || page.data.next_cursor.is_empty() {
            return Ok(agents);
        }
        if !seen_cursors.insert(page.data.next_cursor.clone()) {
            return Ok(agents);
        }
        cursor = Some(page.data.next_cursor);
    }

    Ok(agents)
}

fn agents_endpoint(server_url: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(server_url.trim()).context("invalid TunnelMesh server URL")?;
    let scheme = match url.scheme() {
        "ws" => "http",
        "wss" => "https",
        other => bail!("TunnelMesh server URL must use ws or wss, got {other:?}"),
    };
    url.set_scheme(scheme)
        .map_err(|_| anyhow::anyhow!("failed to set TunnelMesh HTTP scheme"))?;
    url.set_path(AGENTS_PATH);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::agents_endpoint;

    #[test]
    fn converts_websocket_server_url_to_client_agents_endpoint() {
        let endpoint = agents_endpoint("wss://tunnelmesh.example/ws/client");
        assert_eq!(
            endpoint.ok().as_deref(),
            Some("https://tunnelmesh.example/api/v1/client/agents")
        );
    }

    #[test]
    fn rejects_non_websocket_server_url() {
        assert!(agents_endpoint("https://tunnelmesh.example/ws/client").is_err());
    }
}
