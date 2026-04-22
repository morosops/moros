use anyhow::{Context, bail};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use moros_common::{
    blackjack::{
        BlackjackExternalProverResponse, BlackjackExternalZkProofPayload,
        BlackjackGroth16ProofArtifact, BlackjackZkPeekProofRequest, BlackjackZkPeekProofResponse,
        build_blackjack_external_zk_proof_payload, validate_no_blackjack_external_prover_artifact,
        validate_no_blackjack_private_witness, validate_no_blackjack_zk_proof_request,
    },
    config::ServiceConfig,
    infra::{InfraSnapshot, ServiceInfra},
    telemetry,
    web::base_router,
};
use reqwest::Client;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{fs, process::Command};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
enum ProverBackend {
    LocalBuiltCircuit,
    ExternalHttp {
        endpoint: String,
        bearer_token: Option<String>,
    },
}

impl ProverBackend {
    fn mode(&self) -> &'static str {
        match self {
            Self::LocalBuiltCircuit => "local_built_circuit",
            Self::ExternalHttp { .. } => "external_http",
        }
    }

    fn external_url(&self) -> Option<&str> {
        match self {
            Self::LocalBuiltCircuit => None,
            Self::ExternalHttp { endpoint, .. } => Some(endpoint.as_str()),
        }
    }
}

