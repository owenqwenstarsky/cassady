#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cassady::run().await
}
