#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moros_blackjack_prover::run().await
}
