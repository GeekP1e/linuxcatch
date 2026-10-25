mod app;
mod lessons;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run().await
}
