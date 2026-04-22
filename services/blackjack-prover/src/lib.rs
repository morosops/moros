use anyhow::Context;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use moros_common::{
    blackjack::{
        BlackjackExternalProverProofArtifact, BlackjackExternalProverResponse,
        BlackjackExternalZkProofPayload, BlackjackGroth16ProofArtifact,
        BlackjackZkPeekProofRequest, blackjack_hash_hex, build_blackjack_external_zk_proof_payload,
        validate_no_blackjack_private_witness, validate_no_blackjack_zk_proof_request,
    },
    config::ServiceConfig,
    infra::{InfraSnapshot, ServiceInfra},
    telemetry,
    web::base_router,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{fs, process::Command};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
pub struct AppState {
    pub infra: InfraSnapshot,
    pub proof_backend_mode: String,
    pub repo_root: PathBuf,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    service: &'static str,
    role: &'static str,
    status: &'static str,
    infra: InfraSnapshot,
    proof_backend_mode: String,
    proof_payload_schema_version: &'static str,
    proof_payload_encoding: &'static str,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn run() -> anyhow::Result<()> {
    telemetry::init("moros_blackjack_prover");
    let (config, state) = load_state_from_env().await?;
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.bind_address()).await?;
    tracing::info!(
        "{} listening on {}",
        config.service_name,
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn load_state_from_env() -> anyhow::Result<(ServiceConfig, Arc<AppState>)> {
    let config = ServiceConfig::from_env("moros-blackjack-prover", 8087);
    let infra = ServiceInfra::from_config(&config)?;
    let readiness = infra.prepare().await?;
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("failed to resolve Moros repo root")?;
    let proof_backend_mode = std::env::var("MOROS_BLACKJACK_PROVER_MODE")
        .unwrap_or_else(|_| "circom_groth16_bn254".to_string());
    let state = Arc::new(AppState {
        infra: infra.snapshot(&config, readiness),
        proof_backend_mode,
        repo_root,
    });
    Ok((config, state))
}

pub fn build_router(state: Arc<AppState>) -> Router {
    base_router::<Arc<AppState>>("moros-blackjack-prover")
        .route("/", get(root))
        .route(
            "/v1/blackjack/proofs/no-blackjack-peek",
            post(prove_no_blackjack_peek),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn root(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(RootResponse {
        service: "moros-blackjack-prover",
        role: "dealer peek Groth16 proof artifact issuer",
        status: "ready",
        infra: state.infra.clone(),
        proof_backend_mode: state.proof_backend_mode.clone(),
        proof_payload_schema_version: "moros_blackjack_external_proof_payload_v2",
        proof_payload_encoding: "circom_groth16_bn254_garaga_json_v1",
    }))
}

async fn prove_no_blackjack_peek(
    State(state): State<Arc<AppState>>,
    Json(request): Json<BlackjackZkPeekProofRequest>,
) -> ApiResult<BlackjackExternalProverResponse> {
    validate_no_blackjack_zk_proof_request(&request).map_err(ApiError::bad_request)?;
    validate_no_blackjack_private_witness(&request).map_err(ApiError::bad_request)?;

    let payload = prove_locally(&state.repo_root, &request).await?;
    let backend_request_id =
        compute_backend_request_id(&request.target.request_id, &payload.proof_bytes_hash);

    Ok(Json(BlackjackExternalProverResponse {
        proof: BlackjackExternalProverProofArtifact {
            status: "verified_groth16_binding".to_string(),
            request_id: request.target.request_id.clone(),
            claim: request.target.claim.clone(),
            statement_hash: request.target.statement_hash.clone(),
            public_inputs_hash: request.target.public_inputs_hash.clone(),
            proof_system: request.target.proof_system.clone(),
            circuit_family: request.target.circuit_family.clone(),
            circuit_id: request.target.circuit_id.clone(),
            verification_key_id: request.target.verification_key_id.clone(),
            backend_request_id: backend_request_id.clone(),
            proof_artifact_uri: format!("moros-blackjack-prover://proofs/{backend_request_id}"),
            proof_artifact: serde_json::json!({
                "artifact_kind": "moros_blackjack_external_proof_artifact_v1",
                "request_id": request.target.request_id,
                "claim": request.target.claim,
                "statement_hash": request.target.statement_hash,
                "public_inputs_hash": request.target.public_inputs_hash,
                "proof_system": request.target.proof_system,
                "circuit_family": request.target.circuit_family,
                "circuit_id": request.target.circuit_id,
                "verification_key_id": request.target.verification_key_id,
                "backend_request_id": backend_request_id,
                "proof": payload,
            }),
            ..BlackjackExternalProverProofArtifact::default()
        },
    }))
}

async fn prove_locally(
    repo_root: &Path,
    request: &BlackjackZkPeekProofRequest,
) -> Result<BlackjackExternalZkProofPayload, ApiError> {
    let temp_dir = temp_work_dir("moros-blackjack-prover");
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
            tracing::warn!(error = ?error, "failed to clean blackjack prover temp dir");
        }
        return Err(ApiError::internal(anyhow::anyhow!(
            "blackjack prover failed to generate proof: {}{}{}",
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
        tracing::warn!(error = ?error, "failed to clean blackjack prover temp dir");
    }

    build_blackjack_external_zk_proof_payload(&request.target, artifact).map_err(ApiError::internal)
}

pub fn compute_backend_request_id(request_id: &str, proof_bytes_hash: &str) -> String {
    blackjack_hash_hex(format!(
        "moros:blackjack:external-prover-backend-request:v1:{}:{}",
        request_id, proof_bytes_hash
    ))
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
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn internal(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
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
