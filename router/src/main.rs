#[tokio::main]
async fn main() -> multifrost_router::error::Result<()> {
    multifrost_router::run().await
}
