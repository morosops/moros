use tracing_subscriber::{EnvFilter, fmt};

pub fn init(service_name: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{service_name}=info,tower_http=info")));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