#[derive(Clone)]
struct AppState {
    infra: InfraSnapshot,
    http_client: Client,
    prover_backend: ProverBackend,
    repo_root: PathBuf,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    service: &'static str,
    role: &'static str,
    status: &'static str,
    infra: InfraSnapshot,
    prover_backend_mode: String,
    prover_external_url: Option<String>,
    external_proof_payload_schema_version: &'static str,
    external_proof_payload_encoding: &'static str,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

type ApiResult<T> = Result<Json<T>, ApiError>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init("moros_blackjack_verifier");
    let config = ServiceConfig::from_env("moros-blackjack-verifier", 8086);
    let infra = ServiceInfra::from_config(&config)?;
    let readiness = infra.prepare().await?;
    let prover_timeout_ms = std::env::var("MOROS_BLACKJACK_PROVER_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30_000);
    let http_client = Client::builder()
        .timeout(Duration::from_millis(prover_timeout_ms))
        .build()
        .context("failed to build blackjack verifier HTTP client")?;
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("failed to resolve Moros repo root")?;
    let state = Arc::new(AppState {
        infra: infra.snapshot(&config, readiness),
        http_client,
        prover_backend: prover_backend_from_env()?,
        repo_root,
    });

    let app = base_router::<Arc<AppState>>("moros-blackjack-verifier")
        .route("/", get(root))
        .route("/v1/blackjack/dealer-peek/prove", post(issue_peek_proof))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.bind_address()).await?;
    tracing::info!(
        "{} listening on {}",
        config.service_name,
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn prover_backend_from_env() -> anyhow::Result<ProverBackend> {
    match std::env::var("MOROS_BLACKJACK_PROVER_BACKEND")
        .unwrap_or_else(|_| "local_built_circuit".to_string())
        .trim()
    {
        "local" | "local_built_circuit" | "local_fixture" => Ok(ProverBackend::LocalBuiltCircuit),
        "external_http" => {
            let endpoint = std::env::var("MOROS_BLACKJACK_EXTERNAL_PROVER_URL").context(
                "MOROS_BLACKJACK_EXTERNAL_PROVER_URL is required for external_http prover backend",
            )?;
            Ok(ProverBackend::ExternalHttp {
                endpoint,
                bearer_token: std::env::var("MOROS_BLACKJACK_EXTERNAL_PROVER_BEARER_TOKEN").ok(),
            })
        }
        other => bail!("unsupported MOROS_BLACKJACK_PROVER_BACKEND: {other}"),
    }
}

async fn root(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(RootResponse {
        service: "moros-blackjack-verifier",
        role: "dealer peek Groth16 payload generator and validator",
        status: "ready",
        infra: state.infra.clone(),
        prover_backend_mode: state.prover_backend.mode().to_string(),
        prover_external_url: state.prover_backend.external_url().map(str::to_string),
        external_proof_payload_schema_version: "moros_blackjack_external_proof_payload_v2",
        external_proof_payload_encoding: "circom_groth16_bn254_garaga_json_v1",
    }))
}

async fn issue_peek_proof(
    State(state): State<Arc<AppState>>,
    Json(request): Json<BlackjackZkPeekProofRequest>,
) -> ApiResult<BlackjackZkPeekProofResponse> {
    validate_no_blackjack_zk_proof_request(&request).map_err(ApiError::bad_request)?;
    validate_no_blackjack_private_witness(&request).map_err(ApiError::bad_request)?;

    let proof = match &state.prover_backend {
        ProverBackend::LocalBuiltCircuit => prove_locally(&state.repo_root, &request).await?,
        ProverBackend::ExternalHttp {
            endpoint,
            bearer_token,
        } => {
            fetch_external_proof(
                &state.http_client,
                endpoint,
                bearer_token.as_deref(),
                &request,
            )
            .await?
        }
    };

    Ok(Json(BlackjackZkPeekProofResponse { proof }))
}

async fn prove_locally(
    repo_root: &Path,
    request: &BlackjackZkPeekProofRequest,
) -> Result<BlackjackExternalZkProofPayload, ApiError> {
    let temp_dir = temp_work_dir("moros-blackjack-verifier");
    let request_path = temp_dir.join("request.json");
    let output_path = temp_dir.join("proof_payload.json");
    fs::create_dir_all(&temp_dir)
        .await
        .map_err(ApiError::internal)?;
    fs::write(
        &request_path,
        serde_json::to_vec_pretty(request).map_err(ApiError::internal)?,
    )
    .await
    .map_err(ApiError::internal)?;

    let output = Command::new("node")
        .arg(repo_root.join("scripts/prove_blackjack_peek.mjs"))
        .arg(&request_path)
        .arg(&output_path)
        .current_dir(repo_root)
        .output()
        .await
        .map_err(ApiError::internal)?;

    if !output.status.success() {
        let cleanup_result = fs::remove_dir_all(&temp_dir).await;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Err(error) = cleanup_result {
            tracing::warn!(error = ?error, "failed to clean blackjack verifier temp dir");
        }
        return Err(ApiError::internal(anyhow::anyhow!(
            "local blackjack proof generation failed: {}{}{}",
            stderr.trim(),
            if stderr.trim().is_empty() || stdout.trim().is_empty() {
                ""
            } else {
                " | "
            },
            stdout.trim()
        )));
    }

    let artifact: BlackjackGroth16ProofArtifact =
        serde_json::from_slice(&fs::read(&output_path).await.map_err(ApiError::internal)?)
            .map_err(ApiError::internal)?;

    if let Err(error) = fs::remove_dir_all(&temp_dir).await {
        tracing::warn!(error = ?error, "failed to clean blackjack verifier temp dir");
    }

    build_blackjack_external_zk_proof_payload(&request.target, artifact).map_err(ApiError::internal)
}

async fn fetch_external_proof(
    http_client: &Client,
    endpoint: &str,
    bearer_token: Option<&str>,
    request: &BlackjackZkPeekProofRequest,
) -> Result<BlackjackExternalZkProofPayload, ApiError> {
    let mut outbound = http_client.post(endpoint).json(request);
    if let Some(token) = bearer_token {
        outbound = outbound.bearer_auth(token);
    }

    let response = outbound
        .send()
        .await
        .with_context(|| format!("failed to reach external blackjack prover at {endpoint}"))
        .map_err(ApiError::bad_gateway)?
        .error_for_status()
        .context("external blackjack prover rejected dealer peek proof request")
        .map_err(ApiError::bad_gateway)?;
    let payload = response
        .json::<BlackjackExternalProverResponse>()
        .await
        .context("failed to decode external blackjack prover response")
        .map_err(ApiError::bad_gateway)?;

    validate_no_blackjack_external_prover_artifact(&request.target, &payload.proof)
        .map_err(ApiError::bad_gateway)?;

    serde_json::from_value::<BlackjackExternalZkProofPayload>(
        payload
            .proof
            .proof_artifact
            .get("proof")
            .cloned()
            .ok_or_else(|| {
                ApiError::bad_gateway(anyhow::anyhow!(
                    "external blackjack prover response is missing typed proof payload"
                ))
            })?,
    )
    .context("external blackjack prover returned an invalid typed proof payload")
    .map_err(ApiError::bad_gateway)
}

fn temp_work_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()))
}

impl ApiError {
    fn bad_request(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        tracing::error!(error = ?error, "blackjack verifier bad request");
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn bad_gateway(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        tracing::error!(error = ?error, "blackjack verifier bad gateway");
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: error.to_string(),
        }
    }

    fn internal(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        tracing::error!(error = ?error, "blackjack verifier internal error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}
