use anyhow::{Context, bail, ensure};
use num_bigint::BigUint;
use poseidon_bn128::poseidon as poseidon_bn128;
use scalarff::{Bn128FieldElement, FieldElement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet::core::types::Felt;
use starknet_crypto::poseidon_hash_many;
use std::collections::BTreeMap;

const SHOE_DECKS: usize = 6;
const DEALER_STAND_TOTAL: u8 = 17;
const MAX_HANDS_PER_ROUND: usize = 4;
const BLACKJACK_RULESET_SPEC: &str = "vegas_strip_v2:s17:bj3:2:insurance2:1:double_any_two:dasa:max_splits=3:max_hands=4:split_aces_one_card:no_resplit_aces:late_surrender:6_decks";
const BLACKJACK_PROTOCOL_MODE_CURRENT: &str = "poseidon_fisher_yates_merkle_commitment_v1";
const BLACKJACK_PROTOCOL_MODE_TARGET: &str = "poseidon_fisher_yates_merkle_groth16_peek_v3";
const BLACKJACK_COMMITMENT_SCHEME: &str = "stark_poseidon_merkle_card_commitment_v1";
const BLACKJACK_ENCRYPTION_SCHEME_CURRENT: &str = "seed_derived_encrypted_envelope_v1";
const BLACKJACK_ENCRYPTION_SCHEME_TARGET: &str = "poseidon_committed_hidden_card_groth16_v3";
const BLACKJACK_PEEK_PROOF_KIND_TARGET: &str = "zk_no_blackjack_peek_groth16_v3";
const BLACKJACK_PEEK_PROOF_MODE_TARGET: &str = "circom_groth16_bn254_garaga_v1";
const BLACKJACK_REVEAL_PROOF_KIND_ENVELOPE: &str = "seed_envelope_opening_v1";
const BLACKJACK_PEEK_VERIFIER_NAMESPACE: &str = "moros.blackjack.peek.no_blackjack.v1";
const BLACKJACK_PEEK_VERIFIER_KIND_TARGET: &str = "moros_groth16_peek_verifier_v3";
const BLACKJACK_PEEK_PROOF_SYSTEM_TARGET: &str = "groth16_bn254";
const BLACKJACK_PEEK_CIRCUIT_FAMILY_TARGET: &str = "dealer_peek_no_blackjack";
const BLACKJACK_PEEK_CIRCUIT_ID_TARGET: &str = "moros_blackjack_peek_no_blackjack_groth16_v3";
const BLACKJACK_PEEK_VERIFICATION_KEY_ID_TARGET: &str =
    "moros_blackjack_peek_no_blackjack_bn254_vk_v3";
const BLACKJACK_PEEK_PROOF_BINDING_STATUS_VERIFIED: &str = "verified_groth16_binding";
const BLACKJACK_EXTERNAL_PROOF_ARTIFACT_KIND: &str = "moros_blackjack_external_proof_artifact_v1";
const BLACKJACK_EXTERNAL_PROOF_PAYLOAD_SCHEMA_VERSION: &str =
    "moros_blackjack_external_proof_payload_v2";
const BLACKJACK_EXTERNAL_PROOF_PAYLOAD_ENCODING: &str = "circom_groth16_bn254_garaga_json_v1";
const BLACKJACK_EXTERNAL_PROOF_SCHEME_GROTH16: &str = "groth16";
const BLACKJACK_EXTERNAL_PROOF_CURVE_BN254: &str = "bn254";
const BLACKJACK_EXTERNAL_PROOF_BACKEND_CIRCOM: &str = "circom_groth16";
const BLACKJACK_EXTERNAL_PROOF_BACKEND_FIXTURE: &str = "fixture_groth16";
const BLACKJACK_ONCHAIN_CARD_SALT_DOMAIN_HEX: &str = "0x4d4f524f535f424a5f53414c545f5631";
const BLACKJACK_ONCHAIN_CARD_TREE_DEPTH: usize = 9;
const BLACKJACK_ONCHAIN_CARD_TREE_SIZE: usize = 312;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackProofReceipt {
    #[serde(default)]
    pub proof_kind: String,
    #[serde(default)]
    pub receipt: String,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackCommittedCard {
    pub deck_index: usize,
    #[serde(default)]
    pub card_id: u16,
    pub rank: u8,
    #[serde(default)]
    pub commitment: String,
    #[serde(default)]
    pub onchain_commitment: String,
    #[serde(default)]
    pub onchain_salt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackMerkleOpening {
    #[serde(default)]
    pub leaf_hash: String,
    #[serde(default)]
    pub leaf_index: usize,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub siblings: Vec<String>,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackRevealRecord {
    pub deck_index: usize,
    #[serde(default)]
    pub card_id: u16,
    pub rank: u8,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub receipt: BlackjackProofReceipt,
    #[serde(default)]
    pub opening: BlackjackMerkleOpening,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackDealerPeekState {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub checked: bool,
    pub upcard_rank: Option<u8>,
    pub hole_card_index: Option<usize>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub proof_mode: String,
    #[serde(default)]
    pub target_proof_mode: String,
    #[serde(default)]
    pub target_proof_kind: String,
    #[serde(default)]
    pub statement_kind: String,
    #[serde(default)]
    pub public_inputs_hash: String,
    #[serde(default)]
    pub hidden_value_class_commitment: String,
    #[serde(default)]
    pub witness_commitment: String,
    #[serde(default)]
    pub hole_card_rank_commitment: String,
    #[serde(default)]
    pub no_blackjack_proof: BlackjackNoBlackjackProofArtifact,
    #[serde(default)]
    pub receipt: BlackjackProofReceipt,
    #[serde(default)]
    pub opening: BlackjackMerkleOpening,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackNoBlackjackProofArtifact {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub verifier_status: String,
    #[serde(default)]
    pub verifier_namespace: String,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub statement_hash: String,
    #[serde(default)]
    pub statement: BlackjackNoBlackjackProofStatement,
    #[serde(default)]
    pub current_proof_mode: String,
    #[serde(default)]
    pub target_proof_mode: String,
    #[serde(default)]
    pub current_proof_kind: String,
    #[serde(default)]
    pub target_proof_kind: String,
    #[serde(default)]
    pub statement_kind: String,
    #[serde(default)]
    pub public_inputs_hash: String,
    #[serde(default)]
    pub hidden_value_class_commitment: String,
    #[serde(default)]
    pub witness_commitment: String,
    #[serde(default)]
    pub hole_card_rank_commitment: String,
    #[serde(default)]
    pub receipt: BlackjackProofReceipt,
    #[serde(default)]
    pub opening: BlackjackMerkleOpening,
    #[serde(default)]
    pub zk_proof_target: BlackjackZkProofTargetArtifact,
    #[serde(default)]
    pub proof_binding: BlackjackProofBindingArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackNoBlackjackProofStatement {
    #[serde(default)]
    pub hand_id: String,
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub table_id: u64,
    #[serde(default)]
    pub ruleset_hash: String,
    #[serde(default)]
    pub deck_commitment_root: String,
    #[serde(default)]
    pub encrypted_deck_root: String,
    pub dealer_upcard_rank: Option<u8>,
    pub hole_card_index: Option<usize>,
    #[serde(default)]
    pub statement_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackZkProofTargetArtifact {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub verifier_namespace: String,
    #[serde(default)]
    pub verifier_kind: String,
    #[serde(default)]
    pub proof_system: String,
    #[serde(default)]
    pub circuit_family: String,
    #[serde(default)]
    pub circuit_id: String,
    #[serde(default)]
    pub verification_key_id: String,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub statement_hash: String,
    #[serde(default)]
    pub public_inputs_hash: String,
    #[serde(default)]
    pub encrypted_deck_root: String,
    pub dealer_upcard_rank: Option<u8>,
    pub hole_card_index: Option<usize>,
    #[serde(default)]
    pub hidden_value_class_commitment: String,
    #[serde(default)]
    pub witness_commitment: String,
    #[serde(default)]
    pub hole_card_rank_commitment: String,
    #[serde(default)]
    pub artifact_hash: String,
    #[serde(default)]
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackProofBindingArtifact {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub request_bound: bool,
    #[serde(default)]
    pub proof_verified: bool,
    #[serde(default)]
    pub verifier_namespace: String,
    #[serde(default)]
    pub verifier_kind: String,
    #[serde(default)]
    pub proof_system: String,
    #[serde(default)]
    pub circuit_family: String,
    #[serde(default)]
    pub circuit_id: String,
    #[serde(default)]
    pub verification_key_id: String,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub statement_hash: String,
    #[serde(default)]
    pub public_inputs_hash: String,
    #[serde(default)]
    pub target_artifact_hash: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub proof_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackZkPeekProofRequest {
    #[serde(default)]
    pub hand_id: String,
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub table_id: u64,
    #[serde(default)]
    pub transcript_root: String,
    #[serde(default)]
    pub target: BlackjackZkProofTargetArtifact,
    #[serde(default)]
    pub private_witness: Option<BlackjackDealerPeekPrivateWitness>,
    #[serde(default)]
    pub onchain_context: Option<BlackjackOnchainPeekContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackZkPeekProofResponse {
    #[serde(default)]
    pub proof: BlackjackExternalZkProofPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackOnchainPeekContext {
    #[serde(default)]
    pub chain_hand_id: u64,
    #[serde(default)]
    pub table_id: u64,
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub wager: String,
    #[serde(default)]
    pub transcript_root: String,
    #[serde(default)]
    pub dealer_upcard: u8,
    #[serde(default)]
    pub player_first_card: u8,
    #[serde(default)]
    pub player_second_card: u8,
    #[serde(default)]
    pub dealer_blackjack: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackOnchainCardRevealProof {
    #[serde(default)]
    pub deck_index: u64,
    #[serde(default)]
    pub card_id: u16,
    #[serde(default)]
    pub salt: String,
    #[serde(default)]
    pub siblings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackExternalProverProofArtifact {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub claim: String,
    #[serde(default)]
    pub statement_hash: String,
    #[serde(default)]
    pub public_inputs_hash: String,
    #[serde(default)]
    pub proof_system: String,
    #[serde(default)]
    pub circuit_family: String,
    #[serde(default)]
    pub circuit_id: String,
    #[serde(default)]
    pub verification_key_id: String,
    #[serde(default)]
    pub backend_request_id: String,
    #[serde(default)]
    pub proof_artifact_uri: String,
    #[serde(default)]
    pub proof_artifact: Value,
    #[serde(default)]
    pub artifact_signature_scheme: String,
    #[serde(default)]
    pub artifact_signing_key_id: String,
    #[serde(default)]
    pub artifact_signing_public_key: String,
    #[serde(default)]
    pub artifact_signature_message_hash: String,
    #[serde(default)]
    pub artifact_signature: String,
    #[serde(default)]
    pub artifact_signed_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackExternalProverResponse {
    #[serde(default)]
    pub proof: BlackjackExternalProverProofArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackExternalZkProofPayload {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub proof_encoding: String,
    #[serde(default)]
    pub proof: BlackjackGroth16ProofArtifact,
    #[serde(default)]
    pub proof_bytes_hash: String,
    #[serde(default)]
    pub prover_statement_hash: String,
    #[serde(default)]
    pub prover_public_inputs_hash: String,
    #[serde(default)]
    pub verification_key_hash: String,
    #[serde(default)]
    pub proof_transcript_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackGroth16ProofArtifact {
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub curve: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub a: Vec<String>,
    #[serde(default)]
    pub b: Vec<Vec<String>>,
    #[serde(default)]
    pub c: Vec<String>,
    #[serde(default)]
    pub public_inputs: Vec<String>,
    #[serde(default)]
    pub garaga_calldata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackInsuranceState {
    #[serde(default)]
    pub offered: bool,
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub max_wager: String,
    #[serde(default)]
    pub wager: String,
    #[serde(default)]
    pub taken: bool,
    #[serde(default)]
    pub settled: bool,
    #[serde(default)]
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackEncryptedCardEnvelope {
    pub deck_index: usize,
    #[serde(default)]
    pub card_id: u16,
    #[serde(default)]
    pub commitment: String,
    #[serde(default)]
    pub ciphertext: String,
    #[serde(default)]
    pub nonce_commitment: String,
    #[serde(default)]
    pub reveal_key_commitment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackDealerPeekPrivateWitness {
    pub dealer_hole_rank: u8,
    #[serde(default)]
    pub server_seed_hash: String,
    #[serde(default)]
    pub server_seed: String,
    #[serde(default)]
    pub client_seed: String,
    #[serde(default)]
    pub card_salt: String,
    #[serde(default)]
    pub card: BlackjackEncryptedCardEnvelope,
    #[serde(default)]
    pub opening: BlackjackMerkleOpening,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackTranscriptArtifact {
    #[serde(default)]
    pub protocol_mode: String,
    #[serde(default)]
    pub target_protocol_mode: String,
    #[serde(default)]
    pub entropy_mode: String,
    #[serde(default)]
    pub commitment_scheme: String,
    #[serde(default)]
    pub encryption_scheme: String,
    #[serde(default)]
    pub target_encryption_scheme: String,
    #[serde(default)]
    pub ruleset_hash: String,
    #[serde(default)]
    pub deck_commitment_root: String,
    #[serde(default)]
    pub encrypted_deck_root: String,
    #[serde(default)]
    pub dealer_entropy_commitment: String,
    #[serde(default)]
    pub player_entropy_commitment: String,
    #[serde(default)]
    pub shuffle_commitment: String,
    #[serde(default)]
    pub hole_card_index: usize,
    #[serde(default)]
    pub next_reveal_position: usize,
    #[serde(default)]
    pub cards: Vec<BlackjackCommittedCard>,
    #[serde(default)]
    pub encrypted_cards: Vec<BlackjackEncryptedCardEnvelope>,
    #[serde(default)]
    pub reveals: Vec<BlackjackRevealRecord>,
    #[serde(default)]
    pub dealer_peek: BlackjackDealerPeekState,
    #[serde(default)]
    pub insurance: BlackjackInsuranceState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackCardSnapshot {
    pub rank: u8,
    pub revealed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackSeatSnapshot {
    pub seat_index: u8,
    pub wager: String,
    pub cards: Vec<BlackjackCardSnapshot>,
    pub status: String,
    pub outcome: Option<String>,
    pub payout: String,
    pub doubled: bool,
    #[serde(default)]
    pub split_depth: u8,
    #[serde(default)]
    pub split_aces: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackDealerSnapshot {
    pub cards: Vec<BlackjackCardSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackActionLogEntry {
    pub action: String,
    pub seat_index: Option<u8>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackHandSnapshot {
    pub hand_id: String,
    pub player: String,
    pub table_id: u64,
    pub wager: String,
    pub transcript_root: String,
    #[serde(default)]
    pub server_seed_hash: String,
    #[serde(default)]
    pub server_seed: String,
    #[serde(default)]
    pub client_seed: String,
    pub status: String,
    pub phase: String,
    pub active_seat: u8,
    pub seat_count: u8,
    pub action_count: u8,
    pub split_count: u8,
    pub total_payout: String,
    pub dealer: BlackjackDealerSnapshot,
    pub seats: Vec<BlackjackSeatSnapshot>,
    pub action_log: Vec<BlackjackActionLogEntry>,
    pub shoe: Vec<u16>,
    pub next_card_index: usize,
    #[serde(default)]
    pub insurance: BlackjackInsuranceState,
    #[serde(default)]
    pub transcript_artifact: BlackjackTranscriptArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackCardView {
    pub label: String,
    pub revealed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackSeatView {
    pub seat_index: u8,
    pub wager: String,
    pub status: String,
    pub outcome: Option<String>,
    pub payout: String,
    pub doubled: bool,
    pub total: u8,
    pub soft: bool,
    pub is_blackjack: bool,
    pub active: bool,
    pub can_double: bool,
    pub can_split: bool,
    pub cards: Vec<BlackjackCardView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackDealerView {
    pub cards: Vec<BlackjackCardView>,
    pub total: Option<u8>,
    pub soft: Option<bool>,
    pub hidden_cards: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackHandView {
    pub hand_id: String,
    pub player: String,
    pub table_id: u64,
    pub wager: String,
    pub status: String,
    pub phase: String,
    pub transcript_root: String,
    pub server_seed_hash: String,
    pub server_seed: Option<String>,
    pub client_seed: String,
    pub active_seat: u8,
    pub seat_count: u8,
    pub dealer_upcard: Option<u8>,
    pub total_payout: String,
    pub allowed_actions: Vec<String>,
    pub proof_verified: bool,
    pub insurance: BlackjackInsuranceState,
    pub fairness: BlackjackFairnessView,
    pub dealer: BlackjackDealerView,
    pub seats: Vec<BlackjackSeatView>,
    pub action_log: Vec<BlackjackActionLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackFairnessView {
    #[serde(default)]
    pub protocol_mode: String,
    #[serde(default)]
    pub target_protocol_mode: String,
    #[serde(default)]
    pub encryption_scheme: String,
    #[serde(default)]
    pub target_encryption_scheme: String,
    #[serde(default)]
    pub deck_commitment_root: String,
    #[serde(default)]
    pub reveal_count: u16,
    #[serde(default)]
    pub dealer_peek_required: bool,
    #[serde(default)]
    pub dealer_peek_status: String,
    #[serde(default)]
    pub insurance_offered: bool,
    #[serde(default)]
    pub insurance_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackCommittedCardView {
    pub deck_index: usize,
    #[serde(default)]
    pub card_id: u16,
    #[serde(default)]
    pub commitment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackRevealRecordView {
    pub deck_index: usize,
    #[serde(default)]
    pub card_id: u16,
    pub rank: u8,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub proof_kind: String,
    #[serde(default)]
    pub receipt: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub opening: BlackjackMerkleOpening,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackEncryptedCardEnvelopeView {
    pub deck_index: usize,
    #[serde(default)]
    pub card_id: u16,
    #[serde(default)]
    pub commitment: String,
    #[serde(default)]
    pub ciphertext: String,
    #[serde(default)]
    pub nonce_commitment: String,
    #[serde(default)]
    pub reveal_key_commitment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackFairnessAuditView {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub passed: bool,
    #[serde(default)]
    pub reveal_openings_verified: bool,
    #[serde(default)]
    pub dealer_peek_opening_verified: bool,
    #[serde(default)]
    pub dealer_peek_statement_hash_verified: bool,
    #[serde(default)]
    pub dealer_peek_public_inputs_hash_verified: bool,
    #[serde(default)]
    pub dealer_peek_artifact_consistent: bool,
    #[serde(default)]
    pub dealer_peek_zk_target_consistent: bool,
    #[serde(default)]
    pub dealer_peek_proof_binding_verified: bool,
    #[serde(default)]
    pub settlement_redaction_respected: bool,
    #[serde(default)]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlackjackFairnessArtifactView {
    #[serde(default)]
    pub hand_id: String,
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub table_id: u64,
    #[serde(default)]
    pub transcript_root: String,
    #[serde(default)]
    pub protocol_mode: String,
    #[serde(default)]
    pub target_protocol_mode: String,
    #[serde(default)]
    pub commitment_scheme: String,
    #[serde(default)]
    pub encryption_scheme: String,
    #[serde(default)]
    pub target_encryption_scheme: String,
    #[serde(default)]
    pub ruleset_hash: String,
    #[serde(default)]
    pub deck_commitment_root: String,
    #[serde(default)]
    pub encrypted_deck_root: String,
    #[serde(default)]
    pub dealer_entropy_commitment: String,
    #[serde(default)]
    pub player_entropy_commitment: String,
    #[serde(default)]
    pub shuffle_commitment: String,
    #[serde(default)]
    pub hole_card_index: usize,
    #[serde(default)]
    pub next_reveal_position: usize,
    #[serde(default)]
    pub server_seed_hash: String,
    pub server_seed: Option<String>,
    #[serde(default)]
    pub client_seed_commitment: String,
    pub client_seed: Option<String>,
    #[serde(default)]
    pub settled: bool,
    #[serde(default)]
    pub dealer_peek: BlackjackDealerPeekState,
    #[serde(default)]
    pub insurance: BlackjackInsuranceState,
    #[serde(default)]
    pub committed_cards: Vec<BlackjackCommittedCardView>,
    #[serde(default)]
    pub encrypted_cards: Vec<BlackjackEncryptedCardEnvelopeView>,
    #[serde(default)]
    pub reveals: Vec<BlackjackRevealRecordView>,
    #[serde(default)]
    pub audit: BlackjackFairnessAuditView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackOpenPlan {
    pub dealer_upcard: u8,
    pub dealer_upcard_proof: BlackjackOnchainCardRevealProof,
    pub player_first_card: u8,
    pub player_first_card_proof: BlackjackOnchainCardRevealProof,
    pub player_second_card: u8,
    pub player_second_card_proof: BlackjackOnchainCardRevealProof,
    pub dealer_reveals: Vec<u8>,
    pub dealer_reveal_proofs: Vec<BlackjackOnchainCardRevealProof>,
    pub should_finalize: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackActionPlan {
    pub action: String,
    pub seat_index: u8,
    pub player_draws: Vec<u8>,
    pub player_draw_proofs: Vec<BlackjackOnchainCardRevealProof>,
    pub dealer_reveals: Vec<u8>,
    pub dealer_reveal_proofs: Vec<BlackjackOnchainCardRevealProof>,
    pub should_finalize: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackTimeoutPlan {
    pub action: String,
    pub should_release_reservation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackChainSeat {
    pub seat_index: u8,
    pub wager: String,
    pub status: String,
    pub outcome: Option<String>,
    pub payout: String,
    pub doubled: bool,
    pub cards: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackChainHand {
    pub hand_id: u64,
    pub player: String,
    pub table_id: u64,
    pub wager: String,
    pub status: String,
    pub phase: String,
    pub transcript_root: String,
    pub active_seat: u8,
    pub seat_count: u8,
    pub action_count: u8,
    pub split_count: u8,
    pub dealer_cards: Vec<u8>,
    pub seats: Vec<BlackjackChainSeat>,
    pub total_payout: String,
}

#[derive(Debug, Clone, Copy)]
struct HandMath {
    total: u8,
    soft: bool,
}

#[derive(Debug, Clone, Copy)]
struct DrawnCard {
    deck_index: usize,
    card_id: u16,
    rank: u8,
}

pub fn seed_hand_snapshot(
    hand_id: &str,
    player: &str,
    table_id: u64,
    wager: &str,
    transcript_root: &str,
) -> anyhow::Result<BlackjackHandSnapshot> {
    seed_hand_snapshot_with_secret(
        hand_id,
        player,
        table_id,
        wager,
        transcript_root,
        transcript_root,
        transcript_root,
        None,
    )
}

pub fn seed_hand_snapshot_with_secret(
    hand_id: &str,
    player: &str,
    table_id: u64,
    wager: &str,
    transcript_root: &str,
    server_seed_hash: &str,
    server_seed: &str,
    client_seed: Option<&str>,
) -> anyhow::Result<BlackjackHandSnapshot> {
    let resolved_client_seed = client_seed.unwrap_or_default().trim().to_string();
    let shoe = generate_shoe(server_seed, &resolved_client_seed);
    let transcript_artifact = build_transcript_artifact(
        hand_id,
        player,
        table_id,
        server_seed_hash,
        server_seed,
        &resolved_client_seed,
        &shoe,
    );
    let resolved_transcript_root = if transcript_root.is_empty()
        || same_hex_string(transcript_root, server_seed_hash)
        || !same_hex_string(transcript_root, &transcript_artifact.deck_commitment_root)
    {
        transcript_artifact.deck_commitment_root.clone()
    } else {
        transcript_root.to_string()
    };
    let mut snapshot = BlackjackHandSnapshot {
        hand_id: hand_id.to_string(),
        player: player.to_string(),
        table_id,
        wager: wager.to_string(),
        transcript_root: resolved_transcript_root,
        server_seed_hash: server_seed_hash.to_string(),
        server_seed: server_seed.to_string(),
        client_seed: resolved_client_seed,
        status: "active".to_string(),
        phase: "player_turn".to_string(),
        active_seat: 0,
        seat_count: 1,
        action_count: 0,
        split_count: 0,
        total_payout: "0".to_string(),
        dealer: BlackjackDealerSnapshot { cards: Vec::new() },
        seats: vec![BlackjackSeatSnapshot {
            seat_index: 0,
            wager: wager.to_string(),
            cards: Vec::new(),
            status: "active".to_string(),
            outcome: None,
            payout: "0".to_string(),
            doubled: false,
            split_depth: 0,
            split_aces: false,
        }],
        action_log: Vec::new(),
        shoe,
        next_card_index: 0,
        insurance: BlackjackInsuranceState::default(),
        transcript_artifact,
    };

    let player_first = draw_card(&mut snapshot)?;
    let dealer_up = draw_card(&mut snapshot)?;
    let player_second = draw_card(&mut snapshot)?;
    let dealer_hole = draw_card(&mut snapshot)?;

    snapshot.seats[0].cards.push(BlackjackCardSnapshot {
        rank: player_first.rank,
        revealed: true,
    });
    snapshot.dealer.cards.push(BlackjackCardSnapshot {
        rank: dealer_up.rank,
        revealed: true,
    });
    snapshot.seats[0].cards.push(BlackjackCardSnapshot {
        rank: player_second.rank,
        revealed: true,
    });
    snapshot.dealer.cards.push(BlackjackCardSnapshot {
        rank: dealer_hole.rank,
        revealed: false,
    });
    record_reveal(
        &mut snapshot,
        player_first.deck_index,
        player_first.card_id,
        player_first.rank,
        "opening",
        "player:0:0",
    );
    record_reveal(
        &mut snapshot,
        dealer_up.deck_index,
        dealer_up.card_id,
        dealer_up.rank,
        "opening",
        "dealer:0",
    );
    record_reveal(
        &mut snapshot,
        player_second.deck_index,
        player_second.card_id,
        player_second.rank,
        "opening",
        "player:0:1",
    );
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "deal".to_string(),
        seat_index: None,
        detail: "initial cards committed to the table transcript".to_string(),
    });
    update_dealer_peek_state(
        &mut snapshot,
        dealer_up.rank,
        dealer_hole.rank,
        dealer_hole.deck_index,
        !server_seed_hash.is_empty(),
    );
    update_insurance_state(&mut snapshot)?;

    let opening_math = hand_math_for_seat(&snapshot.seats[0]);
    if is_blackjack(
        snapshot.seats[0].cards.len(),
        opening_math.total,
        snapshot.split_count,
    ) {
        snapshot.seats[0].status = "blackjack".to_string();
        snapshot.action_log.push(BlackjackActionLogEntry {
            action: "blackjack".to_string(),
            seat_index: Some(0),
            detail: "opening hand is a natural blackjack".to_string(),
        });
    }

    if snapshot.insurance.offered && !snapshot.insurance.settled {
        snapshot.status = "awaiting_insurance".to_string();
        snapshot.phase = "insurance".to_string();
    } else {
        refresh_player_phase(&mut snapshot);
    }

    let dealer_blackjack = should_force_opening_dealer_blackjack_reveal(&snapshot);
    if !snapshot.insurance.offered
        && (dealer_blackjack || allowed_actions(&snapshot).is_empty())
        && snapshot.phase != "settled"
    {
        resolve_dealer_phase(&mut snapshot)?;
    }
    Ok(snapshot)
}

pub fn snapshot_to_view(snapshot: &BlackjackHandSnapshot) -> BlackjackHandView {
    let dealer_upcard = snapshot.dealer.cards.first().map(|card| card.rank);
    let dealer_cards = snapshot
        .dealer
        .cards
        .iter()
        .map(|card| BlackjackCardView {
            label: if card.revealed {
                rank_label(card.rank)
            } else {
                "◆".to_string()
            },
            revealed: card.revealed,
        })
        .collect::<Vec<_>>();
    let dealer_visible = visible_ranks(&snapshot.dealer.cards);
    let dealer_math = if dealer_visible.is_empty() {
        None
    } else {
        Some(hand_math(&dealer_visible))
    };

    let seats = snapshot
        .seats
        .iter()
        .map(|seat| {
            let ranks = seat.cards.iter().map(|card| card.rank).collect::<Vec<_>>();
            let math = hand_math(&ranks);
            BlackjackSeatView {
                seat_index: seat.seat_index,
                wager: format_strk_amount(&seat.wager),
                status: seat.status.clone(),
                outcome: seat.outcome.clone(),
                payout: format_strk_amount(&seat.payout),
                doubled: seat.doubled,
                total: math.total,
                soft: math.soft,
                is_blackjack: is_blackjack(ranks.len(), math.total, seat.split_depth),
                active: snapshot.phase == "player_turn"
                    && snapshot.active_seat == seat.seat_index
                    && seat.status == "active",
                can_double: can_double(snapshot, seat),
                can_split: can_split(snapshot, seat),
                cards: seat
                    .cards
                    .iter()
                    .map(|card| BlackjackCardView {
                        label: rank_label(card.rank),
                        revealed: card.revealed,
                    })
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let proof_verified = !snapshot.server_seed.is_empty()
        && (snapshot.phase == "settled" || snapshot.status == "settled")
        && fairness_artifact_view(snapshot).audit.passed;

    BlackjackHandView {
        hand_id: snapshot.hand_id.clone(),
        player: snapshot.player.clone(),
        table_id: snapshot.table_id,
        wager: snapshot.wager.clone(),
        status: snapshot.status.clone(),
        phase: snapshot.phase.clone(),
        transcript_root: snapshot.transcript_root.clone(),
        server_seed_hash: if snapshot.server_seed_hash.is_empty() {
            snapshot.transcript_root.clone()
        } else {
            snapshot.server_seed_hash.clone()
        },
        server_seed: if (snapshot.phase == "settled" || snapshot.status == "settled")
            && !snapshot.server_seed.is_empty()
        {
            Some(snapshot.server_seed.clone())
        } else {
            None
        },
        client_seed: snapshot.client_seed.clone(),
        active_seat: snapshot.active_seat,
        seat_count: snapshot.seat_count,
        dealer_upcard,
        total_payout: snapshot.total_payout.clone(),
        allowed_actions: allowed_actions(snapshot),
        proof_verified,
        insurance: snapshot.insurance.clone(),
        fairness: BlackjackFairnessView {
            protocol_mode: if snapshot.transcript_artifact.protocol_mode.is_empty() {
                BLACKJACK_PROTOCOL_MODE_CURRENT.to_string()
            } else {
                snapshot.transcript_artifact.protocol_mode.clone()
            },
            target_protocol_mode: if snapshot.transcript_artifact.target_protocol_mode.is_empty() {
                BLACKJACK_PROTOCOL_MODE_TARGET.to_string()
            } else {
                snapshot.transcript_artifact.target_protocol_mode.clone()
            },
            encryption_scheme: if snapshot.transcript_artifact.encryption_scheme.is_empty() {
                BLACKJACK_ENCRYPTION_SCHEME_CURRENT.to_string()
            } else {
                snapshot.transcript_artifact.encryption_scheme.clone()
            },
            target_encryption_scheme: if snapshot
                .transcript_artifact
                .target_encryption_scheme
                .is_empty()
            {
                BLACKJACK_ENCRYPTION_SCHEME_TARGET.to_string()
            } else {
                snapshot
                    .transcript_artifact
                    .target_encryption_scheme
                    .clone()
            },
            deck_commitment_root: snapshot.transcript_artifact.deck_commitment_root.clone(),
            reveal_count: snapshot.transcript_artifact.reveals.len() as u16,
            dealer_peek_required: snapshot.transcript_artifact.dealer_peek.required,
            dealer_peek_status: snapshot.transcript_artifact.dealer_peek.outcome.clone(),
            insurance_offered: snapshot.insurance.offered,
            insurance_status: snapshot.insurance.outcome.clone(),
        },
        dealer: BlackjackDealerView {
            cards: dealer_cards,
            total: dealer_math.map(|value| value.total),
            soft: dealer_math.map(|value| value.soft),
            hidden_cards: snapshot
                .dealer
                .cards
                .iter()
                .filter(|card| !card.revealed)
                .count() as u8,
        },
        seats,
        action_log: snapshot.action_log.clone(),
    }
}

pub fn fairness_artifact_view(snapshot: &BlackjackHandSnapshot) -> BlackjackFairnessArtifactView {
    let settled = snapshot.phase == "settled" || snapshot.status == "settled";
    let mut artifact = BlackjackFairnessArtifactView {
        hand_id: snapshot.hand_id.clone(),
        player: snapshot.player.clone(),
        table_id: snapshot.table_id,
        transcript_root: snapshot.transcript_root.clone(),
        protocol_mode: if snapshot.transcript_artifact.protocol_mode.is_empty() {
            BLACKJACK_PROTOCOL_MODE_CURRENT.to_string()
        } else {
            snapshot.transcript_artifact.protocol_mode.clone()
        },
        target_protocol_mode: if snapshot.transcript_artifact.target_protocol_mode.is_empty() {
            BLACKJACK_PROTOCOL_MODE_TARGET.to_string()
        } else {
            snapshot.transcript_artifact.target_protocol_mode.clone()
        },
        commitment_scheme: snapshot.transcript_artifact.commitment_scheme.clone(),
        encryption_scheme: if snapshot.transcript_artifact.encryption_scheme.is_empty() {
            BLACKJACK_ENCRYPTION_SCHEME_CURRENT.to_string()
        } else {
            snapshot.transcript_artifact.encryption_scheme.clone()
        },
        target_encryption_scheme: if snapshot
            .transcript_artifact
            .target_encryption_scheme
            .is_empty()
        {
            BLACKJACK_ENCRYPTION_SCHEME_TARGET.to_string()
        } else {
            snapshot
                .transcript_artifact
                .target_encryption_scheme
                .clone()
        },
        ruleset_hash: snapshot.transcript_artifact.ruleset_hash.clone(),
        deck_commitment_root: snapshot.transcript_artifact.deck_commitment_root.clone(),
        encrypted_deck_root: snapshot.transcript_artifact.encrypted_deck_root.clone(),
        dealer_entropy_commitment: snapshot
            .transcript_artifact
            .dealer_entropy_commitment
            .clone(),
        player_entropy_commitment: snapshot
            .transcript_artifact
            .player_entropy_commitment
            .clone(),
        shuffle_commitment: snapshot.transcript_artifact.shuffle_commitment.clone(),
        hole_card_index: snapshot.transcript_artifact.hole_card_index,
        next_reveal_position: snapshot.transcript_artifact.next_reveal_position,
        server_seed_hash: snapshot.server_seed_hash.clone(),
        server_seed: if settled && !snapshot.server_seed.is_empty() {
            Some(snapshot.server_seed.clone())
        } else {
            None
        },
        client_seed_commitment: snapshot
            .transcript_artifact
            .player_entropy_commitment
            .clone(),
        client_seed: if settled && !snapshot.client_seed.is_empty() {
            Some(snapshot.client_seed.clone())
        } else {
            None
        },
        settled,
        dealer_peek: snapshot.transcript_artifact.dealer_peek.clone(),
        insurance: snapshot.insurance.clone(),
        committed_cards: snapshot
            .transcript_artifact
            .cards
            .iter()
            .map(|card| BlackjackCommittedCardView {
                deck_index: card.deck_index,
                card_id: card.card_id,
                commitment: card.commitment.clone(),
            })
            .collect(),
        encrypted_cards: snapshot
            .transcript_artifact
            .encrypted_cards
            .iter()
            .map(|card| BlackjackEncryptedCardEnvelopeView {
                deck_index: card.deck_index,
                card_id: card.card_id,
                commitment: card.commitment.clone(),
                ciphertext: card.ciphertext.clone(),
                nonce_commitment: card.nonce_commitment.clone(),
                reveal_key_commitment: card.reveal_key_commitment.clone(),
            })
            .collect(),
        reveals: snapshot
            .transcript_artifact
            .reveals
            .iter()
            .map(|reveal| BlackjackRevealRecordView {
                deck_index: reveal.deck_index,
                card_id: reveal.card_id,
                rank: reveal.rank,
                stage: reveal.stage.clone(),
                target: reveal.target.clone(),
                proof_kind: reveal.receipt.proof_kind.clone(),
                receipt: reveal.receipt.receipt.clone(),
                verified: reveal.receipt.verified,
                opening: reveal.opening.clone(),
            })
            .collect(),
        audit: BlackjackFairnessAuditView::default(),
    };
    artifact.audit = audit_fairness_artifact_view(&artifact);
    artifact
}

pub fn audit_fairness_artifact_view(
    artifact: &BlackjackFairnessArtifactView,
) -> BlackjackFairnessAuditView {
    let reveal_openings_verified = artifact.reveals.iter().all(|reveal| {
        artifact
            .encrypted_cards
            .iter()
            .find(|card| card.deck_index == reveal.deck_index)
            .is_some_and(|card| {
                verify_encrypted_card_envelope_opening(
                    card,
                    &reveal.opening,
                    &artifact.encrypted_deck_root,
                )
            })
    });

    let dealer_peek_opening_verified = if artifact.dealer_peek.required {
        artifact
            .dealer_peek
            .hole_card_index
            .and_then(|hole_card_index| {
                artifact
                    .encrypted_cards
                    .iter()
                    .find(|card| card.deck_index == hole_card_index)
            })
            .is_some_and(|card| {
                verify_encrypted_card_envelope_opening(
                    card,
                    &artifact.dealer_peek.opening,
                    &artifact.encrypted_deck_root,
                )
            })
    } else {
        true
    };

    let dealer_peek_statement_hash_verified = verify_no_blackjack_statement_binding(artifact);
    let dealer_peek_public_inputs_hash_verified =
        verify_no_blackjack_public_inputs_binding(artifact);
    let dealer_peek_artifact_consistent = verify_no_blackjack_artifact_consistency(artifact);
    let dealer_peek_zk_target_consistent = verify_no_blackjack_zk_target_binding(artifact);
    let dealer_peek_proof_binding_verified =
        verify_no_blackjack_proof_binding_for_artifact(artifact);
    let settlement_redaction_respected = if artifact.settled {
        true
    } else {
        artifact.server_seed.is_none() && artifact.client_seed.is_none()
    };

    let mut issues = Vec::new();
    if !reveal_openings_verified {
        issues.push("reveal_openings_invalid".to_string());
    }
    if !dealer_peek_opening_verified {
        issues.push("dealer_peek_opening_invalid".to_string());
    }
    if !dealer_peek_statement_hash_verified {
        issues.push("dealer_peek_statement_hash_mismatch".to_string());
    }
    if !dealer_peek_public_inputs_hash_verified {
        issues.push("dealer_peek_public_inputs_hash_mismatch".to_string());
    }
    if !dealer_peek_artifact_consistent {
        issues.push("dealer_peek_artifact_inconsistent".to_string());
    }
    if !dealer_peek_zk_target_consistent {
        issues.push("dealer_peek_zk_target_invalid".to_string());
    }
    if !dealer_peek_proof_binding_verified {
        issues.push("dealer_peek_proof_binding_invalid".to_string());
    }
    if !settlement_redaction_respected {
        issues.push("premature_seed_disclosure".to_string());
    }

    BlackjackFairnessAuditView {
        mode: "public_artifact_consistency_v2".to_string(),
        passed: issues.is_empty(),
        reveal_openings_verified,
        dealer_peek_opening_verified,
        dealer_peek_statement_hash_verified,
        dealer_peek_public_inputs_hash_verified,
        dealer_peek_artifact_consistent,
        dealer_peek_zk_target_consistent,
        dealer_peek_proof_binding_verified,
        settlement_redaction_respected,
        issues,
    }
}

pub fn verify_encrypted_card_envelope_opening(
    card: &BlackjackEncryptedCardEnvelopeView,
    opening: &BlackjackMerkleOpening,
    expected_root: &str,
) -> bool {
    if opening.leaf_index != card.deck_index || opening.root != expected_root {
        return false;
    }
    let expected_leaf_hash = encrypted_leaf_hash_view(card);
    if opening.leaf_hash != expected_leaf_hash {
        return false;
    }
    let recomputed_root =
        merkle_root_from_opening(&opening.leaf_hash, opening.leaf_index, &opening.siblings);
    !expected_root.is_empty() && opening.root == recomputed_root && opening.verified
}

fn onchain_merkle_root_from_opening(
    leaf_hash: &str,
    leaf_index: usize,
    siblings: &[String],
) -> anyhow::Result<String> {
    let mut hash = bn254_field_from_hex_string(leaf_hash, "blackjack onchain opening leaf hash")?;
    let mut index = leaf_index;
    for sibling in siblings {
        let sibling = bn254_field_from_hex_string(sibling, "blackjack onchain opening sibling")?;
        hash = if index % 2 == 0 {
            poseidon_bn128(2, &[hash, sibling])?
        } else {
            poseidon_bn128(2, &[sibling, hash])?
        };
        index /= 2;
    }
    Ok(bn254_hex(&hash))
}

fn verify_onchain_card_private_opening(
    table_id: u64,
    player: &str,
    card_id: u16,
    deck_index: usize,
    card_salt: &str,
    opening: &BlackjackMerkleOpening,
    expected_root: &str,
) -> bool {
    if opening.leaf_index != deck_index || !same_hex_string(&opening.root, expected_root) {
        return false;
    }
    let player = match felt_from_hex_string(player, "blackjack player address") {
        Ok(player) => player,
        Err(_) => return false,
    };
    let expected_leaf_hash =
        match onchain_card_leaf(table_id, player, deck_index, card_id, card_salt) {
            Ok(hash) => hash,
            Err(_) => return false,
        };
    if !same_hex_string(&opening.leaf_hash, &expected_leaf_hash) {
        return false;
    }
    let recomputed_root = match onchain_merkle_root_from_opening(
        &opening.leaf_hash,
        opening.leaf_index,
        &opening.siblings,
    ) {
        Ok(root) => root,
        Err(_) => return false,
    };
    !expected_root.is_empty()
        && same_hex_string(&opening.root, &recomputed_root)
        && opening.verified
}

pub fn compute_no_blackjack_statement_hash(
    statement: &BlackjackNoBlackjackProofStatement,
) -> String {
    hash_hex(format!(
        "moros:blackjack:peek-statement:v1:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        statement.hand_id,
        statement.player,
        statement.table_id,
        statement.ruleset_hash,
        statement.deck_commitment_root,
        statement.encrypted_deck_root,
        statement.dealer_upcard_rank.unwrap_or_default(),
        statement.hole_card_index.unwrap_or_default(),
        statement.statement_kind
    ))
}

fn compute_peek_public_inputs_hash(
    statement_hash: &str,
    statement_kind: &str,
    target_proof_kind: &str,
) -> String {
    hash_hex(format!(
        "moros:blackjack:peek-public-inputs:v1:{}:{}:{}",
        statement_hash, statement_kind, target_proof_kind
    ))
}

fn compute_no_blackjack_zk_target_artifact_hash(
    claim: &str,
    statement_hash: &str,
    public_inputs_hash: &str,
    encrypted_deck_root: &str,
    dealer_upcard_rank: u8,
    hole_card_index: usize,
    hidden_value_class_commitment: &str,
    witness_commitment: &str,
    hole_card_rank_commitment: &str,
) -> String {
    hash_hex(format!(
        "moros:blackjack:peek-zk-target:v1:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        BLACKJACK_PEEK_VERIFIER_NAMESPACE,
        BLACKJACK_PEEK_VERIFIER_KIND_TARGET,
        BLACKJACK_PEEK_PROOF_SYSTEM_TARGET,
        BLACKJACK_PEEK_CIRCUIT_ID_TARGET,
        claim,
        statement_hash,
        public_inputs_hash,
        encrypted_deck_root,
        dealer_upcard_rank,
        hole_card_index,
        hash_hex(format!(
            "{}:{}:{}",
            hidden_value_class_commitment, witness_commitment, hole_card_rank_commitment
        ))
    ))
}

fn compute_no_blackjack_zk_target_request_id(artifact_hash: &str) -> String {
    hash_hex(format!(
        "moros:blackjack:peek-zk-request:v1:{}:{}:{}",
        BLACKJACK_PEEK_VERIFIER_NAMESPACE, BLACKJACK_PEEK_CIRCUIT_ID_TARGET, artifact_hash
    ))
}

fn compute_no_blackjack_proof_binding_id(request_id: &str) -> String {
    hash_hex(format!(
        "moros:blackjack:peek-proof-binding-id:v1:{}:{}",
        BLACKJACK_PEEK_CIRCUIT_ID_TARGET, request_id
    ))
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(canonicalize_json_value)
                .collect::<Vec<_>>(),
        ),
        Value::Object(map) => {
            let mut ordered = serde_json::Map::new();
            let sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_json_value(value)))
                .collect::<BTreeMap<_, _>>();
            for (key, value) in sorted {
                ordered.insert(key, value);
            }
            Value::Object(ordered)
        }
        other => other.clone(),
    }
}

pub fn hash_blackjack_external_proof_artifact(artifact: &Value) -> anyhow::Result<String> {
    ensure!(
        !artifact.is_null(),
        "external blackjack proof artifact is required"
    );
    let canonical = canonicalize_json_value(artifact);
    let encoded = serde_json::to_vec(&canonical)
        .context("failed to canonicalize external blackjack proof artifact")?;
    Ok(hash_hex(encoded))
}

fn compute_blackjack_external_verification_key_hash(
    target: &BlackjackZkProofTargetArtifact,
) -> String {
    hash_hex(format!(
        "moros:blackjack:external-proof-vk:v1:{}:{}:{}:{}",
        target.proof_system, target.circuit_family, target.circuit_id, target.verification_key_id
    ))
}

fn poseidon_hex(elements: &[Felt]) -> String {
    format!("{:#x}", poseidon_hash_many(elements))
}

fn hash_blackjack_external_proof_bytes(
    proof: &BlackjackGroth16ProofArtifact,
) -> anyhow::Result<String> {
    validate_blackjack_groth16_proof_shape(proof)?;
    let canonical = canonicalize_json_value(
        &serde_json::to_value(proof).context("failed to serialize external groth16 proof")?,
    );
    let encoded = serde_json::to_vec(&canonical)
        .context("failed to canonicalize external groth16 proof payload")?;
    Ok(hash_hex(encoded))
}

fn compute_blackjack_external_proof_transcript_hash(
    target: &BlackjackZkProofTargetArtifact,
    proof_bytes_hash: &str,
) -> String {
    hash_hex(format!(
        "moros:blackjack:external-proof-transcript:v1:{}:{}:{}:{}:{}:{}",
        target.request_id,
        target.claim,
        target.statement_hash,
        target.public_inputs_hash,
        target.circuit_id,
        proof_bytes_hash
    ))
}

fn compute_hole_card_rank_commitment(
    hand_id: &str,
    player: &str,
    table_id: u64,
    transcript_root: &str,
    claim: &str,
    statement_hash: &str,
    public_inputs_hash: &str,
    encrypted_deck_root: &str,
    dealer_upcard_rank: u8,
    hole_card_index: usize,
    witness: &BlackjackDealerPeekPrivateWitness,
) -> String {
    poseidon_hex(&[
        felt_from_label("moros.blackjack.hole_card_rank_commitment.v3"),
        felt_from_text_or_hex("blackjack.hand_id", hand_id),
        felt_from_text_or_hex("blackjack.player", player),
        Felt::from(table_id),
        felt_from_text_or_hex("blackjack.transcript_root", transcript_root),
        felt_from_text_or_hex("blackjack.peek.claim", claim),
        felt_from_text_or_hex("blackjack.peek.statement_hash", statement_hash),
        felt_from_text_or_hex("blackjack.peek.public_inputs_hash", public_inputs_hash),
        felt_from_text_or_hex("blackjack.encrypted_deck_root", encrypted_deck_root),
        Felt::from(dealer_upcard_rank),
        Felt::from(hole_card_index as u64),
        Felt::from(witness.dealer_hole_rank),
        felt_from_text_or_hex("blackjack.server_seed_hash", &witness.server_seed_hash),
        felt_from_text_or_hex("blackjack.server_seed", &witness.server_seed),
        felt_from_text_or_hex("blackjack.client_seed", &witness.client_seed),
        Felt::from(witness.card.deck_index as u64),
        Felt::from(witness.card.card_id as u64),
        felt_from_text_or_hex("blackjack.card.commitment", &witness.card.commitment),
        felt_from_text_or_hex("blackjack.card.ciphertext", &witness.card.ciphertext),
        felt_from_text_or_hex(
            "blackjack.card.nonce_commitment",
            &witness.card.nonce_commitment,
        ),
        felt_from_text_or_hex(
            "blackjack.card.reveal_key_commitment",
            &witness.card.reveal_key_commitment,
        ),
        felt_from_text_or_hex("blackjack.opening.leaf_hash", &witness.opening.leaf_hash),
        felt_from_text_or_hex("blackjack.opening.root", &witness.opening.root),
        felt_from_text_or_hex(
            "blackjack.opening.siblings",
            &witness.opening.siblings.join(","),
        ),
    ])
}

fn compute_hole_card_rank_commitment_from_private_witness(
    hand_id: &str,
    player: &str,
    table_id: u64,
    transcript_root: &str,
    target: &BlackjackZkProofTargetArtifact,
    witness: &BlackjackDealerPeekPrivateWitness,
) -> anyhow::Result<String> {
    let dealer_upcard_rank = target
        .dealer_upcard_rank
        .context("zk target is missing dealer_upcard_rank")?;
    let hole_card_index = target
        .hole_card_index
        .context("zk target is missing hole_card_index")?;
    Ok(compute_hole_card_rank_commitment(
        hand_id,
        player,
        table_id,
        transcript_root,
        &target.claim,
        &target.statement_hash,
        &target.public_inputs_hash,
        &target.encrypted_deck_root,
        dealer_upcard_rank,
        hole_card_index,
        witness,
    ))
}

fn parse_u128_decimal(value: &str, label: &str) -> anyhow::Result<u128> {
    value
        .trim()
        .parse::<u128>()
        .with_context(|| format!("{label} must be a valid u128 decimal string"))
}

fn compute_hand_hash_for_peek_public_inputs(
    chain_hand_id: u64,
    table_id: u64,
    wager: u128,
    transcript_root: &str,
    dealer_upcard: u8,
    player_first_card: u8,
    player_second_card: u8,
) -> anyhow::Result<String> {
    let mut current = poseidon_bn128(
        2,
        &[
            Bn128FieldElement::from(chain_hand_id),
            Bn128FieldElement::from(table_id),
        ],
    )?;
    current = poseidon_bn128(2, &[current, bn254_field_from_u128(wager)])?;
    current = poseidon_bn128(
        2,
        &[
            current,
            bn254_field_from_hex_string(transcript_root, "blackjack transcript root")?,
        ],
    )?;
    current = poseidon_bn128(2, &[current, Bn128FieldElement::from(dealer_upcard as u64)])?;
    current = poseidon_bn128(
        2,
        &[current, Bn128FieldElement::from(player_first_card as u64)],
    )?;
    current = poseidon_bn128(
        2,
        &[current, Bn128FieldElement::from(player_second_card as u64)],
    )?;
    Ok(bn254_hex(&current))
}

fn compute_no_blackjack_groth16_public_inputs(
    request: &BlackjackZkPeekProofRequest,
) -> anyhow::Result<Vec<String>> {
    let context = request
        .onchain_context
        .as_ref()
        .context("zk proof request is missing onchain_context")?;
    let target = &request.target;
    Ok(vec![
        compute_hand_hash_for_peek_public_inputs(
            context.chain_hand_id,
            context.table_id,
            parse_u128_decimal(&context.wager, "blackjack wager")?,
            &request.transcript_root,
            context.dealer_upcard,
            context.player_first_card,
            context.player_second_card,
        )?,
        request.transcript_root.trim().to_string(),
        target
            .dealer_upcard_rank
            .context("zk target is missing dealer_upcard_rank")?
            .to_string(),
        target
            .hole_card_index
            .context("zk target is missing hole_card_index")?
            .to_string(),
        if context.dealer_blackjack { "1" } else { "0" }.to_string(),
        context.chain_hand_id.to_string(),
        context.table_id.to_string(),
        context.wager.trim().to_string(),
        context.dealer_upcard.to_string(),
        context.player_first_card.to_string(),
        context.player_second_card.to_string(),
    ])
}

fn validate_blackjack_groth16_proof_shape(
    proof: &BlackjackGroth16ProofArtifact,
) -> anyhow::Result<()> {
    ensure!(
        proof.scheme == BLACKJACK_EXTERNAL_PROOF_SCHEME_GROTH16,
        "external blackjack proof scheme mismatch"
    );
    ensure!(
        proof.curve == BLACKJACK_EXTERNAL_PROOF_CURVE_BN254,
        "external blackjack proof curve mismatch"
    );
    ensure!(
        matches!(
            proof.backend.as_str(),
            BLACKJACK_EXTERNAL_PROOF_BACKEND_CIRCOM | BLACKJACK_EXTERNAL_PROOF_BACKEND_FIXTURE
        ),
        "external blackjack proof backend mismatch"
    );
    ensure!(
        proof.a.len() == 2,
        "external blackjack proof point A shape mismatch"
    );
    ensure!(
        proof.b.len() == 2 && proof.b.iter().all(|row| row.len() == 2),
        "external blackjack proof point B shape mismatch"
    );
    ensure!(
        proof.c.len() == 2,
        "external blackjack proof point C shape mismatch"
    );
    ensure!(
        !proof.public_inputs.is_empty(),
        "external blackjack proof public inputs are missing"
    );
    ensure!(
        proof.a.iter().all(|value| !value.trim().is_empty())
            && proof
                .b
                .iter()
                .flat_map(|row| row.iter())
                .all(|value| !value.trim().is_empty())
            && proof.c.iter().all(|value| !value.trim().is_empty())
            && proof
                .public_inputs
                .iter()
                .all(|value| !value.trim().is_empty()),
        "external blackjack proof contains empty coordinates or public inputs"
    );
    Ok(())
}

fn build_blackjack_groth16_fixture_proof(
    request: &BlackjackZkPeekProofRequest,
) -> anyhow::Result<BlackjackGroth16ProofArtifact> {
    validate_no_blackjack_zk_proof_request(request)?;
    validate_no_blackjack_private_witness(request)?;
    let target = &request.target;
    let witness = request
        .private_witness
        .as_ref()
        .context("zk proof request is missing private dealer peek witness")?;
    let public_inputs = compute_no_blackjack_groth16_public_inputs(request)?;
    let fixture_seed = poseidon_hash_many(&[
        felt_from_label("moros.blackjack.groth16_fixture.v1"),
        felt_from_text_or_hex("blackjack.request_id", &target.request_id),
        felt_from_text_or_hex("blackjack.statement_hash", &target.statement_hash),
        felt_from_text_or_hex("blackjack.public_inputs_hash", &target.public_inputs_hash),
        Felt::from(witness.dealer_hole_rank),
        Felt::from(witness.card.card_id as u64),
        felt_from_text_or_hex("blackjack.card.commitment", &witness.card.commitment),
        felt_from_text_or_hex("blackjack.card.ciphertext", &witness.card.ciphertext),
        felt_from_text_or_hex("blackjack.opening.root", &witness.opening.root),
    ]);
    let a = vec![
        poseidon_hex(&[
            felt_from_label("moros.blackjack.groth16_fixture.a0"),
            fixture_seed,
        ]),
        poseidon_hex(&[
            felt_from_label("moros.blackjack.groth16_fixture.a1"),
            fixture_seed,
        ]),
    ];
    let b = vec![
        vec![
            poseidon_hex(&[
                felt_from_label("moros.blackjack.groth16_fixture.b00"),
                fixture_seed,
            ]),
            poseidon_hex(&[
                felt_from_label("moros.blackjack.groth16_fixture.b01"),
                fixture_seed,
            ]),
        ],
        vec![
            poseidon_hex(&[
                felt_from_label("moros.blackjack.groth16_fixture.b10"),
                fixture_seed,
            ]),
            poseidon_hex(&[
                felt_from_label("moros.blackjack.groth16_fixture.b11"),
                fixture_seed,
            ]),
        ],
    ];
    let c = vec![
        poseidon_hex(&[
            felt_from_label("moros.blackjack.groth16_fixture.c0"),
            fixture_seed,
        ]),
        poseidon_hex(&[
            felt_from_label("moros.blackjack.groth16_fixture.c1"),
            fixture_seed,
        ]),
    ];
    let mut garaga_calldata = Vec::new();
    garaga_calldata.extend(a.clone());
    for row in &b {
        garaga_calldata.extend(row.clone());
    }
    garaga_calldata.extend(c.clone());
    garaga_calldata.extend(public_inputs.clone());
    Ok(BlackjackGroth16ProofArtifact {
        scheme: BLACKJACK_EXTERNAL_PROOF_SCHEME_GROTH16.to_string(),
        curve: BLACKJACK_EXTERNAL_PROOF_CURVE_BN254.to_string(),
        backend: BLACKJACK_EXTERNAL_PROOF_BACKEND_FIXTURE.to_string(),
        a,
        b,
        c,
        public_inputs,
        garaga_calldata,
    })
}

pub fn build_no_blackjack_zk_proof_payload(
    request: &BlackjackZkPeekProofRequest,
) -> anyhow::Result<BlackjackExternalZkProofPayload> {
    let proof = build_blackjack_groth16_fixture_proof(request)?;
    build_blackjack_external_zk_proof_payload(&request.target, proof)
}

pub fn build_blackjack_external_zk_proof_payload(
    target: &BlackjackZkProofTargetArtifact,
    proof: BlackjackGroth16ProofArtifact,
) -> anyhow::Result<BlackjackExternalZkProofPayload> {
    let proof_bytes_hash = hash_blackjack_external_proof_bytes(&proof)?;
    Ok(BlackjackExternalZkProofPayload {
        schema_version: BLACKJACK_EXTERNAL_PROOF_PAYLOAD_SCHEMA_VERSION.to_string(),
        proof_encoding: BLACKJACK_EXTERNAL_PROOF_PAYLOAD_ENCODING.to_string(),
        proof,
        proof_bytes_hash: proof_bytes_hash.clone(),
        prover_statement_hash: target.statement_hash.clone(),
        prover_public_inputs_hash: target.public_inputs_hash.clone(),
        verification_key_hash: compute_blackjack_external_verification_key_hash(target),
        proof_transcript_hash: compute_blackjack_external_proof_transcript_hash(
            target,
            &proof_bytes_hash,
        ),
    })
}

pub fn validate_blackjack_external_zk_proof_payload(
    target: &BlackjackZkProofTargetArtifact,
    payload: &BlackjackExternalZkProofPayload,
) -> anyhow::Result<()> {
    ensure!(
        payload.schema_version == BLACKJACK_EXTERNAL_PROOF_PAYLOAD_SCHEMA_VERSION,
        "external blackjack proof payload schema version mismatch"
    );
    ensure!(
        payload.proof_encoding == BLACKJACK_EXTERNAL_PROOF_PAYLOAD_ENCODING,
        "external blackjack proof payload encoding mismatch"
    );
    validate_blackjack_groth16_proof_shape(&payload.proof)?;
    ensure!(
        payload.proof.public_inputs.len() == 11,
        "external blackjack proof public input length mismatch"
    );
    let expected_proof_bytes_hash = hash_blackjack_external_proof_bytes(&payload.proof)?;
    ensure!(
        payload.proof_bytes_hash == expected_proof_bytes_hash,
        "external blackjack proof payload bytes hash mismatch"
    );
    ensure!(
        payload.prover_statement_hash == target.statement_hash,
        "external blackjack proof payload statement hash mismatch"
    );
    ensure!(
        payload.prover_public_inputs_hash == target.public_inputs_hash,
        "external blackjack proof payload public inputs hash mismatch"
    );
    ensure!(
        payload.verification_key_hash == compute_blackjack_external_verification_key_hash(target),
        "external blackjack proof payload verification key hash mismatch"
    );
    ensure!(
        payload.proof_transcript_hash
            == compute_blackjack_external_proof_transcript_hash(target, &expected_proof_bytes_hash),
        "external blackjack proof payload transcript hash mismatch"
    );
    Ok(())
}

fn build_no_blackjack_zk_proof_target(
    claim: &str,
    statement_hash: &str,
    public_inputs_hash: &str,
    encrypted_deck_root: &str,
    dealer_upcard_rank: u8,
    hole_card_index: usize,
    hidden_value_class_commitment: &str,
    witness_commitment: &str,
    hole_card_rank_commitment: &str,
) -> BlackjackZkProofTargetArtifact {
    let artifact_hash = compute_no_blackjack_zk_target_artifact_hash(
        claim,
        statement_hash,
        public_inputs_hash,
        encrypted_deck_root,
        dealer_upcard_rank,
        hole_card_index,
        hidden_value_class_commitment,
        witness_commitment,
        hole_card_rank_commitment,
    );
    BlackjackZkProofTargetArtifact {
        available: true,
        verifier_namespace: BLACKJACK_PEEK_VERIFIER_NAMESPACE.to_string(),
        verifier_kind: BLACKJACK_PEEK_VERIFIER_KIND_TARGET.to_string(),
        proof_system: BLACKJACK_PEEK_PROOF_SYSTEM_TARGET.to_string(),
        circuit_family: BLACKJACK_PEEK_CIRCUIT_FAMILY_TARGET.to_string(),
        circuit_id: BLACKJACK_PEEK_CIRCUIT_ID_TARGET.to_string(),
        verification_key_id: BLACKJACK_PEEK_VERIFICATION_KEY_ID_TARGET.to_string(),
        claim: claim.to_string(),
        statement_hash: statement_hash.to_string(),
        public_inputs_hash: public_inputs_hash.to_string(),
        encrypted_deck_root: encrypted_deck_root.to_string(),
        dealer_upcard_rank: Some(dealer_upcard_rank),
        hole_card_index: Some(hole_card_index),
        hidden_value_class_commitment: hidden_value_class_commitment.to_string(),
        witness_commitment: witness_commitment.to_string(),
        hole_card_rank_commitment: hole_card_rank_commitment.to_string(),
        artifact_hash: artifact_hash.clone(),
        request_id: compute_no_blackjack_zk_target_request_id(&artifact_hash),
    }
}

pub fn build_no_blackjack_zk_proof_request(
    artifact: &BlackjackFairnessArtifactView,
) -> Option<BlackjackZkPeekProofRequest> {
    let proof = &artifact.dealer_peek.no_blackjack_proof;
    if !proof.available || !proof.zk_proof_target.available {
        return None;
    }
    Some(BlackjackZkPeekProofRequest {
        hand_id: artifact.hand_id.clone(),
        player: artifact.player.clone(),
        table_id: artifact.table_id,
        transcript_root: artifact.transcript_root.clone(),
        target: proof.zk_proof_target.clone(),
        private_witness: None,
        onchain_context: None,
    })
}

pub fn build_no_blackjack_zk_proof_request_with_witness(
    snapshot: &BlackjackHandSnapshot,
) -> Option<BlackjackZkPeekProofRequest> {
    let fairness = fairness_artifact_view(snapshot);
    let mut request = build_no_blackjack_zk_proof_request(&fairness)?;
    let dealer_hole_rank = snapshot.dealer.cards.get(1)?.rank;
    let hole_card_index = snapshot.transcript_artifact.dealer_peek.hole_card_index?;
    let card = snapshot
        .transcript_artifact
        .encrypted_cards
        .get(hole_card_index)?
        .clone();
    let card_salt = snapshot
        .transcript_artifact
        .cards
        .get(hole_card_index)?
        .onchain_salt
        .clone();
    let opening = onchain_private_opening(snapshot, hole_card_index).ok()?;
    request.private_witness = Some(BlackjackDealerPeekPrivateWitness {
        dealer_hole_rank,
        server_seed_hash: snapshot.server_seed_hash.clone(),
        server_seed: snapshot.server_seed.clone(),
        client_seed: snapshot.client_seed.clone(),
        card_salt,
        card,
        opening: opening,
    });
    let witness = request.private_witness.as_ref()?;
    let hole_card_rank_commitment = compute_hole_card_rank_commitment_from_private_witness(
        &request.hand_id,
        &request.player,
        request.table_id,
        &request.transcript_root,
        &request.target,
        witness,
    )
    .ok()?;
    request.target.hole_card_rank_commitment = hole_card_rank_commitment.clone();
    let dealer_upcard_rank = request.target.dealer_upcard_rank?;
    let hole_card_index = request.target.hole_card_index?;
    request.target.artifact_hash = compute_no_blackjack_zk_target_artifact_hash(
        &request.target.claim,
        &request.target.statement_hash,
        &request.target.public_inputs_hash,
        &request.target.encrypted_deck_root,
        dealer_upcard_rank,
        hole_card_index,
        &request.target.hidden_value_class_commitment,
        &request.target.witness_commitment,
        &hole_card_rank_commitment,
    );
    request.target.request_id =
        compute_no_blackjack_zk_target_request_id(&request.target.artifact_hash);
    Some(request)
}

pub fn validate_no_blackjack_zk_proof_request(
    request: &BlackjackZkPeekProofRequest,
) -> anyhow::Result<()> {
    let target = &request.target;
    ensure!(target.available, "zk target is not available");
    let dealer_upcard_rank = target
        .dealer_upcard_rank
        .context("zk target is missing dealer_upcard_rank")?;
    let hole_card_index = target
        .hole_card_index
        .context("zk target is missing hole_card_index")?;
    ensure!(
        target.verifier_namespace == BLACKJACK_PEEK_VERIFIER_NAMESPACE,
        "zk target verifier namespace mismatch"
    );
    ensure!(
        target.verifier_kind == BLACKJACK_PEEK_VERIFIER_KIND_TARGET,
        "zk target verifier kind mismatch"
    );
    ensure!(
        target.proof_system == BLACKJACK_PEEK_PROOF_SYSTEM_TARGET,
        "zk target proof system mismatch"
    );
    ensure!(
        target.circuit_family == BLACKJACK_PEEK_CIRCUIT_FAMILY_TARGET,
        "zk target circuit family mismatch"
    );
    ensure!(
        target.circuit_id == BLACKJACK_PEEK_CIRCUIT_ID_TARGET,
        "zk target circuit id mismatch"
    );
    ensure!(
        target.verification_key_id == BLACKJACK_PEEK_VERIFICATION_KEY_ID_TARGET,
        "zk target verification key mismatch"
    );
    let expected_artifact_hash = compute_no_blackjack_zk_target_artifact_hash(
        &target.claim,
        &target.statement_hash,
        &target.public_inputs_hash,
        &target.encrypted_deck_root,
        dealer_upcard_rank,
        hole_card_index,
        &target.hidden_value_class_commitment,
        &target.witness_commitment,
        &target.hole_card_rank_commitment,
    );
    ensure!(
        target.artifact_hash == expected_artifact_hash,
        "zk target artifact hash mismatch"
    );
    ensure!(
        target.request_id == compute_no_blackjack_zk_target_request_id(&expected_artifact_hash),
        "zk target request id mismatch"
    );
    Ok(())
}

pub fn build_no_blackjack_proof_binding(
    request: &BlackjackZkPeekProofRequest,
) -> anyhow::Result<BlackjackProofBindingArtifact> {
    validate_no_blackjack_zk_proof_request(request)?;
    validate_no_blackjack_private_witness(request)?;
    let target = &request.target;
    let proof_id = compute_no_blackjack_proof_binding_id(&target.request_id);
    Ok(BlackjackProofBindingArtifact {
        available: true,
        status: BLACKJACK_PEEK_PROOF_BINDING_STATUS_VERIFIED.to_string(),
        request_bound: true,
        proof_verified: true,
        verifier_namespace: BLACKJACK_PEEK_VERIFIER_NAMESPACE.to_string(),
        verifier_kind: BLACKJACK_PEEK_VERIFIER_KIND_TARGET.to_string(),
        proof_system: BLACKJACK_PEEK_PROOF_SYSTEM_TARGET.to_string(),
        circuit_family: BLACKJACK_PEEK_CIRCUIT_FAMILY_TARGET.to_string(),
        circuit_id: BLACKJACK_PEEK_CIRCUIT_ID_TARGET.to_string(),
        verification_key_id: BLACKJACK_PEEK_VERIFICATION_KEY_ID_TARGET.to_string(),
        claim: target.claim.clone(),
        statement_hash: target.statement_hash.clone(),
        public_inputs_hash: target.public_inputs_hash.clone(),
        target_artifact_hash: target.artifact_hash.clone(),
        request_id: target.request_id.clone(),
        proof_id: proof_id.clone(),
        ..BlackjackProofBindingArtifact::default()
    })
}

pub fn validate_no_blackjack_external_prover_artifact(
    target: &BlackjackZkProofTargetArtifact,
    proof: &BlackjackExternalProverProofArtifact,
) -> anyhow::Result<()> {
    ensure!(
        proof.status == BLACKJACK_PEEK_PROOF_BINDING_STATUS_VERIFIED,
        "external blackjack prover status mismatch"
    );
    ensure!(
        proof.request_id == target.request_id,
        "external blackjack prover request id mismatch"
    );
    ensure!(
        proof.claim == target.claim,
        "external blackjack prover claim mismatch"
    );
    ensure!(
        proof.statement_hash == target.statement_hash,
        "external blackjack prover statement hash mismatch"
    );
    ensure!(
        proof.public_inputs_hash == target.public_inputs_hash,
        "external blackjack prover public inputs hash mismatch"
    );
    ensure!(
        proof.proof_system == target.proof_system,
        "external blackjack prover proof system mismatch"
    );
    ensure!(
        proof.circuit_family == target.circuit_family,
        "external blackjack prover circuit family mismatch"
    );
    ensure!(
        proof.circuit_id == target.circuit_id,
        "external blackjack prover circuit id mismatch"
    );
    ensure!(
        proof.verification_key_id == target.verification_key_id,
        "external blackjack prover verification key mismatch"
    );
    ensure!(
        !proof.backend_request_id.trim().is_empty(),
        "external blackjack prover backend request id is missing"
    );
    ensure!(
        !proof.proof_artifact_uri.trim().is_empty(),
        "external blackjack prover proof artifact uri is missing"
    );

    let artifact = proof
        .proof_artifact
        .as_object()
        .context("external blackjack prover proof artifact must be an object")?;
    let artifact_kind = artifact
        .get("artifact_kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact kind is missing")?;
    ensure!(
        artifact_kind == BLACKJACK_EXTERNAL_PROOF_ARTIFACT_KIND,
        "external blackjack prover proof artifact kind mismatch"
    );
    let artifact_request_id = artifact
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact request id is missing")?;
    ensure!(
        artifact_request_id == target.request_id,
        "external blackjack prover proof artifact request id mismatch"
    );
    let artifact_claim = artifact
        .get("claim")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact claim is missing")?;
    ensure!(
        artifact_claim == target.claim,
        "external blackjack prover proof artifact claim mismatch"
    );
    let artifact_statement_hash = artifact
        .get("statement_hash")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact statement hash is missing")?;
    ensure!(
        artifact_statement_hash == target.statement_hash,
        "external blackjack prover proof artifact statement hash mismatch"
    );
    let artifact_public_inputs_hash = artifact
        .get("public_inputs_hash")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact public inputs hash is missing")?;
    ensure!(
        artifact_public_inputs_hash == target.public_inputs_hash,
        "external blackjack prover proof artifact public inputs hash mismatch"
    );
    let artifact_proof_system = artifact
        .get("proof_system")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact proof system is missing")?;
    ensure!(
        artifact_proof_system == target.proof_system,
        "external blackjack prover proof artifact proof system mismatch"
    );
    let artifact_circuit_family = artifact
        .get("circuit_family")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact circuit family is missing")?;
    ensure!(
        artifact_circuit_family == target.circuit_family,
        "external blackjack prover proof artifact circuit family mismatch"
    );
    let artifact_circuit_id = artifact
        .get("circuit_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact circuit id is missing")?;
    ensure!(
        artifact_circuit_id == target.circuit_id,
        "external blackjack prover proof artifact circuit id mismatch"
    );
    let artifact_verification_key_id = artifact
        .get("verification_key_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact verification key id is missing")?;
    ensure!(
        artifact_verification_key_id == target.verification_key_id,
        "external blackjack prover proof artifact verification key mismatch"
    );
    let artifact_backend_request_id = artifact
        .get("backend_request_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("external blackjack prover proof artifact backend request id is missing")?;
    ensure!(
        artifact_backend_request_id == proof.backend_request_id,
        "external blackjack prover proof artifact backend request id mismatch"
    );
    ensure!(
        artifact.get("proof").is_some_and(|value| !value.is_null()),
        "external blackjack prover proof artifact proof payload is missing"
    );
    let proof_payload: BlackjackExternalZkProofPayload = serde_json::from_value(
        artifact
            .get("proof")
            .cloned()
            .context("external blackjack prover proof payload is missing")?,
    )
    .context("external blackjack prover proof payload must match the typed schema")?;
    validate_blackjack_external_zk_proof_payload(target, &proof_payload)?;
    Ok(())
}

pub fn validate_no_blackjack_private_witness(
    request: &BlackjackZkPeekProofRequest,
) -> anyhow::Result<()> {
    let target = &request.target;
    let witness = request
        .private_witness
        .as_ref()
        .context("zk proof request is missing private dealer peek witness")?;
    ensure!(
        (1..=13).contains(&witness.dealer_hole_rank),
        "dealer hole rank is outside blackjack rank range"
    );
    let dealer_upcard_rank = target
        .dealer_upcard_rank
        .context("zk target is missing dealer_upcard_rank")?;
    let hole_card_index = target
        .hole_card_index
        .context("zk target is missing hole_card_index")?;
    ensure!(
        witness.card.deck_index == hole_card_index,
        "private witness card index does not match target hole card index"
    );
    ensure!(
        !witness.card_salt.trim().is_empty(),
        "private witness card salt is missing"
    );
    ensure!(
        rank_from_card_id(witness.card.card_id) == witness.dealer_hole_rank,
        "private witness card id does not match dealer hole rank"
    );
    let dealer_blackjack = if dealer_upcard_rank == 1 {
        card_value(witness.dealer_hole_rank) == 10
    } else if card_value(dealer_upcard_rank) == 10 {
        witness.dealer_hole_rank == 1
    } else {
        false
    };
    let expected_claim = dealer_peek_statement_kind(
        true,
        dealer_upcard_rank,
        if dealer_blackjack {
            "dealer_blackjack"
        } else {
            "no_blackjack_seed_verified"
        },
    );
    ensure!(
        target.claim == expected_claim,
        "zk target dealer peek claim does not match witness"
    );
    let hole_value_class = dealer_hole_value_class(witness.dealer_hole_rank);
    let expected_hidden_value_class_commitment = poseidon_hex(&[
        felt_from_label("moros.blackjack.peek.hidden_class.v3"),
        felt_from_text_or_hex("blackjack.encrypted_deck_root", &target.encrypted_deck_root),
        Felt::from(hole_card_index as u64),
        Felt::from(dealer_upcard_rank),
        felt_from_text_or_hex("blackjack.hole_value_class", hole_value_class),
    ]);
    ensure!(
        target.hidden_value_class_commitment == expected_hidden_value_class_commitment,
        "hidden value class commitment mismatch"
    );
    let expected_witness_commitment = poseidon_hex(&[
        felt_from_label("moros.blackjack.peek.witness.v3"),
        felt_from_text_or_hex("blackjack.transcript_root", &request.transcript_root),
        Felt::from(dealer_upcard_rank),
        Felt::from(witness.dealer_hole_rank),
        Felt::from(hole_card_index as u64),
    ]);
    ensure!(
        target.witness_commitment == expected_witness_commitment,
        "dealer peek witness commitment mismatch"
    );
    let expected_hole_card_rank_commitment =
        compute_hole_card_rank_commitment_from_private_witness(
            &request.hand_id,
            &request.player,
            request.table_id,
            &request.transcript_root,
            target,
            witness,
        )?;
    ensure!(
        target.hole_card_rank_commitment == expected_hole_card_rank_commitment,
        "hidden rank commitment mismatch"
    );
    let dealer_entropy_commitment = if witness.server_seed_hash.is_empty() {
        hash_hex(format!(
            "moros:blackjack:dealer-entropy:v1:{}:{}:{}",
            request.hand_id, request.player, request.table_id
        ))
    } else {
        witness.server_seed_hash.clone()
    };
    let expected_card = expected_private_witness_card(
        &request.hand_id,
        request.table_id,
        &dealer_entropy_commitment,
        &witness.server_seed,
        &witness.client_seed,
        hole_card_index,
        witness.card.card_id,
    );
    ensure!(
        witness.card.commitment == expected_card.commitment
            && witness.card.ciphertext == expected_card.ciphertext
            && witness.card.nonce_commitment == expected_card.nonce_commitment
            && witness.card.reveal_key_commitment == expected_card.reveal_key_commitment,
        "private witness encrypted card envelope mismatch"
    );
    let expected_card_salt = onchain_card_salt(
        &request.hand_id,
        &request.player,
        request.table_id,
        &witness.server_seed,
        &witness.client_seed,
        hole_card_index,
    )?;
    ensure!(
        same_hex_string(&witness.card_salt, &expected_card_salt),
        "private witness onchain card salt mismatch"
    );
    ensure!(
        verify_onchain_card_private_opening(
            request.table_id,
            &request.player,
            witness.card.card_id,
            witness.card.deck_index,
            &witness.card_salt,
            &witness.opening,
            &request.transcript_root,
        ),
        "private witness onchain transcript opening mismatch"
    );
    Ok(())
}

pub fn verify_no_blackjack_proof_binding(
    target: &BlackjackZkProofTargetArtifact,
    binding: &BlackjackProofBindingArtifact,
) -> bool {
    if !target.available || !binding.available {
        return false;
    }
    binding.verifier_namespace == BLACKJACK_PEEK_VERIFIER_NAMESPACE
        && binding.verifier_kind == BLACKJACK_PEEK_VERIFIER_KIND_TARGET
        && binding.proof_system == BLACKJACK_PEEK_PROOF_SYSTEM_TARGET
        && binding.circuit_family == BLACKJACK_PEEK_CIRCUIT_FAMILY_TARGET
        && binding.circuit_id == BLACKJACK_PEEK_CIRCUIT_ID_TARGET
        && binding.verification_key_id == BLACKJACK_PEEK_VERIFICATION_KEY_ID_TARGET
        && binding.request_bound
        && binding.proof_verified
        && binding.claim == target.claim
        && binding.statement_hash == target.statement_hash
        && binding.public_inputs_hash == target.public_inputs_hash
        && binding.target_artifact_hash == target.artifact_hash
        && binding.request_id == target.request_id
        && binding.proof_id == compute_no_blackjack_proof_binding_id(&target.request_id)
}

fn verify_no_blackjack_statement_binding(artifact: &BlackjackFairnessArtifactView) -> bool {
    let proof = &artifact.dealer_peek.no_blackjack_proof;
    if !proof.available {
        return true;
    }
    let statement = &proof.statement;
    proof.statement_hash == compute_no_blackjack_statement_hash(statement)
        && statement.hand_id == artifact.hand_id
        && same_felt_hex(&statement.player, &artifact.player)
        && statement.table_id == artifact.table_id
        && statement.ruleset_hash == artifact.ruleset_hash
        && statement.deck_commitment_root == artifact.deck_commitment_root
        && statement.encrypted_deck_root == artifact.encrypted_deck_root
        && statement.dealer_upcard_rank == artifact.dealer_peek.upcard_rank
        && statement.hole_card_index == artifact.dealer_peek.hole_card_index
        && statement.statement_kind == artifact.dealer_peek.statement_kind
        && statement.statement_kind == proof.statement_kind
}

fn verify_no_blackjack_public_inputs_binding(artifact: &BlackjackFairnessArtifactView) -> bool {
    let proof = &artifact.dealer_peek.no_blackjack_proof;
    if !proof.available {
        return true;
    }
    let expected = compute_peek_public_inputs_hash(
        &proof.statement_hash,
        &proof.statement_kind,
        &proof.target_proof_kind,
    );
    expected == proof.public_inputs_hash && expected == artifact.dealer_peek.public_inputs_hash
}

fn verify_no_blackjack_artifact_consistency(artifact: &BlackjackFairnessArtifactView) -> bool {
    let proof = &artifact.dealer_peek.no_blackjack_proof;
    if !proof.available {
        return true;
    }
    proof.verifier_namespace == BLACKJACK_PEEK_VERIFIER_NAMESPACE
        && proof.claim == artifact.dealer_peek.statement_kind
        && proof.statement.statement_kind == artifact.dealer_peek.statement_kind
        && proof.current_proof_mode == artifact.dealer_peek.proof_mode
        && proof.target_proof_mode == artifact.dealer_peek.target_proof_mode
        && proof.target_proof_kind == artifact.dealer_peek.target_proof_kind
        && proof.hidden_value_class_commitment == artifact.dealer_peek.hidden_value_class_commitment
        && proof.witness_commitment == artifact.dealer_peek.witness_commitment
        && proof.hole_card_rank_commitment == artifact.dealer_peek.hole_card_rank_commitment
        && proof.receipt.proof_kind == artifact.dealer_peek.receipt.proof_kind
        && proof.receipt.receipt == artifact.dealer_peek.receipt.receipt
        && proof.receipt.verified == artifact.dealer_peek.receipt.verified
        && proof.opening.leaf_hash == artifact.dealer_peek.opening.leaf_hash
        && proof.opening.leaf_index == artifact.dealer_peek.opening.leaf_index
        && proof.opening.root == artifact.dealer_peek.opening.root
        && proof.opening.siblings == artifact.dealer_peek.opening.siblings
        && proof.opening.verified == artifact.dealer_peek.opening.verified
}

fn verify_no_blackjack_zk_target_binding(artifact: &BlackjackFairnessArtifactView) -> bool {
    let proof = &artifact.dealer_peek.no_blackjack_proof;
    if !proof.available {
        return true;
    }
    let target = &proof.zk_proof_target;
    if !target.available {
        return false;
    }
    let Some(dealer_upcard_rank) = artifact.dealer_peek.upcard_rank else {
        return false;
    };
    let Some(hole_card_index) = artifact.dealer_peek.hole_card_index else {
        return false;
    };
    let expected_artifact_hash = compute_no_blackjack_zk_target_artifact_hash(
        &proof.claim,
        &proof.statement_hash,
        &proof.public_inputs_hash,
        &artifact.encrypted_deck_root,
        dealer_upcard_rank,
        hole_card_index,
        &proof.hidden_value_class_commitment,
        &proof.witness_commitment,
        &proof.hole_card_rank_commitment,
    );
    target.verifier_namespace == BLACKJACK_PEEK_VERIFIER_NAMESPACE
        && target.verifier_kind == BLACKJACK_PEEK_VERIFIER_KIND_TARGET
        && target.proof_system == BLACKJACK_PEEK_PROOF_SYSTEM_TARGET
        && target.circuit_family == BLACKJACK_PEEK_CIRCUIT_FAMILY_TARGET
        && target.circuit_id == BLACKJACK_PEEK_CIRCUIT_ID_TARGET
        && target.verification_key_id == BLACKJACK_PEEK_VERIFICATION_KEY_ID_TARGET
        && target.claim == proof.claim
        && target.statement_hash == proof.statement_hash
        && target.public_inputs_hash == proof.public_inputs_hash
        && target.encrypted_deck_root == artifact.encrypted_deck_root
        && target.dealer_upcard_rank == artifact.dealer_peek.upcard_rank
        && target.hole_card_index == artifact.dealer_peek.hole_card_index
        && target.hidden_value_class_commitment == proof.hidden_value_class_commitment
        && target.witness_commitment == proof.witness_commitment
        && target.hole_card_rank_commitment == proof.hole_card_rank_commitment
        && target.artifact_hash == expected_artifact_hash
        && target.request_id == compute_no_blackjack_zk_target_request_id(&expected_artifact_hash)
}

fn verify_no_blackjack_proof_binding_for_artifact(
    artifact: &BlackjackFairnessArtifactView,
) -> bool {
    let proof = &artifact.dealer_peek.no_blackjack_proof;
    if !proof.available {
        return true;
    }
    verify_no_blackjack_proof_binding(&proof.zk_proof_target, &proof.proof_binding)
}

pub fn opening_plan(snapshot: &BlackjackHandSnapshot) -> anyhow::Result<BlackjackOpenPlan> {
    ensure!(
        snapshot.seats.len() == 1,
        "opening snapshot must have one seat"
    );
    ensure!(
        snapshot.seats[0].cards.len() >= 2 && snapshot.dealer.cards.len() >= 2,
        "opening snapshot is missing initial cards"
    );

    Ok(BlackjackOpenPlan {
        dealer_upcard: snapshot.dealer.cards[0].rank,
        dealer_upcard_proof: onchain_card_reveal_proof(snapshot, 1)?,
        player_first_card: snapshot.seats[0].cards[0].rank,
        player_first_card_proof: onchain_card_reveal_proof(snapshot, 0)?,
        player_second_card: snapshot.seats[0].cards[1].rank,
        player_second_card_proof: onchain_card_reveal_proof(snapshot, 2)?,
        dealer_reveals: if snapshot.phase == "settled" {
            snapshot
                .dealer
                .cards
                .iter()
                .skip(1)
                .map(|card| card.rank)
                .collect()
        } else {
            Vec::new()
        },
        dealer_reveal_proofs: if snapshot.phase == "settled" {
            snapshot
                .dealer
                .cards
                .iter()
                .enumerate()
                .skip(1)
                .map(|(index, _)| {
                    onchain_card_reveal_proof_for_target(snapshot, &format!("dealer:{index}"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        } else {
            Vec::new()
        },
        should_finalize: snapshot.phase == "settled",
    })
}

pub fn plan_action_submission(
    snapshot: &BlackjackHandSnapshot,
    action: &str,
) -> anyhow::Result<(BlackjackHandSnapshot, BlackjackActionPlan)> {
    let mut next = snapshot.clone();
    apply_action_to_snapshot(&mut next, action)?;

    let seat_index = snapshot.active_seat;
    let action = action.to_ascii_lowercase();
    let player_draws = match action.as_str() {
        "hit" | "double" => {
            let seat = next
                .seats
                .iter()
                .find(|seat| seat.seat_index == seat_index)
                .context("seat missing after action")?;
            vec![seat.cards.last().context("seat missing drawn card")?.rank]
        }
        "split" => {
            ensure!(
                next.seats.len() == snapshot.seats.len() + 1,
                "split should create one additional seat"
            );
            let left = next
                .seats
                .iter()
                .find(|seat| seat.seat_index == seat_index)
                .context("left split seat missing after action")?;
            let right = next
                .seats
                .iter()
                .find(|seat| seat.seat_index == seat_index + 1)
                .context("right split seat missing after action")?;
            vec![
                left.cards
                    .last()
                    .context("left split seat missing drawn card")?
                    .rank,
                right
                    .cards
                    .last()
                    .context("right split seat missing drawn card")?
                    .rank,
            ]
        }
        "take_insurance" | "decline_insurance" | "stand" => Vec::new(),
        other => bail!("unsupported action: {other}"),
    };

    let dealer_reveals = if next.phase == "settled" {
        next.dealer
            .cards
            .iter()
            .skip(1)
            .map(|card| card.rank)
            .collect()
    } else {
        Vec::new()
    };
    let new_reveals = next
        .transcript_artifact
        .reveals
        .iter()
        .skip(snapshot.transcript_artifact.reveals.len())
        .collect::<Vec<_>>();
    let player_draw_proofs = new_reveals
        .iter()
        .filter(|reveal| reveal.stage == "player_action")
        .map(|reveal| onchain_card_reveal_proof(&next, reveal.deck_index))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let dealer_reveal_proofs = new_reveals
        .iter()
        .filter(|reveal| reveal.stage == "dealer_resolution")
        .map(|reveal| onchain_card_reveal_proof(&next, reveal.deck_index))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok((
        next.clone(),
        BlackjackActionPlan {
            action,
            seat_index,
            player_draws,
            player_draw_proofs,
            dealer_reveals,
            dealer_reveal_proofs,
            should_finalize: next.phase == "settled" && next.status == "settled",
        },
    ))
}

pub fn plan_timeout_submission(
    snapshot: &BlackjackHandSnapshot,
    action: &str,
) -> anyhow::Result<(BlackjackHandSnapshot, BlackjackTimeoutPlan)> {
    let mut next = snapshot.clone();
    let action = action.to_ascii_lowercase();

    match action.as_str() {
        "force_insurance_decline" => {
            force_insurance_decline_in_snapshot(&mut next)?;
            Ok((
                next,
                BlackjackTimeoutPlan {
                    action,
                    should_release_reservation: false,
                },
            ))
        }
        "force_stand" => {
            force_stand_in_snapshot(&mut next)?;
            Ok((
                next,
                BlackjackTimeoutPlan {
                    action,
                    should_release_reservation: false,
                },
            ))
        }
        "void" | "void_expired_hand" => {
            void_expired_hand_in_snapshot(&mut next)?;
            Ok((
                next,
                BlackjackTimeoutPlan {
                    action: "void_expired_hand".to_string(),
                    should_release_reservation: true,
                },
            ))
        }
        other => bail!("unsupported timeout action: {other}"),
    }
}

pub fn reconcile_view_with_chain(
    snapshot: &BlackjackHandSnapshot,
    chain: &BlackjackChainHand,
) -> anyhow::Result<BlackjackHandView> {
    ensure!(
        same_felt_hex(&snapshot.player, &chain.player)
            && snapshot.table_id == chain.table_id
            && same_felt_hex(&snapshot.transcript_root, &chain.transcript_root),
        "snapshot does not match chain hand"
    );
    verify_chain_matches_snapshot(snapshot, chain)?;

    let dealer_cards = if chain.dealer_cards.len() == 1 && snapshot.dealer.cards.len() >= 2 {
        vec![
            BlackjackCardView {
                label: rank_label(chain.dealer_cards[0]),
                revealed: true,
            },
            BlackjackCardView {
                label: "◆".to_string(),
                revealed: false,
            },
        ]
    } else {
        chain
            .dealer_cards
            .iter()
            .map(|rank| BlackjackCardView {
                label: rank_label(*rank),
                revealed: true,
            })
            .collect::<Vec<_>>()
    };

    let dealer_math = if chain.dealer_cards.is_empty() {
        None
    } else {
        Some(hand_math(&chain.dealer_cards))
    };

    let seats = chain
        .seats
        .iter()
        .map(|seat| {
            let math = hand_math(&seat.cards);
            BlackjackSeatView {
                seat_index: seat.seat_index,
                wager: format_strk_amount(&seat.wager),
                status: seat.status.clone(),
                outcome: seat.outcome.clone(),
                payout: format_strk_amount(&seat.payout),
                doubled: seat.doubled,
                total: math.total,
                soft: math.soft,
                is_blackjack: is_blackjack(seat.cards.len(), math.total, chain.split_count),
                active: chain.phase == "player_turn"
                    && chain.active_seat == seat.seat_index
                    && seat.status == "active",
                can_double: chain.phase == "player_turn"
                    && chain.active_seat == seat.seat_index
                    && seat.status == "active"
                    && seat.cards.len() == 2,
                can_split: chain.phase == "player_turn"
                    && chain.active_seat == seat.seat_index
                    && chain.seat_count < MAX_HANDS_PER_ROUND as u8
                    && seat.status == "active"
                    && seat.cards.len() == 2
                    && card_value(seat.cards[0]) == card_value(seat.cards[1]),
                cards: seat
                    .cards
                    .iter()
                    .map(|rank| BlackjackCardView {
                        label: rank_label(*rank),
                        revealed: true,
                    })
                    .collect(),
            }
        })
        .collect::<Vec<_>>();

    let mut allowed_actions = Vec::new();
    if chain.phase == "insurance" {
        if snapshot.insurance.offered && !snapshot.insurance.settled {
            allowed_actions.push("decline_insurance".to_string());
            if snapshot.insurance.supported && snapshot.insurance.max_wager != "0" {
                allowed_actions.insert(0, "take_insurance".to_string());
            }
        }
    } else if chain.phase == "player_turn" {
        if let Some(active_seat) = chain
            .seats
            .iter()
            .find(|seat| seat.seat_index == chain.active_seat && seat.status == "active")
        {
            allowed_actions.push("hit".to_string());
            allowed_actions.push("stand".to_string());
            if active_seat.cards.len() == 2 {
                allowed_actions.push("double".to_string());
                if chain.seat_count < MAX_HANDS_PER_ROUND as u8
                    && card_value(active_seat.cards[0]) == card_value(active_seat.cards[1])
                {
                    allowed_actions.push("split".to_string());
                }
            }
        }
    }

    Ok(BlackjackHandView {
        hand_id: snapshot.hand_id.clone(),
        player: chain.player.clone(),
        table_id: chain.table_id,
        wager: format_strk_amount(&chain.wager),
        status: chain.status.clone(),
        phase: chain.phase.clone(),
        transcript_root: chain.transcript_root.clone(),
        server_seed_hash: if snapshot.server_seed_hash.is_empty() {
            chain.transcript_root.clone()
        } else {
            snapshot.server_seed_hash.clone()
        },
        server_seed: if (chain.phase == "settled" || chain.status == "settled")
            && !snapshot.server_seed.is_empty()
        {
            Some(snapshot.server_seed.clone())
        } else {
            None
        },
        client_seed: snapshot.client_seed.clone(),
        active_seat: chain.active_seat,
        seat_count: chain.seat_count,
        dealer_upcard: chain.dealer_cards.first().copied(),
        total_payout: format_strk_amount(&chain.total_payout),
        allowed_actions,
        proof_verified: !snapshot.server_seed.is_empty()
            && (chain.phase == "settled" || chain.status == "settled")
            && fairness_artifact_view(snapshot).audit.passed,
        insurance: snapshot.insurance.clone(),
        fairness: BlackjackFairnessView {
            protocol_mode: if snapshot.transcript_artifact.protocol_mode.is_empty() {
                BLACKJACK_PROTOCOL_MODE_CURRENT.to_string()
            } else {
                snapshot.transcript_artifact.protocol_mode.clone()
            },
            target_protocol_mode: if snapshot.transcript_artifact.target_protocol_mode.is_empty() {
                BLACKJACK_PROTOCOL_MODE_TARGET.to_string()
            } else {
                snapshot.transcript_artifact.target_protocol_mode.clone()
            },
            encryption_scheme: if snapshot.transcript_artifact.encryption_scheme.is_empty() {
                BLACKJACK_ENCRYPTION_SCHEME_CURRENT.to_string()
            } else {
                snapshot.transcript_artifact.encryption_scheme.clone()
            },
            target_encryption_scheme: if snapshot
                .transcript_artifact
                .target_encryption_scheme
                .is_empty()
            {
                BLACKJACK_ENCRYPTION_SCHEME_TARGET.to_string()
            } else {
                snapshot
                    .transcript_artifact
                    .target_encryption_scheme
                    .clone()
            },
            deck_commitment_root: snapshot.transcript_artifact.deck_commitment_root.clone(),
            reveal_count: snapshot.transcript_artifact.reveals.len() as u16,
            dealer_peek_required: snapshot.transcript_artifact.dealer_peek.required,
            dealer_peek_status: snapshot.transcript_artifact.dealer_peek.outcome.clone(),
            insurance_offered: snapshot.insurance.offered,
            insurance_status: snapshot.insurance.outcome.clone(),
        },
        dealer: BlackjackDealerView {
            cards: dealer_cards,
            total: dealer_math.map(|value| value.total),
            soft: dealer_math.map(|value| value.soft),
            hidden_cards: if chain.dealer_cards.len() == 1 && snapshot.dealer.cards.len() >= 2 {
                1
            } else {
                0
            },
        },
        seats,
        action_log: snapshot.action_log.clone(),
    })
}

fn verify_chain_matches_snapshot(
    snapshot: &BlackjackHandSnapshot,
    chain: &BlackjackChainHand,
) -> anyhow::Result<()> {
    ensure!(
        chain.dealer_cards.len() <= snapshot.dealer.cards.len(),
        "dealer reveal count exceeds committed transcript"
    );
    for (index, rank) in chain.dealer_cards.iter().enumerate() {
        let committed = snapshot
            .dealer
            .cards
            .get(index)
            .context("dealer transcript missing committed card")?;
        ensure!(
            committed.rank == *rank,
            "dealer reveal does not match committed transcript"
        );
    }

    ensure!(
        chain.seats.len() == snapshot.seats.len(),
        "seat count does not match committed transcript"
    );

    for chain_seat in &chain.seats {
        let snapshot_seat = snapshot
            .seats
            .iter()
            .find(|seat| seat.seat_index == chain_seat.seat_index)
            .context("chain seat missing from committed transcript")?;
        ensure!(
            chain_seat.cards.len() == snapshot_seat.cards.len(),
            "seat card count does not match committed transcript"
        );
        for (index, rank) in chain_seat.cards.iter().enumerate() {
            let committed = snapshot_seat
                .cards
                .get(index)
                .context("seat transcript missing committed card")?;
            ensure!(
                committed.rank == *rank,
                "seat card does not match committed transcript"
            );
        }
    }

    Ok(())
}

pub fn apply_action_to_snapshot(
    snapshot: &mut BlackjackHandSnapshot,
    action: &str,
) -> anyhow::Result<()> {
    let action = action.to_ascii_lowercase();
    ensure!(
        allowed_actions(snapshot)
            .iter()
            .any(|allowed| allowed == &action),
        "action is not allowed in the current hand state"
    );

    if snapshot.phase == "insurance" {
        match action.as_str() {
            "take_insurance" => {
                take_insurance(snapshot)?;
                return Ok(());
            }
            "decline_insurance" => {
                decline_insurance(snapshot)?;
                return Ok(());
            }
            _ => bail!("unsupported insurance action: {action}"),
        }
    }

    let seat_index = snapshot.active_seat as usize;
    match action.as_str() {
        "hit" => {
            let card = draw_card(snapshot)?;
            snapshot.seats[seat_index]
                .cards
                .push(BlackjackCardSnapshot {
                    rank: card.rank,
                    revealed: true,
                });
            let card_position = snapshot.seats[seat_index].cards.len() - 1;
            record_reveal(
                snapshot,
                card.deck_index,
                card.card_id,
                card.rank,
                "player_action",
                &format!("player:{}:{card_position}", snapshot.active_seat),
            );
            let math = hand_math_for_seat(&snapshot.seats[seat_index]);
            if math.total > 21 {
                snapshot.seats[seat_index].status = "busted".to_string();
            } else if math.total == 21 {
                snapshot.seats[seat_index].status = "standing".to_string();
            }
            snapshot.action_log.push(BlackjackActionLogEntry {
                action: "hit".to_string(),
                seat_index: Some(snapshot.active_seat),
                detail: format!(
                    "seat {} drew {}",
                    snapshot.active_seat,
                    rank_label(card.rank)
                ),
            });
        }
        "stand" => {
            snapshot.seats[seat_index].status = "standing".to_string();
            snapshot.action_log.push(BlackjackActionLogEntry {
                action: "stand".to_string(),
                seat_index: Some(snapshot.active_seat),
                detail: format!("seat {} stood", snapshot.active_seat),
            });
        }
        "double" => {
            ensure!(
                can_double(snapshot, &snapshot.seats[seat_index]),
                "double is not allowed"
            );
            let card = draw_card(snapshot)?;
            let next_wager = parse_amount(&snapshot.seats[seat_index].wager)? * 2;
            snapshot.seats[seat_index].wager = next_wager.to_string();
            snapshot.seats[seat_index].doubled = true;
            snapshot.seats[seat_index]
                .cards
                .push(BlackjackCardSnapshot {
                    rank: card.rank,
                    revealed: true,
                });
            let card_position = snapshot.seats[seat_index].cards.len() - 1;
            record_reveal(
                snapshot,
                card.deck_index,
                card.card_id,
                card.rank,
                "player_action",
                &format!("player:{}:{card_position}", snapshot.active_seat),
            );
            let math = hand_math_for_seat(&snapshot.seats[seat_index]);
            snapshot.seats[seat_index].status = if math.total > 21 {
                "busted".to_string()
            } else {
                "standing".to_string()
            };
            snapshot.action_log.push(BlackjackActionLogEntry {
                action: "double".to_string(),
                seat_index: Some(snapshot.active_seat),
                detail: format!(
                    "seat {} doubled and drew {}",
                    snapshot.active_seat,
                    rank_label(card.rank)
                ),
            });
        }
        "split" => {
            ensure!(
                can_split(snapshot, &snapshot.seats[seat_index]),
                "split is not allowed"
            );
            let original = snapshot.seats.remove(seat_index);
            let left_first = original.cards[0].rank;
            let right_first = original.cards[1].rank;
            let left_draw = draw_card(snapshot)?;
            let right_draw = draw_card(snapshot)?;
            let split_depth = original.split_depth + 1;
            let split_aces = left_first == 1 && right_first == 1;

            for seat in snapshot.seats.iter_mut().skip(seat_index) {
                seat.seat_index += 1;
            }

            let mut left = BlackjackSeatSnapshot {
                seat_index: seat_index as u8,
                wager: original.wager.clone(),
                cards: vec![
                    BlackjackCardSnapshot {
                        rank: left_first,
                        revealed: true,
                    },
                    BlackjackCardSnapshot {
                        rank: left_draw.rank,
                        revealed: true,
                    },
                ],
                status: "active".to_string(),
                outcome: None,
                payout: "0".to_string(),
                doubled: false,
                split_depth,
                split_aces,
            };
            let mut right = BlackjackSeatSnapshot {
                seat_index: seat_index as u8 + 1,
                wager: original.wager,
                cards: vec![
                    BlackjackCardSnapshot {
                        rank: right_first,
                        revealed: true,
                    },
                    BlackjackCardSnapshot {
                        rank: right_draw.rank,
                        revealed: true,
                    },
                ],
                status: "active".to_string(),
                outcome: None,
                payout: "0".to_string(),
                doubled: false,
                split_depth,
                split_aces,
            };
            if split_aces || hand_math_for_seat(&left).total == 21 {
                left.status = "standing".to_string();
            }
            if split_aces || hand_math_for_seat(&right).total == 21 {
                right.status = "standing".to_string();
            }
            snapshot.seats.insert(seat_index, left);
            snapshot.seats.insert(seat_index + 1, right);
            record_reveal(
                snapshot,
                left_draw.deck_index,
                left_draw.card_id,
                left_draw.rank,
                "player_action",
                &format!("player:{}:1", seat_index),
            );
            record_reveal(
                snapshot,
                right_draw.deck_index,
                right_draw.card_id,
                right_draw.rank,
                "player_action",
                &format!("player:{}:1", seat_index + 1),
            );
            snapshot.seat_count = snapshot.seats.len() as u8;
            snapshot.split_count += 1;
            snapshot.action_log.push(BlackjackActionLogEntry {
                action: "split".to_string(),
                seat_index: Some(snapshot.active_seat),
                detail: format!(
                    "seat {} split into {} + {}",
                    snapshot.active_seat,
                    rank_label(left_draw.rank),
                    rank_label(right_draw.rank)
                ),
            });
        }
        "surrender" => {
            ensure!(
                can_surrender(snapshot, &snapshot.seats[seat_index]),
                "surrender is not allowed"
            );
            let payout = parse_amount(&snapshot.seats[seat_index].wager)? / 2;
            snapshot.seats[seat_index].status = "settled".to_string();
            snapshot.seats[seat_index].outcome = Some("surrender".to_string());
            snapshot.seats[seat_index].payout = payout.to_string();
            snapshot.status = "settled".to_string();
            snapshot.phase = "settled".to_string();
            snapshot.total_payout = payout.to_string();
            snapshot.action_count += 1;
            snapshot.action_log.push(BlackjackActionLogEntry {
                action: "surrender".to_string(),
                seat_index: Some(snapshot.active_seat),
                detail: format!(
                    "seat {} surrendered for half wager back",
                    snapshot.active_seat
                ),
            });
            return Ok(());
        }
        other => bail!("unsupported action: {other}"),
    }

    snapshot.action_count += 1;
    refresh_player_phase(snapshot);
    if snapshot.phase == "dealer_turn" {
        resolve_dealer_phase(snapshot)?;
    }
    Ok(())
}

pub fn allowed_actions(snapshot: &BlackjackHandSnapshot) -> Vec<String> {
    if snapshot.phase == "insurance" {
        if !snapshot.insurance.offered || snapshot.insurance.settled {
            return Vec::new();
        }

        let mut actions = vec!["decline_insurance".to_string()];
        if snapshot.insurance.supported && snapshot.insurance.max_wager != "0" {
            actions.insert(0, "take_insurance".to_string());
        }
        return actions;
    }

    if snapshot.phase != "player_turn" {
        return Vec::new();
    }
    let Some(seat) = snapshot
        .seats
        .iter()
        .find(|seat| seat.seat_index == snapshot.active_seat && seat.status == "active")
    else {
        return Vec::new();
    };

    let mut actions = vec!["hit".to_string(), "stand".to_string()];
    if can_double(snapshot, seat) {
        actions.push("double".to_string());
    }
    if can_split(snapshot, seat) {
        actions.push("split".to_string());
    }
    if can_surrender(snapshot, seat) {
        actions.push("surrender".to_string());
    }
    actions
}

fn refresh_player_phase(snapshot: &mut BlackjackHandSnapshot) {
    if let Some(next_seat) = snapshot
        .seats
        .iter()
        .find(|seat| seat.status == "active")
        .map(|seat| seat.seat_index)
    {
        snapshot.active_seat = next_seat;
        snapshot.status = "active".to_string();
        snapshot.phase = "player_turn".to_string();
    } else if snapshot.status != "settled" {
        snapshot.status = "awaiting_dealer".to_string();
        snapshot.phase = "dealer_turn".to_string();
    }
    snapshot.seat_count = snapshot.seats.len() as u8;
}

fn take_insurance(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    ensure!(
        snapshot.phase == "insurance" && snapshot.insurance.offered && !snapshot.insurance.settled,
        "insurance is not available in the current hand state"
    );
    let insurance_wager = parse_amount(&snapshot.insurance.max_wager)?;
    ensure!(insurance_wager > 0, "insurance max wager is zero");
    snapshot.insurance.taken = true;
    snapshot.insurance.wager = insurance_wager.to_string();
    snapshot.insurance.settled = true;
    snapshot.insurance.outcome =
        if snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack" {
            "dealer_blackjack".to_string()
        } else {
            "lost".to_string()
        };
    snapshot.transcript_artifact.insurance = snapshot.insurance.clone();
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "take_insurance".to_string(),
        seat_index: None,
        detail: format!("insurance taken at {}", snapshot.insurance.wager),
    });
    advance_after_insurance_decision(snapshot)?;
    Ok(())
}

fn decline_insurance(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    ensure!(
        snapshot.phase == "insurance" && snapshot.insurance.offered && !snapshot.insurance.settled,
        "insurance is not available in the current hand state"
    );
    snapshot.insurance.taken = false;
    snapshot.insurance.wager = "0".to_string();
    snapshot.insurance.settled = true;
    snapshot.insurance.outcome =
        if snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack" {
            "dealer_blackjack".to_string()
        } else {
            "declined".to_string()
        };
    snapshot.transcript_artifact.insurance = snapshot.insurance.clone();
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "decline_insurance".to_string(),
        seat_index: None,
        detail: "insurance declined".to_string(),
    });
    advance_after_insurance_decision(snapshot)?;
    Ok(())
}

fn force_insurance_decline_in_snapshot(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    ensure!(
        snapshot.phase == "insurance" && snapshot.insurance.offered && !snapshot.insurance.settled,
        "insurance is not available in the current hand state"
    );
    snapshot.insurance.taken = false;
    snapshot.insurance.wager = "0".to_string();
    snapshot.insurance.settled = true;
    snapshot.insurance.outcome =
        if snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack" {
            "dealer_blackjack".to_string()
        } else {
            "declined".to_string()
        };
    snapshot.transcript_artifact.insurance = snapshot.insurance.clone();
    snapshot.action_count += 1;
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "force_insurance_decline".to_string(),
        seat_index: None,
        detail: "expired insurance prompt was declined by timeout".to_string(),
    });

    let dealer_blackjack = snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack";
    let player_blackjack = snapshot
        .seats
        .first()
        .map(|seat| {
            let math = hand_math_for_seat(seat);
            is_blackjack(seat.cards.len(), math.total, seat.split_depth)
        })
        .unwrap_or(false);
    if dealer_blackjack || player_blackjack {
        snapshot.status = "awaiting_dealer".to_string();
        snapshot.phase = "dealer_turn".to_string();
        snapshot.active_seat = 0;
    } else {
        refresh_player_phase(snapshot);
    }
    Ok(())
}

fn force_stand_in_snapshot(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    ensure!(
        snapshot.phase == "player_turn" && snapshot.status == "active",
        "hand is not in an active player decision state"
    );
    let seat_index = usize::from(snapshot.active_seat);
    let Some(seat) = snapshot.seats.get_mut(seat_index) else {
        bail!("active seat is missing")
    };
    ensure!(seat.status == "active", "active seat is not playable");
    seat.status = "standing".to_string();
    snapshot.action_count += 1;
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "force_stand".to_string(),
        seat_index: Some(snapshot.active_seat),
        detail: format!("seat {} timed out and stood", snapshot.active_seat),
    });
    refresh_player_phase(snapshot);
    Ok(())
}

fn void_expired_hand_in_snapshot(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    ensure!(
        snapshot.phase == "dealer_turn" || snapshot.status == "awaiting_dealer",
        "only dealer-reveal hands can be voided"
    );
    snapshot.status = "voided".to_string();
    snapshot.phase = "voided".to_string();
    snapshot.total_payout = "0".to_string();
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "void_expired_hand".to_string(),
        seat_index: None,
        detail: "expired dealer reveal was voided and reserved wager was refunded".to_string(),
    });
    Ok(())
}

fn advance_after_insurance_decision(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    let dealer_blackjack = snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack";
    let player_blackjack = snapshot
        .seats
        .first()
        .map(|seat| {
            let math = hand_math_for_seat(seat);
            is_blackjack(seat.cards.len(), math.total, seat.split_depth)
        })
        .unwrap_or(false);

    if dealer_blackjack || player_blackjack {
        resolve_dealer_phase(snapshot)?;
    } else {
        refresh_player_phase(snapshot);
    }
    Ok(())
}

fn resolve_dealer_phase(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    let mut pending_reveals = Vec::new();
    for (index, card) in snapshot.dealer.cards.iter_mut().enumerate() {
        if !card.revealed {
            card.revealed = true;
            let deck_index = if index == 1 {
                snapshot.transcript_artifact.hole_card_index
            } else {
                index + 1
            };
            let card_id = snapshot
                .transcript_artifact
                .cards
                .get(deck_index)
                .map(|card| card.card_id)
                .unwrap_or_default();
            pending_reveals.push((index, deck_index, card_id, card.rank));
        }
    }
    for (dealer_index, deck_index, card_id, rank) in pending_reveals {
        record_reveal(
            snapshot,
            deck_index,
            card_id,
            rank,
            "dealer_resolution",
            &format!("dealer:{dealer_index}"),
        );
    }
    while should_dealer_hit(&snapshot.dealer.cards) {
        let card = draw_card(snapshot)?;
        snapshot.dealer.cards.push(BlackjackCardSnapshot {
            rank: card.rank,
            revealed: true,
        });
        let dealer_index = snapshot.dealer.cards.len() - 1;
        record_reveal(
            snapshot,
            card.deck_index,
            card.card_id,
            card.rank,
            "dealer_resolution",
            &format!("dealer:{dealer_index}"),
        );
    }

    let dealer_ranks = snapshot
        .dealer
        .cards
        .iter()
        .map(|card| card.rank)
        .collect::<Vec<_>>();
    let dealer_math = hand_math(&dealer_ranks);
    let dealer_blackjack = dealer_ranks.len() == 2 && dealer_math.total == 21;
    let dealer_busted = dealer_math.total > 21;
    settle_insurance_state(snapshot, dealer_blackjack);
    let mut total_payout = 0_u128;

    for seat in &mut snapshot.seats {
        let (outcome, payout) = settle_seat(
            seat,
            dealer_math.total,
            dealer_blackjack,
            dealer_busted,
            seat.split_depth,
        )?;
        seat.status = "settled".to_string();
        seat.outcome = Some(outcome.to_string());
        seat.payout = payout.to_string();
        total_payout += payout;
    }

    total_payout += insurance_payout(snapshot)?;

    snapshot.status = "settled".to_string();
    snapshot.phase = "settled".to_string();
    snapshot.total_payout = total_payout.to_string();
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "dealer_resolve".to_string(),
        seat_index: None,
        detail: format!("dealer settled the hand at {}", dealer_math.total),
    });
    Ok(())
}

fn settle_seat(
    seat: &BlackjackSeatSnapshot,
    dealer_total: u8,
    dealer_blackjack: bool,
    dealer_busted: bool,
    split_depth: u8,
) -> anyhow::Result<(&'static str, u128)> {
    let wager = parse_amount(&seat.wager)?;
    let math = hand_math_for_seat(seat);
    if math.total > 21 {
        return Ok(("loss", 0));
    }

    let player_blackjack = is_blackjack(seat.cards.len(), math.total, split_depth);
    let outcome = if dealer_busted {
        if player_blackjack { "blackjack" } else { "win" }
    } else if dealer_blackjack {
        if player_blackjack { "push" } else { "loss" }
    } else if math.total > dealer_total {
        if player_blackjack { "blackjack" } else { "win" }
    } else if math.total < dealer_total {
        "loss"
    } else {
        "push"
    };

    let payout = match outcome {
        "loss" => 0,
        "push" => wager,
        "win" => wager * 2,
        "blackjack" => (wager * 5) / 2,
        _ => bail!("invalid outcome"),
    };
    Ok((outcome, payout))
}

fn can_double(snapshot: &BlackjackHandSnapshot, seat: &BlackjackSeatSnapshot) -> bool {
    snapshot.phase == "player_turn"
        && seat.status == "active"
        && seat.cards.len() == 2
        && !seat.split_aces
}

fn can_split(snapshot: &BlackjackHandSnapshot, seat: &BlackjackSeatSnapshot) -> bool {
    snapshot.phase == "player_turn"
        && snapshot.seats.len() < MAX_HANDS_PER_ROUND
        && seat.status == "active"
        && seat.cards.len() == 2
        && !seat.split_aces
        && card_value(seat.cards[0].rank) == card_value(seat.cards[1].rank)
}

fn can_surrender(snapshot: &BlackjackHandSnapshot, seat: &BlackjackSeatSnapshot) -> bool {
    snapshot.phase == "player_turn"
        && snapshot.seats.len() == 1
        && snapshot.active_seat == seat.seat_index
        && snapshot.split_count == 0
        && snapshot.action_count == 0
        && seat.status == "active"
        && seat.cards.len() == 2
}

fn hand_math_for_seat(seat: &BlackjackSeatSnapshot) -> HandMath {
    let ranks = seat.cards.iter().map(|card| card.rank).collect::<Vec<_>>();
    hand_math(&ranks)
}

fn hand_math(ranks: &[u8]) -> HandMath {
    let mut hard_total = 0_u8;
    let mut aces = 0_u8;
    for rank in ranks {
        hard_total = hard_total.saturating_add(card_value(*rank));
        if *rank == 1 {
            aces = aces.saturating_add(1);
        }
    }
    let soft = aces > 0 && hard_total + 10 <= 21;
    let total = if soft { hard_total + 10 } else { hard_total };
    HandMath { total, soft }
}

fn should_dealer_hit(cards: &[BlackjackCardSnapshot]) -> bool {
    let ranks = cards.iter().map(|card| card.rank).collect::<Vec<_>>();
    let math = hand_math(&ranks);
    math.total < DEALER_STAND_TOTAL
}

fn is_blackjack(card_count: usize, total: u8, split_depth: u8) -> bool {
    split_depth == 0 && card_count == 2 && total == 21
}

fn visible_ranks(cards: &[BlackjackCardSnapshot]) -> Vec<u8> {
    cards
        .iter()
        .filter(|card| card.revealed)
        .map(|card| card.rank)
        .collect()
}

fn rank_from_card_id(card_id: u16) -> u8 {
    ((card_id % 13) as u8) + 1
}

fn draw_card(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<DrawnCard> {
    let deck_index = snapshot.next_card_index;
    let Some(card) = snapshot.shoe.get(deck_index).copied() else {
        bail!("shoe exhausted")
    };
    snapshot.next_card_index += 1;
    snapshot.transcript_artifact.next_reveal_position = snapshot.next_card_index;
    Ok(DrawnCard {
        deck_index,
        card_id: card,
        rank: rank_from_card_id(card),
    })
}

fn generate_shoe(seed_text: &str, client_seed: &str) -> Vec<u16> {
    let mut deck = canonical_shoe();
    let seed = combined_shuffle_seed(seed_text, client_seed);
    for index in (1..deck.len()).rev() {
        let swap_hash = poseidon_hash_many(&[seed, Felt::from(index as u64)]);
        let swap_index = felt_mod_usize(&swap_hash, index + 1);
        deck.swap(index, swap_index);
    }
    deck
}

fn canonical_shoe() -> Vec<u16> {
    let mut deck = Vec::with_capacity(52 * SHOE_DECKS);
    for deck_copy in 0..SHOE_DECKS {
        for suit in 0..4 {
            for rank_zero_based in 0..13 {
                let card_id = ((deck_copy * 52) + (suit * 13) + rank_zero_based) as u16;
                deck.push(card_id);
            }
        }
    }
    deck
}

fn combined_shuffle_seed(seed_text: &str, client_seed: &str) -> Felt {
    poseidon_hash_many(&[
        felt_from_text_or_hex("blackjack.server_seed", seed_text),
        felt_from_text_or_hex("blackjack.client_seed", client_seed),
    ])
}

fn felt_from_text_or_hex(label: &str, value: &str) -> Felt {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Felt::ZERO;
    }
    if let Ok(felt) = Felt::from_hex(trimmed) {
        return felt;
    }
    let mut felts = vec![felt_from_label(label)];
    for chunk in trimmed.as_bytes().chunks(31) {
        let mut padded = [0_u8; 32];
        let start = 32 - chunk.len();
        padded[start..].copy_from_slice(chunk);
        felts.push(Felt::from_bytes_be(&padded));
    }
    poseidon_hash_many(&felts)
}

fn felt_from_label(label: &str) -> Felt {
    let mut felts = Vec::new();
    for chunk in label.as_bytes().chunks(31) {
        let mut padded = [0_u8; 32];
        let start = 32 - chunk.len();
        padded[start..].copy_from_slice(chunk);
        felts.push(Felt::from_bytes_be(&padded));
    }
    poseidon_hash_many(&felts)
}

fn felt_mod_usize(value: &Felt, modulus: usize) -> usize {
    if modulus == 0 {
        return 0;
    }
    value.to_bytes_be().iter().fold(0usize, |acc, byte| {
        ((acc * 256) + usize::from(*byte)) % modulus
    })
}

fn build_transcript_artifact(
    hand_id: &str,
    player: &str,
    table_id: u64,
    server_seed_hash: &str,
    server_seed: &str,
    client_seed: &str,
    shoe: &[u16],
) -> BlackjackTranscriptArtifact {
    let dealer_entropy_commitment = if server_seed_hash.is_empty() {
        hash_hex(format!(
            "moros:blackjack:dealer-entropy:v1:{hand_id}:{player}:{table_id}"
        ))
    } else {
        server_seed_hash.to_string()
    };
    let player_felt =
        felt_from_hex_string(player, "blackjack transcript player").unwrap_or(Felt::ZERO);
    let player_entropy_commitment = if client_seed.is_empty() {
        hash_hex(format!(
            "moros:blackjack:player-hint:v1:{player}:{table_id}:{hand_id}"
        ))
    } else {
        hash_hex(format!("moros:blackjack:client-seed:v1:{client_seed}"))
    };
    let cards = shoe
        .iter()
        .enumerate()
        .map(|(deck_index, card_id)| {
            let rank = rank_from_card_id(*card_id);
            let onchain_salt = onchain_card_salt(
                hand_id,
                player,
                table_id,
                server_seed,
                client_seed,
                deck_index,
            )
            .expect("blackjack onchain card salt must build");
            BlackjackCommittedCard {
                deck_index,
                card_id: *card_id,
                rank,
                commitment: hash_hex(format!(
                    "moros:blackjack:card:v1:{dealer_entropy_commitment}:{hand_id}:{table_id}:{deck_index}:{card_id}"
                )),
                onchain_commitment: onchain_card_leaf(
                    table_id,
                    player_felt,
                    deck_index,
                    *card_id,
                    &onchain_salt,
                )
                .expect("blackjack onchain card commitment must build"),
                onchain_salt,
            }
        })
        .collect::<Vec<_>>();
    let encrypted_cards = shoe
        .iter()
        .enumerate()
        .map(|(deck_index, card_id)| {
            expected_private_witness_card(
                hand_id,
                table_id,
                &dealer_entropy_commitment,
                server_seed,
                client_seed,
                deck_index,
                *card_id,
            )
        })
        .collect::<Vec<_>>();
    let deck_commitment_root = onchain_deck_commitment_root_hex(table_id, player_felt, &cards)
        .expect("blackjack deck commitment root must build");
    let encrypted_deck_root =
        merkle_root_hex(encrypted_cards.iter().map(encrypted_leaf_hash).collect());

    BlackjackTranscriptArtifact {
        protocol_mode: BLACKJACK_PROTOCOL_MODE_CURRENT.to_string(),
        target_protocol_mode: BLACKJACK_PROTOCOL_MODE_TARGET.to_string(),
        entropy_mode: if client_seed.is_empty() {
            "dealer_seed_only".to_string()
        } else {
            "dealer_plus_client_seed".to_string()
        },
        commitment_scheme: BLACKJACK_COMMITMENT_SCHEME.to_string(),
        encryption_scheme: BLACKJACK_ENCRYPTION_SCHEME_CURRENT.to_string(),
        target_encryption_scheme: BLACKJACK_ENCRYPTION_SCHEME_TARGET.to_string(),
        ruleset_hash: hash_hex(BLACKJACK_RULESET_SPEC),
        deck_commitment_root: deck_commitment_root.clone(),
        encrypted_deck_root,
        dealer_entropy_commitment: dealer_entropy_commitment.clone(),
        player_entropy_commitment,
        shuffle_commitment: hash_hex(format!(
            "moros:blackjack:shuffle:v1:{dealer_entropy_commitment}:{}:{deck_commitment_root}",
            if client_seed.is_empty() {
                "no-client-seed"
            } else {
                client_seed
            }
        )),
        hole_card_index: 3,
        next_reveal_position: 0,
        cards,
        encrypted_cards,
        reveals: Vec::new(),
        dealer_peek: BlackjackDealerPeekState::default(),
        insurance: BlackjackInsuranceState::default(),
    }
}

fn expected_private_witness_card(
    hand_id: &str,
    table_id: u64,
    dealer_entropy_commitment: &str,
    server_seed: &str,
    client_seed: &str,
    deck_index: usize,
    card_id: u16,
) -> BlackjackEncryptedCardEnvelope {
    let rank = rank_from_card_id(card_id);
    BlackjackEncryptedCardEnvelope {
        deck_index,
        card_id,
        commitment: hash_hex(format!(
            "moros:blackjack:card:v1:{dealer_entropy_commitment}:{hand_id}:{table_id}:{deck_index}:{card_id}"
        )),
        ciphertext: hash_hex(format!(
            "moros:blackjack:sealed-card:v1:{server_seed}:{client_seed}:{hand_id}:{table_id}:{deck_index}:{card_id}"
        )),
        nonce_commitment: hash_hex(format!(
            "moros:blackjack:nonce:v1:{server_seed}:{client_seed}:{hand_id}:{table_id}:{deck_index}"
        )),
        reveal_key_commitment: hash_hex(format!(
            "moros:blackjack:reveal-key:v1:{server_seed}:{client_seed}:{hand_id}:{table_id}:{deck_index}:{card_id}:{rank}"
        )),
    }
}

fn record_reveal(
    snapshot: &mut BlackjackHandSnapshot,
    deck_index: usize,
    card_id: u16,
    rank: u8,
    stage: &str,
    target: &str,
) {
    snapshot
        .transcript_artifact
        .reveals
        .push(BlackjackRevealRecord {
            deck_index,
            card_id,
            rank,
            stage: stage.to_string(),
            target: target.to_string(),
            receipt: BlackjackProofReceipt {
                proof_kind: BLACKJACK_REVEAL_PROOF_KIND_ENVELOPE.to_string(),
                receipt: hash_hex(format!(
                    "moros:blackjack:reveal-opening:v1:{}:{}:{}:{}",
                    snapshot
                        .transcript_artifact
                        .encrypted_cards
                        .get(deck_index)
                        .map(|card| card.ciphertext.as_str())
                        .unwrap_or_default(),
                    snapshot
                        .transcript_artifact
                        .cards
                        .get(deck_index)
                        .map(|card| card.commitment.as_str())
                        .unwrap_or_default(),
                    rank,
                    target
                )),
                verified: true,
            },
            opening: merkle_opening_for_encrypted_card(
                &snapshot.transcript_artifact.encrypted_cards,
                deck_index,
                &snapshot.transcript_artifact.encrypted_deck_root,
            ),
        });
}

fn update_dealer_peek_state(
    snapshot: &mut BlackjackHandSnapshot,
    dealer_up_rank: u8,
    dealer_hole_rank: u8,
    hole_card_index: usize,
    verified: bool,
) {
    let required = dealer_up_rank == 1 || card_value(dealer_up_rank) == 10;
    let outcome = if !required {
        "not_required"
    } else if hand_math(&[dealer_up_rank, dealer_hole_rank]).total == 21 {
        "dealer_blackjack"
    } else {
        "no_blackjack_seed_verified"
    };
    let statement_kind = dealer_peek_statement_kind(required, dealer_up_rank, outcome);
    let target_proof_kind = if required {
        BLACKJACK_PEEK_PROOF_KIND_TARGET.to_string()
    } else {
        String::new()
    };
    let proof_mode = if required {
        BLACKJACK_PEEK_PROOF_MODE_TARGET.to_string()
    } else {
        String::new()
    };
    let target_proof_mode = if required {
        BLACKJACK_PEEK_PROOF_MODE_TARGET.to_string()
    } else {
        String::new()
    };
    let statement = if required {
        BlackjackNoBlackjackProofStatement {
            hand_id: snapshot.hand_id.clone(),
            player: snapshot.player.clone(),
            table_id: snapshot.table_id,
            ruleset_hash: snapshot.transcript_artifact.ruleset_hash.clone(),
            deck_commitment_root: snapshot.transcript_artifact.deck_commitment_root.clone(),
            encrypted_deck_root: snapshot.transcript_artifact.encrypted_deck_root.clone(),
            dealer_upcard_rank: Some(dealer_up_rank),
            hole_card_index: Some(hole_card_index),
            statement_kind: statement_kind.to_string(),
        }
    } else {
        BlackjackNoBlackjackProofStatement::default()
    };
    let statement_hash = if required {
        compute_no_blackjack_statement_hash(&statement)
    } else {
        String::new()
    };
    let hole_value_class = dealer_hole_value_class(dealer_hole_rank);
    let public_inputs_hash = if required {
        compute_peek_public_inputs_hash(&statement_hash, statement_kind, &target_proof_kind)
    } else {
        String::new()
    };
    let hidden_value_class_commitment = if required {
        poseidon_hex(&[
            felt_from_label("moros.blackjack.peek.hidden_class.v3"),
            felt_from_text_or_hex(
                "blackjack.encrypted_deck_root",
                &snapshot.transcript_artifact.encrypted_deck_root,
            ),
            Felt::from(hole_card_index as u64),
            Felt::from(dealer_up_rank),
            felt_from_text_or_hex("blackjack.hole_value_class", hole_value_class),
        ])
    } else {
        String::new()
    };
    let witness_commitment = if required {
        poseidon_hex(&[
            felt_from_label("moros.blackjack.peek.witness.v3"),
            felt_from_text_or_hex("blackjack.transcript_root", &snapshot.transcript_root),
            Felt::from(dealer_up_rank),
            Felt::from(dealer_hole_rank),
            Felt::from(hole_card_index as u64),
        ])
    } else {
        String::new()
    };
    let opening = if required {
        merkle_opening_for_encrypted_card(
            &snapshot.transcript_artifact.encrypted_cards,
            hole_card_index,
            &snapshot.transcript_artifact.encrypted_deck_root,
        )
    } else {
        BlackjackMerkleOpening::default()
    };
    let proof_opening = if required {
        onchain_private_opening(snapshot, hole_card_index)
            .expect("dealer peek proof opening must match onchain transcript root")
    } else {
        BlackjackMerkleOpening::default()
    };
    let hole_card_rank_commitment = if required {
        let witness = BlackjackDealerPeekPrivateWitness {
            dealer_hole_rank,
            server_seed_hash: snapshot.server_seed_hash.clone(),
            server_seed: snapshot.server_seed.clone(),
            client_seed: snapshot.client_seed.clone(),
            card_salt: snapshot.transcript_artifact.cards[hole_card_index]
                .onchain_salt
                .clone(),
            card: snapshot.transcript_artifact.encrypted_cards[hole_card_index].clone(),
            opening: proof_opening.clone(),
        };
        compute_hole_card_rank_commitment(
            &snapshot.hand_id,
            &snapshot.player,
            snapshot.table_id,
            &snapshot.transcript_root,
            statement_kind,
            &statement_hash,
            &public_inputs_hash,
            &snapshot.transcript_artifact.encrypted_deck_root,
            dealer_up_rank,
            hole_card_index,
            &witness,
        )
    } else {
        String::new()
    };
    let zk_proof_target = if required {
        build_no_blackjack_zk_proof_target(
            statement_kind,
            &statement_hash,
            &public_inputs_hash,
            &snapshot.transcript_artifact.encrypted_deck_root,
            dealer_up_rank,
            hole_card_index,
            &hidden_value_class_commitment,
            &witness_commitment,
            &hole_card_rank_commitment,
        )
    } else {
        BlackjackZkProofTargetArtifact::default()
    };
    let proof_binding = if required {
        let request = BlackjackZkPeekProofRequest {
            hand_id: snapshot.hand_id.clone(),
            player: snapshot.player.clone(),
            table_id: snapshot.table_id,
            transcript_root: snapshot.transcript_root.clone(),
            target: zk_proof_target.clone(),
            private_witness: Some(BlackjackDealerPeekPrivateWitness {
                dealer_hole_rank,
                server_seed_hash: snapshot.server_seed_hash.clone(),
                server_seed: snapshot.server_seed.clone(),
                client_seed: snapshot.client_seed.clone(),
                card_salt: snapshot.transcript_artifact.cards[hole_card_index]
                    .onchain_salt
                    .clone(),
                card: snapshot.transcript_artifact.encrypted_cards[hole_card_index].clone(),
                opening: proof_opening.clone(),
            }),
            onchain_context: None,
        };
        build_no_blackjack_proof_binding(&request)
            .expect("local blackjack dealer-peek proof binding must build")
    } else {
        BlackjackProofBindingArtifact::default()
    };
    let receipt = if required {
        BlackjackProofReceipt {
            proof_kind: BLACKJACK_PEEK_PROOF_KIND_TARGET.to_string(),
            receipt: proof_binding.proof_id.clone(),
            verified: verified
                && verify_no_blackjack_proof_binding(&zk_proof_target, &proof_binding),
        }
    } else {
        BlackjackProofReceipt::default()
    };
    let no_blackjack_proof = if required {
        BlackjackNoBlackjackProofArtifact {
            available: true,
            verifier_status: if receipt.verified {
                BLACKJACK_PEEK_PROOF_BINDING_STATUS_VERIFIED.to_string()
            } else {
                "unverified".to_string()
            },
            verifier_namespace: BLACKJACK_PEEK_VERIFIER_NAMESPACE.to_string(),
            claim: statement_kind.to_string(),
            statement_hash,
            statement,
            current_proof_mode: BLACKJACK_PEEK_PROOF_MODE_TARGET.to_string(),
            target_proof_mode: target_proof_mode.clone(),
            current_proof_kind: BLACKJACK_PEEK_PROOF_KIND_TARGET.to_string(),
            target_proof_kind: target_proof_kind.clone(),
            statement_kind: statement_kind.to_string(),
            public_inputs_hash: public_inputs_hash.clone(),
            hidden_value_class_commitment: hidden_value_class_commitment.clone(),
            witness_commitment: witness_commitment.clone(),
            hole_card_rank_commitment: hole_card_rank_commitment.clone(),
            receipt: receipt.clone(),
            opening: opening.clone(),
            zk_proof_target,
            proof_binding,
        }
    } else {
        BlackjackNoBlackjackProofArtifact::default()
    };

    snapshot.transcript_artifact.dealer_peek = BlackjackDealerPeekState {
        required,
        checked: required,
        upcard_rank: Some(dealer_up_rank),
        hole_card_index: Some(hole_card_index),
        outcome: outcome.to_string(),
        proof_mode,
        target_proof_mode,
        target_proof_kind,
        statement_kind: statement_kind.to_string(),
        public_inputs_hash,
        hidden_value_class_commitment,
        witness_commitment,
        hole_card_rank_commitment,
        no_blackjack_proof,
        receipt,
        opening,
    };

    if required {
        snapshot.action_log.push(BlackjackActionLogEntry {
            action: "dealer_peek".to_string(),
            seat_index: None,
            detail: if outcome == "dealer_blackjack" {
                "dealer peek found blackjack".to_string()
            } else {
                "dealer peek proved no blackjack before player action".to_string()
            },
        });
    }
}

fn update_insurance_state(snapshot: &mut BlackjackHandSnapshot) -> anyhow::Result<()> {
    let dealer_up_rank = snapshot
        .dealer
        .cards
        .first()
        .map(|card| card.rank)
        .unwrap_or_default();
    if dealer_up_rank != 1 {
        let insurance = BlackjackInsuranceState {
            offered: false,
            supported: false,
            max_wager: "0".to_string(),
            wager: "0".to_string(),
            taken: false,
            settled: true,
            outcome: "not_offered".to_string(),
        };
        snapshot.insurance = insurance.clone();
        snapshot.transcript_artifact.insurance = insurance;
        return Ok(());
    }

    let max_wager = (parse_amount(&snapshot.wager)? / 2).to_string();
    let insurance = BlackjackInsuranceState {
        offered: true,
        supported: true,
        max_wager,
        wager: "0".to_string(),
        taken: false,
        settled: false,
        outcome: "offered".to_string(),
    };
    snapshot.insurance = insurance.clone();
    snapshot.transcript_artifact.insurance = insurance;
    snapshot.action_log.push(BlackjackActionLogEntry {
        action: "insurance_offer".to_string(),
        seat_index: None,
        detail: "insurance window opened on dealer Ace before player action".to_string(),
    });
    Ok(())
}

fn settle_insurance_state(snapshot: &mut BlackjackHandSnapshot, dealer_blackjack: bool) {
    if !snapshot.insurance.offered || snapshot.insurance.settled {
        return;
    }
    snapshot.insurance.settled = true;
    snapshot.insurance.outcome = if dealer_blackjack {
        "dealer_blackjack".to_string()
    } else if snapshot.insurance.taken {
        "lost".to_string()
    } else {
        "declined".to_string()
    };
    snapshot.transcript_artifact.insurance = snapshot.insurance.clone();
}

fn insurance_payout(snapshot: &BlackjackHandSnapshot) -> anyhow::Result<u128> {
    if snapshot.insurance.taken && snapshot.insurance.outcome == "dealer_blackjack" {
        Ok(parse_amount(&snapshot.insurance.wager)? * 3)
    } else {
        Ok(0)
    }
}

fn should_force_opening_dealer_blackjack_reveal(snapshot: &BlackjackHandSnapshot) -> bool {
    snapshot.transcript_artifact.dealer_peek.required
        && (!snapshot.insurance.offered || snapshot.insurance.settled)
        && snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack"
}

fn dealer_peek_statement_kind(required: bool, dealer_up_rank: u8, outcome: &str) -> &'static str {
    if !required {
        return "not_required";
    }
    if outcome == "dealer_blackjack" {
        return "dealer_blackjack_revealed";
    }
    if dealer_up_rank == 1 {
        "hole_card_not_ten_value"
    } else {
        "hole_card_not_ace"
    }
}

fn dealer_hole_value_class(rank: u8) -> &'static str {
    if rank == 1 {
        "ace"
    } else if card_value(rank) == 10 {
        "ten_value"
    } else {
        "non_blackjack_value"
    }
}

pub fn blackjack_hash_hex(input: impl AsRef<[u8]>) -> String {
    let mut felts = vec![felt_from_label("moros.blackjack.hash.v1")];
    for chunk in input.as_ref().chunks(31) {
        let mut padded = [0_u8; 32];
        let start = 32 - chunk.len();
        padded[start..].copy_from_slice(chunk);
        felts.push(Felt::from_bytes_be(&padded));
    }
    poseidon_hex(&felts)
}

fn hash_hex(input: impl AsRef<[u8]>) -> String {
    blackjack_hash_hex(input)
}

fn normalize_hex_string(value: &str) -> String {
    value.trim().trim_start_matches("0x").to_ascii_lowercase()
}

fn same_hex_string(left: &str, right: &str) -> bool {
    !left.trim().is_empty()
        && !right.trim().is_empty()
        && normalize_hex_string(left) == normalize_hex_string(right)
}

fn felt_from_hex_string(value: &str, label: &str) -> anyhow::Result<Felt> {
    Felt::from_hex(value.trim()).with_context(|| format!("invalid {label}: {value}"))
}

fn bn254_field_from_hex_string(value: &str, label: &str) -> anyhow::Result<Bn128FieldElement> {
    let trimmed = value.trim();
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    let parsed = if normalized.is_empty() {
        BigUint::default()
    } else {
        BigUint::parse_bytes(normalized.as_bytes(), 16)
            .with_context(|| format!("invalid {label}: {value}"))?
    };
    Ok(Bn128FieldElement::from_biguint(&parsed))
}

fn bn254_field_from_felt(value: Felt) -> Bn128FieldElement {
    Bn128FieldElement::from_biguint(
        &BigUint::parse_bytes(
            format!("{value:#x}").trim_start_matches("0x").as_bytes(),
            16,
        )
        .unwrap_or_default(),
    )
}

fn bn254_field_from_u128(value: u128) -> Bn128FieldElement {
    Bn128FieldElement::from_biguint(&BigUint::from(value))
}

fn bn254_hex(value: &Bn128FieldElement) -> String {
    let encoded = value.to_biguint().to_str_radix(16);
    if encoded.is_empty() {
        "0x0".to_string()
    } else {
        format!("0x{encoded}")
    }
}

fn bn254_poseidon_hex(inputs: &[Bn128FieldElement]) -> anyhow::Result<String> {
    let input_count = u8::try_from(inputs.len()).context("bn254 poseidon input count overflow")?;
    let hash = poseidon_bn128(input_count, inputs)?;
    Ok(bn254_hex(&hash))
}

fn onchain_card_salt(
    hand_id: &str,
    player: &str,
    table_id: u64,
    server_seed: &str,
    client_seed: &str,
    deck_index: usize,
) -> anyhow::Result<String> {
    let seed = poseidon_bn128(
        5,
        &[
            bn254_field_from_felt(felt_from_text_or_hex("blackjack.hand_id", hand_id)),
            bn254_field_from_felt(felt_from_text_or_hex("blackjack.player", player)),
            Bn128FieldElement::from(table_id),
            bn254_field_from_felt(felt_from_text_or_hex("blackjack.server_seed", server_seed)),
            bn254_field_from_felt(felt_from_text_or_hex("blackjack.client_seed", client_seed)),
        ],
    )?;
    let value = poseidon_bn128(
        3,
        &[
            seed,
            Bn128FieldElement::from(deck_index as u64),
            bn254_field_from_felt(
                felt_from_hex_string(
                    BLACKJACK_ONCHAIN_CARD_SALT_DOMAIN_HEX,
                    "blackjack onchain card salt domain",
                )
                .expect("card salt domain constant must be valid"),
            ),
        ],
    )?;
    let value = if value == Bn128FieldElement::zero() {
        Bn128FieldElement::from(1_u64)
    } else {
        value
    };
    Ok(bn254_hex(&value))
}

fn onchain_card_leaf(
    table_id: u64,
    player: Felt,
    deck_index: usize,
    card_id: u16,
    salt: &str,
) -> anyhow::Result<String> {
    let _ = table_id;
    let _ = player;
    let _ = deck_index;
    bn254_poseidon_hex(&[
        Bn128FieldElement::from(card_id as u64),
        bn254_field_from_hex_string(salt, "blackjack onchain card salt")?,
    ])
}

fn onchain_card_leaf_from_committed_card(
    table_id: u64,
    player: Felt,
    card: &BlackjackCommittedCard,
) -> anyhow::Result<String> {
    onchain_card_leaf(
        table_id,
        player,
        card.deck_index,
        card.card_id,
        &card.onchain_salt,
    )
}

fn onchain_deck_commitment_leaves(
    table_id: u64,
    player: Felt,
    cards: &[BlackjackCommittedCard],
) -> anyhow::Result<Vec<String>> {
    let mut leaves = vec!["0x0".to_string(); BLACKJACK_ONCHAIN_CARD_TREE_SIZE];
    for card in cards.iter().take(BLACKJACK_ONCHAIN_CARD_TREE_SIZE) {
        leaves[card.deck_index] = onchain_card_leaf_from_committed_card(table_id, player, card)?;
    }
    Ok(leaves)
}

fn onchain_merkle_root_from_leaves(mut level: Vec<String>) -> anyhow::Result<String> {
    if level.is_empty() {
        return Ok("0x0".to_string());
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len() / 2);
        let mut index = 0;
        while index < level.len() {
            let right = level
                .get(index + 1)
                .cloned()
                .unwrap_or_else(|| level[index].clone());
            next.push(bn254_poseidon_hex(&[
                bn254_field_from_hex_string(&level[index], "blackjack onchain merkle left")?,
                bn254_field_from_hex_string(&right, "blackjack onchain merkle right")?,
            ])?);
            index += 2;
        }
        level = next;
    }
    Ok(level[0].clone())
}

fn onchain_deck_commitment_root_hex(
    table_id: u64,
    player: Felt,
    cards: &[BlackjackCommittedCard],
) -> anyhow::Result<String> {
    onchain_merkle_root_from_leaves(onchain_deck_commitment_leaves(table_id, player, cards)?)
}

pub fn onchain_card_reveal_proof(
    snapshot: &BlackjackHandSnapshot,
    deck_index: usize,
) -> anyhow::Result<BlackjackOnchainCardRevealProof> {
    ensure!(
        deck_index < BLACKJACK_ONCHAIN_CARD_TREE_SIZE,
        "deck index is outside onchain commitment tree"
    );
    let player = felt_from_hex_string(&snapshot.player, "blackjack player address")?;
    let card = snapshot
        .transcript_artifact
        .cards
        .get(deck_index)
        .with_context(|| format!("committed card {deck_index} is missing"))?;
    let mut level = onchain_deck_commitment_leaves(
        snapshot.table_id,
        player,
        &snapshot.transcript_artifact.cards,
    )?;
    let mut index = deck_index;
    let mut siblings = Vec::with_capacity(BLACKJACK_ONCHAIN_CARD_TREE_DEPTH);
    for _ in 0..BLACKJACK_ONCHAIN_CARD_TREE_DEPTH {
        let sibling_index = if index % 2 == 0 { index + 1 } else { index - 1 };
        let sibling = level
            .get(sibling_index)
            .cloned()
            .unwrap_or_else(|| level[index].clone());
        siblings.push(sibling);
        let mut next = Vec::with_capacity(level.len() / 2);
        let mut cursor = 0;
        while cursor < level.len() {
            let right = level
                .get(cursor + 1)
                .cloned()
                .unwrap_or_else(|| level[cursor].clone());
            next.push(bn254_poseidon_hex(&[
                bn254_field_from_hex_string(&level[cursor], "blackjack onchain merkle left")?,
                bn254_field_from_hex_string(&right, "blackjack onchain merkle right")?,
            ])?);
            cursor += 2;
        }
        level = next;
        index /= 2;
    }
    ensure!(
        same_hex_string(&level[0], &snapshot.transcript_root),
        "onchain card proof root does not match transcript root"
    );
    Ok(BlackjackOnchainCardRevealProof {
        deck_index: deck_index as u64,
        card_id: card.card_id,
        salt: card.onchain_salt.clone(),
        siblings,
    })
}

fn onchain_private_opening(
    snapshot: &BlackjackHandSnapshot,
    deck_index: usize,
) -> anyhow::Result<BlackjackMerkleOpening> {
    let proof = onchain_card_reveal_proof(snapshot, deck_index)?;
    let player = felt_from_hex_string(&snapshot.player, "blackjack player address")?;
    let leaf_hash = onchain_card_leaf(
        snapshot.table_id,
        player,
        deck_index,
        proof.card_id,
        &proof.salt,
    )?;
    Ok(BlackjackMerkleOpening {
        leaf_hash,
        leaf_index: deck_index,
        root: snapshot.transcript_root.clone(),
        siblings: proof.siblings,
        verified: true,
    })
}

fn onchain_card_reveal_proof_for_target(
    snapshot: &BlackjackHandSnapshot,
    target: &str,
) -> anyhow::Result<BlackjackOnchainCardRevealProof> {
    let reveal = snapshot
        .transcript_artifact
        .reveals
        .iter()
        .find(|reveal| reveal.target == target)
        .with_context(|| format!("reveal target {target} is missing"))?;
    onchain_card_reveal_proof(snapshot, reveal.deck_index)
}

fn encrypted_leaf_hash(card: &BlackjackEncryptedCardEnvelope) -> String {
    hash_hex(format!(
        "moros:blackjack:encrypted-leaf:v1:{}:{}:{}:{}:{}",
        card.deck_index,
        card.commitment,
        card.ciphertext,
        card.nonce_commitment,
        card.reveal_key_commitment
    ))
}

fn encrypted_leaf_hash_view(card: &BlackjackEncryptedCardEnvelopeView) -> String {
    hash_hex(format!(
        "moros:blackjack:encrypted-leaf:v1:{}:{}:{}:{}:{}",
        card.deck_index,
        card.commitment,
        card.ciphertext,
        card.nonce_commitment,
        card.reveal_key_commitment
    ))
}

fn merkle_root_hex(mut leaves: Vec<String>) -> String {
    if leaves.is_empty() {
        return hash_hex("moros:blackjack:encrypted-merkle:empty");
    }
    while leaves.len() > 1 {
        let mut next = Vec::with_capacity(leaves.len().div_ceil(2));
        let mut index = 0;
        while index < leaves.len() {
            let left = leaves[index].clone();
            let right = leaves
                .get(index + 1)
                .cloned()
                .unwrap_or_else(|| left.clone());
            next.push(hash_hex(format!(
                "moros:blackjack:encrypted-merkle-node:v1:{left}:{right}"
            )));
            index += 2;
        }
        leaves = next;
    }
    leaves.pop().unwrap_or_default()
}

fn merkle_root_from_opening(leaf_hash: &str, leaf_index: usize, siblings: &[String]) -> String {
    let mut hash = leaf_hash.to_string();
    let mut index = leaf_index;
    for sibling in siblings {
        hash = if index % 2 == 0 {
            hash_hex(format!(
                "moros:blackjack:encrypted-merkle-node:v1:{hash}:{sibling}"
            ))
        } else {
            hash_hex(format!(
                "moros:blackjack:encrypted-merkle-node:v1:{sibling}:{hash}"
            ))
        };
        index /= 2;
    }
    hash
}

fn merkle_opening_for_encrypted_card(
    cards: &[BlackjackEncryptedCardEnvelope],
    leaf_index: usize,
    expected_root: &str,
) -> BlackjackMerkleOpening {
    let Some(card) = cards.get(leaf_index) else {
        return BlackjackMerkleOpening::default();
    };
    let leaf_hash = encrypted_leaf_hash(card);
    let mut level = cards.iter().map(encrypted_leaf_hash).collect::<Vec<_>>();
    let mut index = leaf_index;
    let mut siblings = Vec::new();
    while level.len() > 1 {
        let sibling_index = if index % 2 == 0 { index + 1 } else { index - 1 };
        let sibling = level
            .get(sibling_index)
            .cloned()
            .unwrap_or_else(|| level[index].clone());
        siblings.push(sibling);

        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut cursor = 0;
        while cursor < level.len() {
            let left = level[cursor].clone();
            let right = level
                .get(cursor + 1)
                .cloned()
                .unwrap_or_else(|| left.clone());
            next.push(hash_hex(format!(
                "moros:blackjack:encrypted-merkle-node:v1:{left}:{right}"
            )));
            cursor += 2;
        }
        level = next;
        index /= 2;
    }
    let root = level
        .first()
        .cloned()
        .unwrap_or_else(|| hash_hex("moros:blackjack:encrypted-merkle:empty"));
    BlackjackMerkleOpening {
        leaf_hash,
        leaf_index,
        root: root.clone(),
        siblings,
        verified: !expected_root.is_empty() && expected_root == root,
    }
}

fn card_value(rank: u8) -> u8 {
    match rank {
        1 => 1,
        10..=13 => 10,
        _ => rank,
    }
}

fn rank_label(rank: u8) -> String {
    match rank {
        1 => "A".to_string(),
        11 => "J".to_string(),
        12 => "Q".to_string(),
        13 => "K".to_string(),
        _ => rank.to_string(),
    }
}

fn parse_amount(value: &str) -> anyhow::Result<u128> {
    value.parse::<u128>().map_err(Into::into)
}

fn format_strk_amount(raw: &str) -> String {
    let Ok(value) = raw.parse::<u128>() else {
        return raw.to_string();
    };
    let whole = value / 1_000_000_000_000_000_000_u128;
    let fractional = value % 1_000_000_000_000_000_000_u128;
    if fractional == 0 {
        return whole.to_string();
    }

    let mut fractional_text = format!("{fractional:018}");
    while fractional_text.ends_with('0') {
        fractional_text.pop();
    }
    format!("{whole}.{fractional_text}")
}

fn same_felt_hex(left: &str, right: &str) -> bool {
    normalize_felt_hex(left) == normalize_felt_hex(right)
}

fn normalize_felt_hex(value: &str) -> String {
    let trimmed = value.trim();
    let without_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed)
        .trim_start_matches('0');

    if without_prefix.is_empty() {
        "0".to_string()
    } else {
        without_prefix.to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BlackjackCardSnapshot, BlackjackChainHand, BlackjackChainSeat, BlackjackDealerSnapshot,
        BlackjackHandSnapshot, BlackjackInsuranceState, BlackjackSeatSnapshot, allowed_actions,
        apply_action_to_snapshot, hand_math, plan_timeout_submission, reconcile_view_with_chain,
        same_felt_hex, seed_hand_snapshot, seed_hand_snapshot_with_secret,
    };
    use std::sync::OnceLock;

    const TEST_SERVER_SEED_HASH: &str = "0xfeedblackjack";
    const TEST_SERVER_SEED: &str = "0xcafeblackjack";

    fn test_card_id(rank: u8, variant: u16) -> u16 {
        (variant * 13) + u16::from(rank - 1)
    }

    fn opening_ranks(client_seed: &str) -> [u8; 4] {
        let shoe = super::generate_shoe(TEST_SERVER_SEED, client_seed);
        [
            super::rank_from_card_id(shoe[0]),
            super::rank_from_card_id(shoe[1]),
            super::rank_from_card_id(shoe[2]),
            super::rank_from_card_id(shoe[3]),
        ]
    }

    fn find_client_seed(predicate: impl Fn([u8; 4]) -> bool) -> String {
        for candidate in 0..50_000 {
            let seed = format!("blackjack-search-{candidate}");
            if predicate(opening_ranks(&seed)) {
                return seed;
            }
        }
        panic!("no blackjack opening seed matched the requested predicate");
    }

    fn no_blackjack_peek_client_seed() -> &'static str {
        static SEED: OnceLock<String> = OnceLock::new();
        SEED.get_or_init(|| {
            find_client_seed(|opening| opening[1] == 1 && super::card_value(opening[3]) != 10)
        })
        .as_str()
    }

    fn dealer_blackjack_client_seed() -> &'static str {
        static SEED: OnceLock<String> = OnceLock::new();
        SEED.get_or_init(|| {
            find_client_seed(|opening| opening[1] == 1 && super::card_value(opening[3]) == 10)
        })
        .as_str()
    }

    fn player_blackjack_client_seed() -> &'static str {
        static SEED: OnceLock<String> = OnceLock::new();
        SEED.get_or_init(|| {
            find_client_seed(|opening| {
                let math = super::hand_math(&[opening[0], opening[2]]);
                super::is_blackjack(2, math.total, 0)
                    && opening[1] != 1
                    && super::card_value(opening[1]) != 10
            })
        })
        .as_str()
    }

    fn seeded_snapshot_for_client_seed(
        hand_id: &str,
        player: &str,
        table_id: u64,
        wager: &str,
        transcript_root: &str,
        client_seed: &str,
    ) -> BlackjackHandSnapshot {
        seed_hand_snapshot_with_secret(
            hand_id,
            player,
            table_id,
            wager,
            transcript_root,
            TEST_SERVER_SEED_HASH,
            TEST_SERVER_SEED,
            Some(client_seed),
        )
        .expect("snapshot should seed")
    }

    #[test]
    fn seeded_hand_has_two_player_cards_and_one_hidden_dealer_card() {
        let snapshot = seed_hand_snapshot("hand-1", "0xabc", 1, "100", "0xfeed").unwrap();
        assert_eq!(snapshot.seats.len(), 1);
        assert_eq!(snapshot.seats[0].cards.len(), 2);
        assert_eq!(snapshot.dealer.cards.len(), 2);
        assert!(snapshot.dealer.cards[0].revealed);
        assert!(!snapshot.dealer.cards[1].revealed);
        assert_eq!(
            snapshot.transcript_artifact.cards.len(),
            52 * super::SHOE_DECKS
        );
        assert_eq!(
            snapshot.transcript_artifact.encrypted_cards.len(),
            52 * super::SHOE_DECKS
        );
        assert_eq!(snapshot.transcript_artifact.reveals.len(), 3);
        assert_eq!(snapshot.transcript_artifact.hole_card_index, 3);
        assert!(!snapshot.transcript_artifact.deck_commitment_root.is_empty());
        assert!(!snapshot.transcript_artifact.encrypted_deck_root.is_empty());
        assert_eq!(
            snapshot.transcript_root,
            snapshot.transcript_artifact.deck_commitment_root
        );
    }

    #[test]
    fn hand_math_treats_ace_nine_six_as_hard_sixteen() {
        let math = hand_math(&[1, 9, 6]);
        assert_eq!(math.total, 16);
        assert!(!math.soft);
    }

    #[test]
    fn late_surrender_is_only_available_before_the_first_player_action() {
        let mut snapshot = manual_player_turn(vec![BlackjackSeatSnapshot {
            seat_index: 0,
            wager: "100".to_string(),
            cards: vec![revealed(10), revealed(6)],
            status: "active".to_string(),
            outcome: None,
            payout: "0".to_string(),
            doubled: false,
            split_depth: 0,
            split_aces: false,
        }]);
        let opening_actions = allowed_actions(&snapshot);
        assert!(opening_actions.iter().any(|action| action == "surrender"));
        assert!(opening_actions.iter().any(|action| action == "double"));

        apply_action_to_snapshot(&mut snapshot, "hit").unwrap();
        let actions = allowed_actions(&snapshot);
        assert!(!actions.iter().any(|action| action == "double"));
        assert!(!actions.iter().any(|action| action == "surrender"));
    }

    #[test]
    fn split_rules_allow_four_hands_max() {
        let mut snapshot = manual_player_turn(vec![pair_seat(0), pair_seat(1), pair_seat(2)]);
        snapshot.active_seat = 2;
        assert!(
            allowed_actions(&snapshot)
                .iter()
                .any(|action| action == "split")
        );

        snapshot.seats.push(pair_seat(3));
        snapshot.seat_count = 4;
        assert!(
            !allowed_actions(&snapshot)
                .iter()
                .any(|action| action == "split")
        );
    }

    #[test]
    fn split_aces_receive_one_card_each_and_cannot_continue() {
        let mut snapshot = manual_player_turn(vec![BlackjackSeatSnapshot {
            seat_index: 0,
            wager: "100".to_string(),
            cards: vec![revealed(1), revealed(1)],
            status: "active".to_string(),
            outcome: None,
            payout: "0".to_string(),
            doubled: false,
            split_depth: 0,
            split_aces: false,
        }]);
        snapshot.shoe = vec![
            test_card_id(10, 0),
            test_card_id(9, 1),
            test_card_id(8, 2),
            test_card_id(7, 3),
        ];
        snapshot.next_card_index = 0;

        apply_action_to_snapshot(&mut snapshot, "split").unwrap();

        assert_eq!(snapshot.seats.len(), 2);
        assert!(snapshot.seats.iter().all(|seat| seat.split_aces));
        assert!(snapshot.seats.iter().all(|seat| seat.status != "active"));
        assert!(allowed_actions(&snapshot).is_empty());
    }

    #[test]
    fn felt_hex_comparison_ignores_leading_zero_padding() {
        assert!(same_felt_hex(
            "0x0643e1766bc860d19ce81",
            "0x643e1766bc860d19ce81"
        ));
        assert!(same_felt_hex(
            "0x019d6118e3327d508c08ef6573433fe4",
            "0x19d6118e3327d508c08ef6573433fe4"
        ));
    }

    #[test]
    fn seeded_natural_blackjack_auto_enters_dealer_resolution() {
        let snapshot = seeded_snapshot_for_client_seed(
            "sepolia-hand-3",
            "0x643e1766bc860d19ce81c8c7c315d62e5396f2f404d441d0ae9f75e7ed7548a",
            1,
            "10000000000000000000",
            "0x019d611dc01078e186304f2a1cf5bf1e",
            player_blackjack_client_seed(),
        );

        let player_ranks = snapshot.seats[0]
            .cards
            .iter()
            .map(|card| card.rank)
            .collect::<Vec<_>>();
        let player_math = super::hand_math(&player_ranks);
        assert!(super::is_blackjack(2, player_math.total, 0));
        assert_eq!(snapshot.seats[0].status, "settled");
        assert_eq!(snapshot.seats[0].outcome.as_deref(), Some("blackjack"));
        assert_eq!(snapshot.status, "settled");
        assert_eq!(snapshot.phase, "settled");
        assert!(allowed_actions(&snapshot).is_empty());
        assert!(snapshot.dealer.cards.len() >= 2);
        assert!(snapshot.dealer.cards.iter().all(|card| card.revealed));
    }

    #[test]
    fn dealer_peek_for_blackjack_waits_for_insurance_decision() {
        let snapshot = seeded_snapshot_for_client_seed(
            "dealer-bj-open",
            "0xabc",
            1,
            "100",
            "0xfeed-dealer-bj-open",
            dealer_blackjack_client_seed(),
        );

        assert!(snapshot.transcript_artifact.dealer_peek.required);
        assert_eq!(
            snapshot.transcript_artifact.dealer_peek.outcome,
            "dealer_blackjack"
        );
        assert_eq!(
            snapshot.transcript_artifact.dealer_peek.statement_kind,
            "dealer_blackjack_revealed"
        );
        assert!(
            snapshot
                .transcript_artifact
                .dealer_peek
                .no_blackjack_proof
                .available
        );
        assert_eq!(
            snapshot.transcript_artifact.dealer_peek.target_proof_kind,
            "zk_no_blackjack_peek_groth16_v3"
        );
        assert!(snapshot.insurance.offered);
        assert_eq!(snapshot.insurance.max_wager, "50");
        assert_eq!(snapshot.insurance.outcome, "offered");
        assert_eq!(snapshot.status, "awaiting_insurance");
        assert_eq!(snapshot.phase, "insurance");
        assert_eq!(
            allowed_actions(&snapshot),
            vec![
                "take_insurance".to_string(),
                "decline_insurance".to_string()
            ]
        );
        assert!(!snapshot.dealer.cards[1].revealed);
    }

    #[test]
    fn ace_upcard_tracks_insurance_offer_state() {
        let snapshot = seeded_snapshot_for_client_seed(
            "insurance-offer",
            "0xabc",
            1,
            "100",
            "0xfeed-insurance-offer",
            no_blackjack_peek_client_seed(),
        );
        let view = super::snapshot_to_view(&snapshot);

        assert!(snapshot.insurance.offered);
        assert!(snapshot.insurance.supported);
        assert_eq!(snapshot.insurance.max_wager, "50");
        assert_eq!(snapshot.insurance.outcome, "offered");
        assert!(!snapshot.insurance.settled);
        assert_eq!(snapshot.phase, "insurance");
        assert_eq!(
            view.allowed_actions,
            vec![
                "take_insurance".to_string(),
                "decline_insurance".to_string()
            ]
        );
        assert_eq!(
            snapshot.transcript_artifact.dealer_peek.proof_mode,
            super::BLACKJACK_PEEK_PROOF_MODE_TARGET
        );
        assert_eq!(
            snapshot.transcript_artifact.dealer_peek.target_proof_mode,
            super::BLACKJACK_PEEK_PROOF_MODE_TARGET
        );
        assert_eq!(
            snapshot.transcript_artifact.dealer_peek.statement_kind,
            "hole_card_not_ten_value"
        );
        assert!(
            snapshot
                .transcript_artifact
                .dealer_peek
                .no_blackjack_proof
                .available
        );
        assert_eq!(
            snapshot
                .transcript_artifact
                .dealer_peek
                .no_blackjack_proof
                .verifier_namespace,
            super::BLACKJACK_PEEK_VERIFIER_NAMESPACE
        );
        assert_eq!(
            snapshot
                .transcript_artifact
                .dealer_peek
                .no_blackjack_proof
                .claim,
            "hole_card_not_ten_value"
        );
        assert_eq!(
            snapshot
                .transcript_artifact
                .dealer_peek
                .no_blackjack_proof
                .statement
                .statement_kind,
            "hole_card_not_ten_value"
        );
        assert!(
            !snapshot
                .transcript_artifact
                .dealer_peek
                .no_blackjack_proof
                .statement_hash
                .is_empty()
        );
        assert!(
            !snapshot
                .transcript_artifact
                .dealer_peek
                .public_inputs_hash
                .is_empty()
        );
        assert!(
            !snapshot
                .transcript_artifact
                .dealer_peek
                .hidden_value_class_commitment
                .is_empty()
        );
        assert!(
            !snapshot
                .transcript_artifact
                .dealer_peek
                .witness_commitment
                .is_empty()
        );
        assert_eq!(view.fairness.insurance_status, "offered");
    }

    #[test]
    fn declining_insurance_allows_player_turn_to_start() {
        let mut snapshot = seeded_snapshot_for_client_seed(
            "insurance-decline",
            "0xabc",
            1,
            "100",
            "0xfeed-insurance-decline",
            no_blackjack_peek_client_seed(),
        );

        apply_action_to_snapshot(&mut snapshot, "decline_insurance").unwrap();

        assert!(snapshot.insurance.settled);
        assert_eq!(snapshot.insurance.outcome, "declined");
        assert_eq!(snapshot.phase, "player_turn");
        assert!(allowed_actions(&snapshot).contains(&"hit".to_string()));
    }

    #[test]
    fn forced_insurance_decline_keeps_dealer_cards_hidden_until_chain_reveal() {
        let snapshot = seeded_snapshot_for_client_seed(
            "insurance-timeout",
            "0xabc",
            1,
            "100",
            "0xfeed-insurance-timeout",
            no_blackjack_peek_client_seed(),
        );

        let (next, plan) = plan_timeout_submission(&snapshot, "force_insurance_decline").unwrap();

        assert_eq!(plan.action, "force_insurance_decline");
        assert!(!plan.should_release_reservation);
        assert!(next.insurance.settled);
        assert_eq!(next.insurance.outcome, "declined");
        assert_eq!(next.phase, "player_turn");
        assert!(
            !next.dealer.cards[1].revealed,
            "timeout handling must not reveal the dealer hole card locally"
        );
    }

    #[test]
    fn forced_stand_moves_to_dealer_turn_without_local_dealer_resolution() {
        let snapshot = manual_player_turn(vec![BlackjackSeatSnapshot {
            seat_index: 0,
            wager: "100".to_string(),
            cards: vec![revealed(10), revealed(8)],
            status: "active".to_string(),
            outcome: None,
            payout: "0".to_string(),
            doubled: false,
            split_depth: 0,
            split_aces: false,
        }]);

        let (next, plan) = plan_timeout_submission(&snapshot, "force_stand").unwrap();

        assert_eq!(plan.action, "force_stand");
        assert!(!plan.should_release_reservation);
        assert_eq!(next.seats[0].status, "standing");
        assert_eq!(next.status, "awaiting_dealer");
        assert_eq!(next.phase, "dealer_turn");
        assert_eq!(next.dealer.cards.len(), 2);
        assert!(
            !next.dealer.cards[1].revealed,
            "forced stand should not pre-resolve the dealer transcript"
        );
        assert_eq!(next.total_payout, "0");
    }

    #[test]
    fn void_timeout_plan_marks_hand_voided_and_releases_reservation() {
        let mut snapshot = manual_player_turn(vec![BlackjackSeatSnapshot {
            seat_index: 0,
            wager: "100".to_string(),
            cards: vec![revealed(10), revealed(8)],
            status: "standing".to_string(),
            outcome: None,
            payout: "0".to_string(),
            doubled: false,
            split_depth: 0,
            split_aces: false,
        }]);
        snapshot.status = "awaiting_dealer".to_string();
        snapshot.phase = "dealer_turn".to_string();

        let (next, plan) = plan_timeout_submission(&snapshot, "void").unwrap();

        assert_eq!(plan.action, "void_expired_hand");
        assert!(plan.should_release_reservation);
        assert_eq!(next.status, "voided");
        assert_eq!(next.phase, "voided");
        assert_eq!(next.total_payout, "0");
    }

    #[test]
    fn taking_insurance_offsets_dealer_blackjack_loss() {
        let mut snapshot = seeded_snapshot_for_client_seed(
            "insurance-win",
            "0xabc",
            1,
            "100",
            "0xfeed-insurance-win",
            dealer_blackjack_client_seed(),
        );

        apply_action_to_snapshot(&mut snapshot, "take_insurance").unwrap();

        assert_eq!(snapshot.phase, "settled");
        assert_eq!(snapshot.insurance.wager, "50");
        assert!(snapshot.insurance.taken);
        assert_eq!(snapshot.insurance.outcome, "dealer_blackjack");
        assert_eq!(snapshot.total_payout, "150");
    }

    #[test]
    fn non_ace_upcard_has_no_insurance_offer() {
        let snapshot = seed_hand_snapshot("no-insurance", "0xabc", 1, "100", "seed-1").unwrap();

        assert!(!snapshot.insurance.offered);
        assert_eq!(snapshot.insurance.outcome, "not_offered");
        assert!(snapshot.insurance.settled);
    }

    #[test]
    fn explicit_client_seed_changes_shuffle_commitment() {
        let baseline = seed_hand_snapshot_with_secret(
            "hand-client-a",
            "0xabc",
            2,
            "100",
            "0xfeed-client",
            "0xbeef-client",
            "0xcafe-client",
            None,
        )
        .unwrap();
        let mixed = seed_hand_snapshot_with_secret(
            "hand-client-a",
            "0xabc",
            2,
            "100",
            "0xfeed-client",
            "0xbeef-client",
            "0xcafe-client",
            Some("424242"),
        )
        .unwrap();

        assert_eq!(mixed.client_seed, "424242");
        assert_ne!(
            baseline.transcript_artifact.deck_commitment_root,
            mixed.transcript_artifact.deck_commitment_root
        );
        assert_eq!(
            mixed.transcript_artifact.entropy_mode,
            "dealer_plus_client_seed"
        );
        assert_ne!(
            baseline.transcript_artifact.encrypted_deck_root,
            mixed.transcript_artifact.encrypted_deck_root
        );
        assert_eq!(
            mixed.transcript_artifact.encryption_scheme,
            super::BLACKJACK_ENCRYPTION_SCHEME_CURRENT
        );
    }

    #[test]
    fn settled_snapshot_exposes_fairness_summary() {
        let snapshot = seed_hand_snapshot_with_secret(
            "hand-5",
            "0xabc",
            2,
            "1000000000000000000",
            "0xfeed-5",
            "0xbeef-5",
            "0xcafe-5",
            Some("12345"),
        )
        .unwrap();
        let settled_snapshot = snapshot_to_settled(snapshot);
        let view = super::snapshot_to_view(&settled_snapshot);
        assert_eq!(
            view.fairness.protocol_mode,
            super::BLACKJACK_PROTOCOL_MODE_CURRENT
        );
        assert_eq!(
            view.fairness.target_protocol_mode,
            super::BLACKJACK_PROTOCOL_MODE_TARGET
        );
        assert_eq!(
            view.fairness.encryption_scheme,
            super::BLACKJACK_ENCRYPTION_SCHEME_CURRENT
        );
        assert_eq!(
            view.fairness.target_encryption_scheme,
            super::BLACKJACK_ENCRYPTION_SCHEME_TARGET
        );
        assert!(view.fairness.reveal_count >= 4);
        assert!(!view.fairness.deck_commitment_root.is_empty());
        assert!(
            settled_snapshot
                .transcript_artifact
                .reveals
                .iter()
                .any(|reveal| reveal.target == "dealer:1" && reveal.deck_index == 3)
        );
    }

    #[test]
    fn fairness_artifact_exposes_encrypted_envelopes_without_revealing_seeds_early() {
        let snapshot = seeded_snapshot_for_client_seed(
            "hand-fairness-artifact",
            "0xabc",
            2,
            "1000000000000000000",
            "0xfeed-artifact",
            no_blackjack_peek_client_seed(),
        );
        let fairness = super::fairness_artifact_view(&snapshot);

        assert_eq!(fairness.hand_id, snapshot.hand_id);
        assert_eq!(fairness.player, snapshot.player);
        assert_eq!(fairness.table_id, snapshot.table_id);
        assert_eq!(fairness.transcript_root, snapshot.transcript_root);
        assert_eq!(
            fairness.encryption_scheme,
            super::BLACKJACK_ENCRYPTION_SCHEME_CURRENT
        );
        assert_eq!(
            fairness.target_encryption_scheme,
            super::BLACKJACK_ENCRYPTION_SCHEME_TARGET
        );
        assert_eq!(
            fairness.dealer_peek.target_proof_kind,
            super::BLACKJACK_PEEK_PROOF_KIND_TARGET
        );
        assert_eq!(
            fairness.dealer_peek.proof_mode,
            super::BLACKJACK_PEEK_PROOF_MODE_TARGET
        );
        assert_eq!(
            fairness.dealer_peek.target_proof_mode,
            super::BLACKJACK_PEEK_PROOF_MODE_TARGET
        );
        assert!(fairness.dealer_peek.no_blackjack_proof.available);
        assert_eq!(
            fairness.dealer_peek.no_blackjack_proof.verifier_namespace,
            super::BLACKJACK_PEEK_VERIFIER_NAMESPACE
        );
        assert_eq!(
            fairness.dealer_peek.no_blackjack_proof.verifier_status,
            super::BLACKJACK_PEEK_PROOF_BINDING_STATUS_VERIFIED
        );
        assert_eq!(
            fairness.dealer_peek.no_blackjack_proof.current_proof_kind,
            super::BLACKJACK_PEEK_PROOF_KIND_TARGET
        );
        assert_eq!(
            fairness.dealer_peek.no_blackjack_proof.target_proof_kind,
            super::BLACKJACK_PEEK_PROOF_KIND_TARGET
        );
        assert_eq!(
            fairness
                .dealer_peek
                .no_blackjack_proof
                .statement
                .encrypted_deck_root,
            fairness.encrypted_deck_root
        );
        assert_eq!(
            fairness
                .dealer_peek
                .no_blackjack_proof
                .statement
                .statement_kind,
            fairness.dealer_peek.statement_kind
        );
        assert!(
            !fairness
                .dealer_peek
                .no_blackjack_proof
                .statement_hash
                .is_empty()
        );
        assert!(matches!(
            fairness.dealer_peek.statement_kind.as_str(),
            "hole_card_not_ten_value" | "hole_card_not_ace"
        ));
        assert!(!fairness.dealer_peek.public_inputs_hash.is_empty());
        assert_eq!(
            fairness.dealer_peek.no_blackjack_proof.receipt.proof_kind,
            super::BLACKJACK_PEEK_PROOF_KIND_TARGET
        );
        assert!(fairness.dealer_peek.no_blackjack_proof.receipt.verified);
        assert!(
            fairness
                .dealer_peek
                .no_blackjack_proof
                .zk_proof_target
                .available
        );
        assert_eq!(
            fairness
                .dealer_peek
                .no_blackjack_proof
                .zk_proof_target
                .circuit_id,
            super::BLACKJACK_PEEK_CIRCUIT_ID_TARGET
        );
        assert_eq!(
            fairness
                .dealer_peek
                .no_blackjack_proof
                .zk_proof_target
                .verification_key_id,
            super::BLACKJACK_PEEK_VERIFICATION_KEY_ID_TARGET
        );
        assert!(
            !fairness
                .dealer_peek
                .no_blackjack_proof
                .zk_proof_target
                .request_id
                .is_empty()
        );
        assert!(
            !fairness
                .dealer_peek
                .no_blackjack_proof
                .proof_binding
                .proof_id
                .is_empty()
        );
        assert!(
            !fairness
                .dealer_peek
                .hidden_value_class_commitment
                .is_empty()
        );
        assert!(!fairness.dealer_peek.witness_commitment.is_empty());
        assert_eq!(
            fairness.dealer_peek.opening.root,
            fairness.encrypted_deck_root
        );
        assert!(fairness.dealer_peek.opening.verified);
        assert_eq!(
            fairness.encrypted_cards.len(),
            snapshot.transcript_artifact.cards.len()
        );
        assert!(fairness.server_seed.is_none());
        assert!(fairness.client_seed.is_none());
        assert!(
            fairness
                .encrypted_cards
                .iter()
                .all(|card| !card.ciphertext.is_empty()
                    && !card.nonce_commitment.is_empty()
                    && !card.reveal_key_commitment.is_empty())
        );
        assert!(fairness.reveals.iter().all(|reveal| {
            reveal.proof_kind == super::BLACKJACK_REVEAL_PROOF_KIND_ENVELOPE
                && reveal.opening.verified
                && reveal.opening.root == fairness.encrypted_deck_root
        }));
        assert_eq!(fairness.audit.mode, "public_artifact_consistency_v2");
        assert!(fairness.audit.passed);
        assert!(fairness.audit.reveal_openings_verified);
        assert!(fairness.audit.dealer_peek_opening_verified);
        assert!(fairness.audit.dealer_peek_statement_hash_verified);
        assert!(fairness.audit.dealer_peek_public_inputs_hash_verified);
        assert!(fairness.audit.dealer_peek_artifact_consistent);
        assert!(fairness.audit.dealer_peek_zk_target_consistent);
        assert!(fairness.audit.dealer_peek_proof_binding_verified);
        assert!(fairness.audit.settlement_redaction_respected);
        assert!(fairness.audit.issues.is_empty());
    }

    #[test]
    fn fairness_audit_detects_tampered_no_blackjack_statement() {
        let snapshot = seeded_snapshot_for_client_seed(
            "hand-fairness-tamper",
            "0xabc",
            2,
            "1000000000000000000",
            "0xfeed-tamper",
            no_blackjack_peek_client_seed(),
        );
        let mut fairness = super::fairness_artifact_view(&snapshot);
        fairness.dealer_peek.no_blackjack_proof.statement.table_id += 1;
        fairness.audit = super::audit_fairness_artifact_view(&fairness);

        assert!(!fairness.audit.passed);
        assert!(!fairness.audit.dealer_peek_statement_hash_verified);
        assert!(
            fairness
                .audit
                .issues
                .iter()
                .any(|issue| issue == "dealer_peek_statement_hash_mismatch")
        );
    }

    #[test]
    fn fairness_audit_detects_tampered_no_blackjack_proof_binding() {
        let snapshot = seeded_snapshot_for_client_seed(
            "hand-fairness-proof-binding-tamper",
            "0xabc",
            2,
            "1000000000000000000",
            "0xfeed-proof-binding-tamper",
            no_blackjack_peek_client_seed(),
        );
        let mut fairness = super::fairness_artifact_view(&snapshot);
        fairness
            .dealer_peek
            .no_blackjack_proof
            .proof_binding
            .proof_id = "0xbad".to_string();
        fairness.audit = super::audit_fairness_artifact_view(&fairness);

        assert!(!fairness.audit.passed);
        assert!(!fairness.audit.dealer_peek_proof_binding_verified);
        assert!(
            fairness
                .audit
                .issues
                .iter()
                .any(|issue| issue == "dealer_peek_proof_binding_invalid")
        );
    }

    #[test]
    fn fairness_audit_detects_tampered_no_blackjack_zk_target() {
        let snapshot = seeded_snapshot_for_client_seed(
            "hand-fairness-zk-target-tamper",
            "0xabc",
            2,
            "1000000000000000000",
            "0xfeed-zk-target-tamper",
            no_blackjack_peek_client_seed(),
        );
        let mut fairness = super::fairness_artifact_view(&snapshot);
        fairness
            .dealer_peek
            .no_blackjack_proof
            .zk_proof_target
            .verification_key_id = "wrong_vk".to_string();
        fairness.audit = super::audit_fairness_artifact_view(&fairness);

        assert!(!fairness.audit.passed);
        assert!(!fairness.audit.dealer_peek_zk_target_consistent);
        assert!(
            fairness
                .audit
                .issues
                .iter()
                .any(|issue| issue == "dealer_peek_zk_target_invalid")
        );
    }

    #[test]
    fn reconcile_rejects_chain_cards_that_do_not_match_committed_transcript() {
        let snapshot = seed_hand_snapshot("hand-3", "0xabc", 2, "100", "0xfeed-3").unwrap();
        let chain = BlackjackChainHand {
            hand_id: 1,
            player: snapshot.player.clone(),
            table_id: snapshot.table_id,
            wager: snapshot.wager.clone(),
            status: "active".to_string(),
            phase: "player_turn".to_string(),
            transcript_root: snapshot.transcript_root.clone(),
            active_seat: 0,
            seat_count: 1,
            action_count: 0,
            split_count: 0,
            dealer_cards: vec![snapshot.dealer.cards[0].rank],
            seats: vec![BlackjackChainSeat {
                seat_index: 0,
                wager: snapshot.wager.clone(),
                status: "active".to_string(),
                outcome: None,
                payout: "0".to_string(),
                doubled: false,
                cards: vec![snapshot.seats[0].cards[0].rank, 13],
            }],
            total_payout: "0".to_string(),
        };

        let error = reconcile_view_with_chain(&snapshot, &chain).unwrap_err();
        assert!(error.to_string().contains("committed transcript"));
    }

    #[test]
    fn reconcile_preserves_view_when_chain_context_matches() {
        let mut snapshot =
            seed_hand_snapshot("hand-digest-mismatch", "0xabc", 2, "100", "0xfeed-digest").unwrap();
        snapshot.transcript_artifact.dealer_peek.required = true;
        snapshot.transcript_artifact.dealer_peek.outcome = "continue".to_string();
        snapshot
            .transcript_artifact
            .dealer_peek
            .no_blackjack_proof
            .proof_binding
            .proof_id = "0x1234".to_string();
        let chain = BlackjackChainHand {
            hand_id: 9,
            player: snapshot.player.clone(),
            table_id: snapshot.table_id,
            wager: snapshot.wager.clone(),
            status: "active".to_string(),
            phase: "player_turn".to_string(),
            transcript_root: snapshot.transcript_root.clone(),
            active_seat: 0,
            seat_count: 1,
            action_count: 0,
            split_count: 0,
            dealer_cards: vec![snapshot.dealer.cards[0].rank],
            seats: vec![BlackjackChainSeat {
                seat_index: 0,
                wager: snapshot.wager.clone(),
                status: "active".to_string(),
                outcome: None,
                payout: "0".to_string(),
                doubled: false,
                cards: snapshot.seats[0]
                    .cards
                    .iter()
                    .map(|card| card.rank)
                    .collect(),
            }],
            total_payout: "0".to_string(),
        };

        let view = reconcile_view_with_chain(&snapshot, &chain).unwrap();
        assert_eq!(view.hand_id, snapshot.hand_id);
    }

    #[test]
    fn reconcile_marks_settled_revealed_hand_as_verified() {
        let snapshot = seed_hand_snapshot_with_secret(
            "hand-4",
            "0xabc",
            2,
            "1000000000000000000",
            "0xfeed-4",
            "0xbeef-4",
            "0xcafe-4",
            Some("999"),
        )
        .unwrap();
        let settled_snapshot = snapshot_to_settled(snapshot);
        let chain = BlackjackChainHand {
            hand_id: 4,
            player: settled_snapshot.player.clone(),
            table_id: settled_snapshot.table_id,
            wager: settled_snapshot.wager.clone(),
            status: "settled".to_string(),
            phase: "settled".to_string(),
            transcript_root: settled_snapshot.transcript_root.clone(),
            active_seat: settled_snapshot.active_seat,
            seat_count: settled_snapshot.seat_count,
            action_count: settled_snapshot.action_count,
            split_count: settled_snapshot.split_count,
            dealer_cards: settled_snapshot
                .dealer
                .cards
                .iter()
                .map(|card| card.rank)
                .collect(),
            seats: settled_snapshot
                .seats
                .iter()
                .map(|seat| BlackjackChainSeat {
                    seat_index: seat.seat_index,
                    wager: seat.wager.clone(),
                    status: seat.status.clone(),
                    outcome: seat.outcome.clone(),
                    payout: seat.payout.clone(),
                    doubled: seat.doubled,
                    cards: seat.cards.iter().map(|card| card.rank).collect(),
                })
                .collect(),
            total_payout: settled_snapshot.total_payout.clone(),
        };

        let view = reconcile_view_with_chain(&settled_snapshot, &chain).unwrap();
        assert!(view.proof_verified);
    }

    fn revealed(rank: u8) -> BlackjackCardSnapshot {
        BlackjackCardSnapshot {
            rank,
            revealed: true,
        }
    }

    fn pair_seat(seat_index: u8) -> BlackjackSeatSnapshot {
        BlackjackSeatSnapshot {
            seat_index,
            wager: "100".to_string(),
            cards: vec![revealed(8), revealed(8)],
            status: "active".to_string(),
            outcome: None,
            payout: "0".to_string(),
            doubled: false,
            split_depth: 0,
            split_aces: false,
        }
    }

    fn manual_player_turn(seats: Vec<BlackjackSeatSnapshot>) -> BlackjackHandSnapshot {
        BlackjackHandSnapshot {
            hand_id: "manual".to_string(),
            player: "0xabc".to_string(),
            table_id: 1,
            wager: "100".to_string(),
            transcript_root: "0xroot".to_string(),
            server_seed_hash: "0xhash".to_string(),
            server_seed: "0xseed".to_string(),
            client_seed: String::new(),
            status: "active".to_string(),
            phase: "player_turn".to_string(),
            active_seat: 0,
            seat_count: seats.len() as u8,
            action_count: 0,
            split_count: 0,
            total_payout: "0".to_string(),
            dealer: BlackjackDealerSnapshot {
                cards: vec![
                    revealed(6),
                    BlackjackCardSnapshot {
                        rank: 10,
                        revealed: false,
                    },
                ],
            },
            seats,
            action_log: Vec::new(),
            shoe: vec![
                test_card_id(2, 0),
                test_card_id(3, 0),
                test_card_id(4, 0),
                test_card_id(5, 0),
                test_card_id(6, 0),
                test_card_id(7, 0),
                test_card_id(8, 0),
                test_card_id(9, 0),
            ],
            next_card_index: 0,
            insurance: BlackjackInsuranceState::default(),
            transcript_artifact: super::BlackjackTranscriptArtifact::default(),
        }
    }

    fn snapshot_to_settled(mut snapshot: BlackjackHandSnapshot) -> BlackjackHandSnapshot {
        while snapshot.phase != "settled" {
            let actions = allowed_actions(&snapshot);
            if actions.iter().any(|action| action == "stand") {
                apply_action_to_snapshot(&mut snapshot, "stand").unwrap();
            } else if actions.iter().any(|action| action == "hit") {
                apply_action_to_snapshot(&mut snapshot, "hit").unwrap();
            } else {
                break;
            }
        }
        snapshot
    }
}
