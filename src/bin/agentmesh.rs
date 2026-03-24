#[tokio::main]
async fn main() -> anyhow::Result<()> {
    agentmesh::cli::main_entry().await
}
