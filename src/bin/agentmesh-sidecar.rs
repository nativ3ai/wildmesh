#[tokio::main]
async fn main() -> anyhow::Result<()> {
    agentmesh::cli::main_sidecar_entry().await
}
