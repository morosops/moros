use core::panic_with_felt252;
use garaga::hashes::poseidon_bn254::poseidon_hash_2;
use moros_contracts::blackjack_peek_verifier::groth16_verifier::{
    IGroth16VerifierBN254DispatcherTrait, IGroth16VerifierBN254LibraryDispatcher,
};
use moros_contracts::interfaces::{
    IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IBlackjackTableDispatcher,
    IBlackjackTableDispatcherTrait, IDeckCommitmentDispatcher, IDeckCommitmentDispatcherTrait,
    IERC20Dispatcher, IERC20DispatcherTrait, IMockDealerPeekGroth16VerifierDispatcher,
    IMockDealerPeekGroth16VerifierDispatcherTrait, ISessionRegistryDispatcher,
    ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
};
use moros_contracts::types::{BlackjackCardRevealProof, GameKind, HandOutcome, HandStatus};
use snforge_std::fs::{FileTrait, read_txt};
use snforge_std::{
    ContractClassTrait, DeclareResultTrait, declare, start_cheat_block_number,
    start_cheat_caller_address, stop_cheat_block_number, stop_cheat_caller_address,
};
use starknet::{ClassHash, ContractAddress};

const TABLE_ID: u64 = 2;
const SESSION_EXPIRY: u64 = 9_999_999;
const ONE_STRK: u128 = 1_000_000_000_000_000_000;
const TABLE_MIN_WAGER: u128 = ONE_STRK;
const TABLE_MAX_WAGER: u128 = 100_u128 * ONE_STRK;
const DEFAULT_WAGER: u128 = 10_u128 * ONE_STRK;
const STARTING_BANKROLL: u128 = 1_000_u128 * ONE_STRK;
const HOUSE_LIQUIDITY: u128 = 10_000_u128 * ONE_STRK;
const INITIAL_SUPPLY: u128 = 100_000_u128 * ONE_STRK;
const BLACKJACK_TIMEOUT_BLOCKS: u64 = 50;

fn card_id_for_rank_variant(rank: u8, variant_index: u16) -> u16 {
    ((variant_index * 13_u16) + (rank.into() - 1_u16))
}

#[derive(Copy, Drop)]
struct VerifiedDeckFixture {
    root: u256,
    player_first_proof: BlackjackCardRevealProof,
    dealer_upcard_proof: BlackjackCardRevealProof,
    player_second_proof: BlackjackCardRevealProof,
    dealer_hole_proof: BlackjackCardRevealProof,
}

#[derive(Copy, Drop)]
struct DeployedStack {
    token: IERC20Dispatcher,
    token_address: ContractAddress,
    vault: IBankrollVaultDispatcher,
    vault_address: ContractAddress,
    table_registry: ITableRegistryDispatcher,
    table_registry_address: ContractAddress,
    sessions: ISessionRegistryDispatcher,
    sessions_address: ContractAddress,
    commitment: IDeckCommitmentDispatcher,
    commitment_address: ContractAddress,
    mock_verifier: IMockDealerPeekGroth16VerifierDispatcher,
    mock_verifier_address: ContractAddress,
    blackjack: IBlackjackTableDispatcher,
    blackjack_address: ContractAddress,
}

fn owner() -> ContractAddress {
    0x111.try_into().unwrap()
}

fn player() -> ContractAddress {
    0x222.try_into().unwrap()
}

fn session_key() -> ContractAddress {
    0x333.try_into().unwrap()
}

fn u256_zero() -> u256 {
    u256 { low: 0, high: 0 }
}

fn u256_from_u64(value: u64) -> u256 {
    u256 { low: value.into(), high: 0 }
}

fn u256_from_u8(value: u8) -> u256 {
    u256 { low: value.into(), high: 0 }
}

fn u256_from_u16(value: u16) -> u256 {
    u256 { low: value.into(), high: 0 }
}

fn u256_from_u128(value: u128) -> u256 {
    u256 { low: value, high: 0 }
}

fn u256_words(low: u128, high: u128) -> u256 {
    u256 { low, high }
}

fn card_leaf(card_id: u16, salt: u256) -> u256 {
    poseidon_hash_2(u256_from_u16(card_id), salt)
}

fn zero_subtree(level: u8) -> u256 {
    let mut value = u256_zero();
    let mut cursor = 0_u8;
    loop {
        if cursor >= level {
            break;
        }
        value = poseidon_hash_2(value, value);
        cursor += 1_u8;
    }
    value
}

fn merkle_root_for_first_four(leaf_0: u256, leaf_1: u256, leaf_2: u256, leaf_3: u256) -> u256 {
    let node_01 = poseidon_hash_2(leaf_0, leaf_1);
    let node_23 = poseidon_hash_2(leaf_2, leaf_3);
    let mut root = poseidon_hash_2(poseidon_hash_2(node_01, node_23), zero_subtree(2_u8));
    let mut level = 3_u8;
    loop {
        if level >= 9_u8 {
            break;
        }
        root = poseidon_hash_2(root, zero_subtree(level));
        level += 1_u8;
    }
    root
}

fn proof_for_first_four(
    deck_index: u64,
    card_id: u16,
    salt: u256,
    leaf_0: u256,
    leaf_1: u256,
    leaf_2: u256,
    leaf_3: u256,
) -> BlackjackCardRevealProof {
    let node_01 = poseidon_hash_2(leaf_0, leaf_1);
    let node_23 = poseidon_hash_2(leaf_2, leaf_3);
    let sibling_0 = if deck_index == 0_u64 {
        leaf_1
    } else if deck_index == 1_u64 {
        leaf_0
    } else if deck_index == 2_u64 {
        leaf_3
    } else {
        leaf_2
    };
    let sibling_1 = if deck_index < 2_u64 {
        node_23
    } else {
        node_01
    };
    BlackjackCardRevealProof {
        deck_index,
        card_id,
        salt,
        sibling_0,
        sibling_1,
        sibling_2: zero_subtree(2_u8),
        sibling_3: zero_subtree(3_u8),
        sibling_4: zero_subtree(4_u8),
        sibling_5: zero_subtree(5_u8),
        sibling_6: zero_subtree(6_u8),
        sibling_7: zero_subtree(7_u8),
        sibling_8: zero_subtree(8_u8),
    }
}

fn verified_deck_fixture(
    player_first_card: u8, dealer_upcard: u8, player_second_card: u8, dealer_hole_card: u8,
) -> VerifiedDeckFixture {
    let salt_0 = u256_from_u64(0x7001);
    let salt_1 = u256_from_u64(0x7002);
    let salt_2 = u256_from_u64(0x7003);
    let salt_3 = u256_from_u64(0x7004);
    let card_id_0 = card_id_for_rank_variant(player_first_card, 0_u16);
    let card_id_1 = card_id_for_rank_variant(dealer_upcard, 1_u16);
    let card_id_2 = card_id_for_rank_variant(player_second_card, 2_u16);
    let card_id_3 = card_id_for_rank_variant(dealer_hole_card, 3_u16);
    let leaf_0 = card_leaf(card_id_0, salt_0);
    let leaf_1 = card_leaf(card_id_1, salt_1);
    let leaf_2 = card_leaf(card_id_2, salt_2);
    let leaf_3 = card_leaf(card_id_3, salt_3);
    VerifiedDeckFixture {
        root: merkle_root_for_first_four(leaf_0, leaf_1, leaf_2, leaf_3),
        player_first_proof: proof_for_first_four(
            0_u64, card_id_0, salt_0, leaf_0, leaf_1, leaf_2, leaf_3,
        ),
        dealer_upcard_proof: proof_for_first_four(
            1_u64, card_id_1, salt_1, leaf_0, leaf_1, leaf_2, leaf_3,
        ),
        player_second_proof: proof_for_first_four(
            2_u64, card_id_2, salt_2, leaf_0, leaf_1, leaf_2, leaf_3,
        ),
        dealer_hole_proof: proof_for_first_four(
            3_u64, card_id_3, salt_3, leaf_0, leaf_1, leaf_2, leaf_3,
        ),
    }
}

fn deploy_stack() -> DeployedStack {
    let token_class = declare("MockStrkToken").unwrap().contract_class();
    let (token_address, _) = token_class
        .deploy(@array![owner().into(), INITIAL_SUPPLY.into(), 'TSTRK'.into(), 'STRK'.into()])
        .unwrap();
    let token = IERC20Dispatcher { contract_address: token_address };

    let vault_class = declare("BankrollVault").unwrap().contract_class();
    let (vault_address, _) = vault_class
        .deploy(@array![owner().into(), token_address.into()])
        .unwrap();
    let vault = IBankrollVaultDispatcher { contract_address: vault_address };

    let registry_class = declare("TableRegistry").unwrap().contract_class();
    let (table_registry_address, _) = registry_class.deploy(@array![owner().into()]).unwrap();
    let table_registry = ITableRegistryDispatcher { contract_address: table_registry_address };

    let sessions_class = declare("SessionRegistry").unwrap().contract_class();
    let (sessions_address, _) = sessions_class.deploy(@array![owner().into()]).unwrap();
    let sessions = ISessionRegistryDispatcher { contract_address: sessions_address };

    let commitment_class = declare("DeckCommitment").unwrap().contract_class();
    let (commitment_address, _) = commitment_class.deploy(@array![owner().into()]).unwrap();
    let commitment = IDeckCommitmentDispatcher { contract_address: commitment_address };

    let mock_verifier_class = declare("MockGroth16Verifier").unwrap().contract_class();
    let (mock_verifier_address, _) = mock_verifier_class.deploy(@array![]).unwrap();
    let mock_verifier = IMockDealerPeekGroth16VerifierDispatcher {
        contract_address: mock_verifier_address,
    };

    let blackjack_class = declare("BlackjackTable").unwrap().contract_class();
    let (blackjack_address, _) = blackjack_class
        .deploy(
            @array![
                owner().into(), vault_address.into(), table_registry_address.into(),
                sessions_address.into(), commitment_address.into(), mock_verifier_address.into(),
            ],
        )
        .unwrap();
    let blackjack = IBlackjackTableDispatcher { contract_address: blackjack_address };

    start_cheat_caller_address(vault_address, owner());
    vault.set_operator(blackjack_address, true);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(commitment_address, owner());
    commitment.set_operator(blackjack_address, true);
    stop_cheat_caller_address(commitment_address);

    start_cheat_caller_address(table_registry_address, owner());
    table_registry
        .register_table(
            TABLE_ID, blackjack_address, GameKind::Blackjack, TABLE_MIN_WAGER, TABLE_MAX_WAGER,
        );
    stop_cheat_caller_address(table_registry_address);

    start_cheat_caller_address(token_address, owner());
    token.transfer(player(), STARTING_BANKROLL.into());
    token.approve(vault_address, HOUSE_LIQUIDITY.into());
    stop_cheat_caller_address(token_address);

    start_cheat_caller_address(vault_address, owner());
    vault.deposit_house_liquidity(HOUSE_LIQUIDITY);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(token_address, player());
    token.approve(vault_address, STARTING_BANKROLL.into());
    stop_cheat_caller_address(token_address);

    start_cheat_caller_address(vault_address, player());
    vault.deposit_public(player(), STARTING_BANKROLL);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(sessions_address, player());
    sessions.register_session_key(player(), session_key(), TABLE_MAX_WAGER, SESSION_EXPIRY);
    stop_cheat_caller_address(sessions_address);

    start_cheat_caller_address(sessions_address, player());
    sessions.register_session_key(player(), owner(), TABLE_MAX_WAGER, SESSION_EXPIRY);
    stop_cheat_caller_address(sessions_address);

    DeployedStack {
        token,
        token_address,
        vault,
        vault_address,
        table_registry,
        table_registry_address,
        sessions,
        sessions_address,
        commitment,
        commitment_address,
        mock_verifier,
        mock_verifier_address,
        blackjack,
        blackjack_address,
    }
}

fn declare_real_verifier() -> ClassHash {
    *declare("Groth16VerifierBN254").unwrap().contract_class().class_hash
}

fn is_peek_upcard(dealer_upcard: u8) -> bool {
    dealer_upcard == 1_u8
        || dealer_upcard == 10_u8
        || dealer_upcard == 11_u8
        || dealer_upcard == 12_u8
        || dealer_upcard == 13_u8
}

fn upcard_class(dealer_upcard: u8) -> u8 {
    if dealer_upcard == 1_u8 {
        1_u8
    } else {
        2_u8
    }
}

fn dealer_peek_hand_hash(
    hand_id: u64, transcript_root: u256, dealer_upcard: u8, first_card: u8, second_card: u8,
) -> u256 {
    let mut hash = poseidon_hash_2(u256_from_u64(hand_id), u256_from_u64(TABLE_ID));
    hash = poseidon_hash_2(hash, u256_from_u128(DEFAULT_WAGER));
    hash = poseidon_hash_2(hash, transcript_root);
    hash = poseidon_hash_2(hash, u256_from_u8(dealer_upcard));
    hash = poseidon_hash_2(hash, u256_from_u8(first_card));
    hash = poseidon_hash_2(hash, u256_from_u8(second_card));
    hash
}

fn configure_mock_verifier(
    stack: DeployedStack,
    hand_id: u64,
    transcript_root: u256,
    dealer_upcard: u8,
    first_card: u8,
    second_card: u8,
    dealer_blackjack: bool,
) {
    start_cheat_caller_address(stack.mock_verifier_address, owner());
    stack.mock_verifier.set_valid(true);
    stack
        .mock_verifier
        .set_public_inputs(
            dealer_peek_hand_hash(hand_id, transcript_root, dealer_upcard, first_card, second_card),
            transcript_root,
            u256_from_u64(3_u64),
            u256_from_u8(upcard_class(dealer_upcard)),
            if dealer_blackjack {
                u256_from_u8(1_u8)
            } else {
                u256_zero()
            },
            u256_from_u64(hand_id),
            u256_from_u64(TABLE_ID),
            u256_from_u128(DEFAULT_WAGER),
            u256_from_u8(dealer_upcard),
            u256_from_u8(first_card),
            u256_from_u8(second_card),
        );
    stop_cheat_caller_address(stack.mock_verifier_address);
}

fn precommit_hand(
    stack: DeployedStack,
    hand_id: u64,
    transcript_root: u256,
    dealer_upcard: u8,
    dealer_blackjack: bool,
) {
    start_cheat_caller_address(stack.commitment_address, owner());
    stack
        .commitment
        .post_hand_commitment(
            hand_id,
            TABLE_ID,
            transcript_root,
            BLACKJACK_TIMEOUT_BLOCKS,
            is_peek_upcard(dealer_upcard),
            dealer_blackjack,
        );
    stop_cheat_caller_address(stack.commitment_address);
}

fn open_hand_verified_raw(
    stack: DeployedStack,
    fixture: VerifiedDeckFixture,
    dealer_upcard: u8,
    first_card: u8,
    second_card: u8,
    dealer_peek_proof: Span<felt252>,
) -> u64 {
    start_cheat_caller_address(stack.blackjack_address, session_key());
    let opened_hand_id = stack
        .blackjack
        .open_hand_verified(
            TABLE_ID,
            player(),
            DEFAULT_WAGER,
            fixture.root,
            dealer_upcard,
            fixture.dealer_upcard_proof,
            first_card,
            fixture.player_first_proof,
            second_card,
            fixture.player_second_proof,
            dealer_peek_proof,
        );
    stop_cheat_caller_address(stack.blackjack_address);
    opened_hand_id
}

fn open_hand_verified_as_player_raw(
    stack: DeployedStack,
    fixture: VerifiedDeckFixture,
    dealer_upcard: u8,
    first_card: u8,
    second_card: u8,
    dealer_peek_proof: Span<felt252>,
) -> u64 {
    start_cheat_caller_address(stack.blackjack_address, player());
    let opened_hand_id = stack
        .blackjack
        .open_hand_verified(
            TABLE_ID,
            player(),
            DEFAULT_WAGER,
            fixture.root,
            dealer_upcard,
            fixture.dealer_upcard_proof,
            first_card,
            fixture.player_first_proof,
            second_card,
            fixture.player_second_proof,
            dealer_peek_proof,
        );
    stop_cheat_caller_address(stack.blackjack_address);
    opened_hand_id
}

fn open_hand_verified_raw_with_wager(
    stack: DeployedStack,
    fixture: VerifiedDeckFixture,
    wager: u128,
    dealer_upcard: u8,
    first_card: u8,
    second_card: u8,
    dealer_peek_proof: Span<felt252>,
) -> u64 {
    start_cheat_caller_address(stack.blackjack_address, session_key());
    let opened_hand_id = stack
        .blackjack
        .open_hand_verified(
            TABLE_ID,
            player(),
            wager,
            fixture.root,
            dealer_upcard,
            fixture.dealer_upcard_proof,
            first_card,
            fixture.player_first_proof,
            second_card,
            fixture.player_second_proof,
            dealer_peek_proof,
        );
    stop_cheat_caller_address(stack.blackjack_address);
    opened_hand_id
}

fn open_hand_with_fixture(
    stack: DeployedStack, dealer_upcard: u8, first_card: u8, second_card: u8, dealer_hole_card: u8,
) -> (u64, VerifiedDeckFixture) {
    let fixture = verified_deck_fixture(first_card, dealer_upcard, second_card, dealer_hole_card);
    let hand_id = stack.blackjack.peek_next_hand_id();
    let dealer_blackjack = if dealer_upcard == 1_u8 {
        dealer_hole_card >= 10_u8
    } else if is_peek_upcard(dealer_upcard) {
        dealer_hole_card == 1_u8
    } else {
        false
    };
    precommit_hand(stack, hand_id, fixture.root, dealer_upcard, dealer_blackjack);
    let mut dealer_peek_proof: Array<felt252> = array![];
    if is_peek_upcard(dealer_upcard) {
        configure_mock_verifier(
            stack, hand_id, fixture.root, dealer_upcard, first_card, second_card, dealer_blackjack,
        );
        dealer_peek_proof.append(1);
    }
    let opened_hand_id = open_hand_verified_raw(
        stack, fixture, dealer_upcard, first_card, second_card, dealer_peek_proof.span(),
    );
    (opened_hand_id, fixture)
}

fn open_hand(stack: DeployedStack, dealer_upcard: u8, first_card: u8, second_card: u8) -> u64 {
    let (hand_id, _) = open_hand_with_fixture(stack, dealer_upcard, first_card, second_card, 9_u8);
    hand_id
}

fn open_hand_with_dealer_blackjack(
    stack: DeployedStack, dealer_upcard: u8, first_card: u8, second_card: u8,
) -> (u64, VerifiedDeckFixture) {
    let dealer_hole = if dealer_upcard == 1_u8 {
        10_u8
    } else {
        1_u8
    };
    open_hand_with_fixture(stack, dealer_upcard, first_card, second_card, dealer_hole)
}

fn load_real_peek_proof() -> Array<felt252> {
    let file = FileTrait::new("tests/fixtures/blackjack_peek_proof_calldata.txt");
    read_txt(@file)
}

#[test]
fn test_verified_card_reveal_path_binds_dealer_hole_card_to_commitment() {
    let stack = deploy_stack();
    let (hand_id, fixture) = open_hand_with_fixture(stack, 9_u8, 10_u8, 7_u8, 8_u8);

    start_cheat_caller_address(stack.blackjack_address, player());
    stack.blackjack.submit_stand(player(), hand_id, 0_u8);
    stop_cheat_caller_address(stack.blackjack_address);

    start_cheat_caller_address(stack.blackjack_address, session_key());
    stack.blackjack.reveal_dealer_card_verified(hand_id, 8_u8, fixture.dealer_hole_proof);
    stack.blackjack.finalize_hand(hand_id);
    stop_cheat_caller_address(stack.blackjack_address);

    let hand = stack.blackjack.get_hand(hand_id);
    let seat = stack.blackjack.get_seat(hand_id, 0_u8);
    assert(hand.status == HandStatus::Settled, 'HAND_SETTLED');
    assert(hand.transcript_root == fixture.root, 'ROOT_BOUND');
    assert(stack.blackjack.get_dealer_card(hand_id, 1_u8) == 8_u8, 'HOLE_REVEALED');
    assert(seat.outcome == HandOutcome::Push, 'PUSH_EXPECTED');
    assert(stack.vault.balance_of(player()) == STARTING_BANKROLL, 'PLAYER_BALANCE');
}

#[test]
#[should_panic(expected: 'CARD_ID_MISMATCH')]
fn test_verified_open_rejects_rank_not_matching_commitment() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(10_u8, 9_u8, 7_u8, 8_u8);
    precommit_hand(stack, stack.blackjack.peek_next_hand_id(), fixture.root, 9_u8, false);
    let empty_proof: Array<felt252> = array![];
    open_hand_verified_raw(stack, fixture, 9_u8, 10_u8, 8_u8, empty_proof.span());
}

#[test]
#[should_panic(expected: 'PEEK_PROOF_REQUIRED')]
fn test_peek_upcard_requires_non_empty_groth16_proof() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(10_u8, 1_u8, 7_u8, 9_u8);
    precommit_hand(stack, stack.blackjack.peek_next_hand_id(), fixture.root, 1_u8, false);
    let empty_proof: Array<felt252> = array![];
    open_hand_verified_raw(stack, fixture, 1_u8, 10_u8, 7_u8, empty_proof.span());
}

#[test]
#[should_panic(expected: 'PEEK_PROOF_INVALID')]
fn test_invalid_mock_peek_proof_fails_blackjack_open() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(10_u8, 1_u8, 7_u8, 9_u8);
    let hand_id = stack.blackjack.peek_next_hand_id();
    precommit_hand(stack, hand_id, fixture.root, 1_u8, false);
    configure_mock_verifier(stack, hand_id, fixture.root, 1_u8, 10_u8, 7_u8, false);
    start_cheat_caller_address(stack.mock_verifier_address, owner());
    stack.mock_verifier.set_valid(false);
    stop_cheat_caller_address(stack.mock_verifier_address);
    let proof: Array<felt252> = array![1];
    open_hand_verified_raw(stack, fixture, 1_u8, 10_u8, 7_u8, proof.span());
}

#[test]
#[should_panic(expected: 'PEEK_HAND_HASH')]
fn test_stale_peek_proof_cannot_be_reused_for_next_hand() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(10_u8, 1_u8, 7_u8, 9_u8);
    let proof: Array<felt252> = array![1];

    let first_hand_id = stack.blackjack.peek_next_hand_id();
    precommit_hand(stack, first_hand_id, fixture.root, 1_u8, false);
    configure_mock_verifier(stack, first_hand_id, fixture.root, 1_u8, 10_u8, 7_u8, false);
    let _ = open_hand_verified_raw(stack, fixture, 1_u8, 10_u8, 7_u8, proof.span());

    precommit_hand(stack, stack.blackjack.peek_next_hand_id(), fixture.root, 1_u8, false);
    let _ = open_hand_verified_raw(stack, fixture, 1_u8, 10_u8, 7_u8, proof.span());
}

#[test]
fn test_ace_upcard_opens_insurance_window_from_peek_result() {
    let stack = deploy_stack();
    let hand_id = open_hand(stack, 1_u8, 10_u8, 7_u8);

    let hand = stack.blackjack.get_hand(hand_id);
    assert(hand.status == HandStatus::AwaitingInsurance, 'INSURANCE_OPEN');

    start_cheat_caller_address(stack.blackjack_address, player());
    stack.blackjack.submit_decline_insurance(player(), hand_id, false);
    stop_cheat_caller_address(stack.blackjack_address);

    let after_decline = stack.blackjack.get_hand(hand_id);
    assert(after_decline.status == HandStatus::Active, 'ACTIVE_AFTER_DECLINE');
}

#[test]
#[should_panic(expected: 'SESSION_DENIED')]
fn test_session_max_wager_caps_blackjack_insurance_exposure() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.sessions_address, player());
    stack.sessions.register_session_key(player(), session_key(), DEFAULT_WAGER, SESSION_EXPIRY);
    stop_cheat_caller_address(stack.sessions_address);

    let hand_id = open_hand(stack, 1_u8, 10_u8, 7_u8);

    start_cheat_caller_address(stack.blackjack_address, session_key());
    stack.blackjack.submit_take_insurance(player(), hand_id, false);
    stop_cheat_caller_address(stack.blackjack_address);
}

#[test]
#[should_panic(expected: 'DECK_COMMIT_REQUIRED')]
fn test_player_cannot_self_supply_blackjack_transcript() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(1_u8, 9_u8, 13_u8, 8_u8);
    let empty_peek_proof: Array<felt252> = array![];

    open_hand_verified_as_player_raw(stack, fixture, 9_u8, 1_u8, 13_u8, empty_peek_proof.span());
}

#[test]
#[should_panic(expected: ('HOUSE_EXPOSURE_CAP',))]
fn test_blackjack_rejects_hand_above_dynamic_house_exposure_cap() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(10_u8, 9_u8, 7_u8, 8_u8);
    let hand_id = stack.blackjack.peek_next_hand_id();
    let empty_peek_proof: Array<felt252> = array![];
    precommit_hand(stack, hand_id, fixture.root, 9_u8, false);

    open_hand_verified_raw_with_wager(
        stack, fixture, 13_u128 * ONE_STRK, 9_u8, 10_u8, 7_u8, empty_peek_proof.span(),
    );
}

#[test]
#[should_panic(expected: ('GAME_WAGER_CAP',))]
fn test_blackjack_rejects_hand_above_absolute_game_cap_even_if_table_limit_is_higher() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(10_u8, 9_u8, 7_u8, 8_u8);
    let hand_id = stack.blackjack.peek_next_hand_id();
    let empty_peek_proof: Array<felt252> = array![];

    start_cheat_caller_address(stack.table_registry_address, owner());
    stack.table_registry.set_table_limits(TABLE_ID, TABLE_MIN_WAGER, 250_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.table_registry_address);

    precommit_hand(stack, hand_id, fixture.root, 9_u8, false);
    open_hand_verified_raw_with_wager(
        stack, fixture, 101_u128 * ONE_STRK, 9_u8, 10_u8, 7_u8, empty_peek_proof.span(),
    );
}

#[test]
#[should_panic(expected: ('GAME_WAGER_CAP',))]
fn test_blackjack_owner_can_tighten_absolute_wager_cap() {
    let stack = deploy_stack();
    let fixture = verified_deck_fixture(10_u8, 9_u8, 7_u8, 8_u8);
    let hand_id = stack.blackjack.peek_next_hand_id();
    let empty_peek_proof: Array<felt252> = array![];

    start_cheat_caller_address(stack.blackjack_address, owner());
    stack.blackjack.set_wager_cap(40_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.blackjack_address);

    assert(stack.blackjack.get_wager_cap() == 40_u128 * ONE_STRK, 'CAP_UPDATED');

    precommit_hand(stack, hand_id, fixture.root, 9_u8, false);
    open_hand_verified_raw_with_wager(
        stack, fixture, 41_u128 * ONE_STRK, 9_u8, 10_u8, 7_u8, empty_peek_proof.span(),
    );
}

#[test]
fn test_taking_insurance_pays_two_to_one_when_dealer_has_blackjack() {
    let stack = deploy_stack();
    let (hand_id, fixture) = open_hand_with_dealer_blackjack(stack, 1_u8, 9_u8, 7_u8);

    start_cheat_caller_address(stack.blackjack_address, session_key());
    stack.blackjack.submit_take_insurance(player(), hand_id, false);
    stop_cheat_caller_address(stack.blackjack_address);

    start_cheat_caller_address(stack.blackjack_address, session_key());
    stack.blackjack.reveal_dealer_card_verified(hand_id, 10_u8, fixture.dealer_hole_proof);
    stack.blackjack.finalize_hand(hand_id);
    stop_cheat_caller_address(stack.blackjack_address);

    let settled = stack.blackjack.get_seat(hand_id, 0_u8);
    assert(settled.outcome == HandOutcome::Loss, 'MAIN_HAND_LOSS');
    assert(settled.payout == 0_u128, 'NO_MAIN_PAYOUT');
    assert(stack.vault.balance_of(player()) == STARTING_BANKROLL, 'INSURANCE_OFFSETS_LOSS');
}

#[test]
fn test_natural_blackjack_pays_three_to_two_with_verified_dealer_reveal() {
    let stack = deploy_stack();
    let (hand_id, fixture) = open_hand_with_fixture(stack, 9_u8, 1_u8, 13_u8, 8_u8);

    let hand = stack.blackjack.get_hand(hand_id);
    assert(hand.status == HandStatus::AwaitingDealer, 'BLACKJACK_WAITS');

    start_cheat_caller_address(stack.blackjack_address, session_key());
    stack.blackjack.reveal_dealer_card_verified(hand_id, 8_u8, fixture.dealer_hole_proof);
    stack.blackjack.finalize_hand(hand_id);
    stop_cheat_caller_address(stack.blackjack_address);

    let settled_seat = stack.blackjack.get_seat(hand_id, 0_u8);
    assert(settled_seat.outcome == HandOutcome::Blackjack, 'BLACKJACK_OUTCOME');
    assert(
        settled_seat.payout == DEFAULT_WAGER + DEFAULT_WAGER + (DEFAULT_WAGER / 2_u128),
        'BLACKJACK_PAYOUT',
    );
    assert(
        stack.vault.balance_of(player()) == STARTING_BANKROLL
            + (DEFAULT_WAGER + (DEFAULT_WAGER / 2_u128)),
        'BLACKJACK_BALANCE',
    );
}

#[test]
fn test_void_expired_blackjack_hand_refunds_reserved_balance() {
    let stack = deploy_stack();
    let hand_id = open_hand(stack, 9_u8, 10_u8, 6_u8);

    start_cheat_caller_address(stack.blackjack_address, player());
    stack.blackjack.submit_stand(player(), hand_id, 0_u8);
    stop_cheat_caller_address(stack.blackjack_address);

    start_cheat_block_number(stack.blackjack_address, 1_000_000_u64);
    start_cheat_caller_address(stack.blackjack_address, player());
    stack.blackjack.void_expired_hand(hand_id);
    stop_cheat_caller_address(stack.blackjack_address);
    stop_cheat_block_number(stack.blackjack_address);

    let hand = stack.blackjack.get_hand(hand_id);
    assert(hand.status == HandStatus::Voided, 'HAND_VOIDED');
    assert(stack.vault.balance_of(player()) == STARTING_BANKROLL, 'PLAYER_REFUNDED');
    assert(stack.vault.reserved_of(player(), hand_id) == 0_u128, 'RESERVE_CLEARED');
    assert(stack.vault.hand_exposure_of(hand_id) == 0_u128, 'EXPOSURE_CLEARED');
}

#[test]
#[should_panic(expected: 'CARD_PROOF_REQUIRED')]
fn test_legacy_card_entrypoints_are_disabled() {
    let stack = deploy_stack();
    let hand_id = open_hand(stack, 9_u8, 10_u8, 6_u8);

    start_cheat_caller_address(stack.blackjack_address, session_key());
    stack.blackjack.reveal_dealer_card(hand_id, 8_u8);
    stop_cheat_caller_address(stack.blackjack_address);
}

#[test]
#[fork(
    url: "https://starknet-sepolia.g.alchemy.com/starknet/version/rpc/v0_10/demo",
    block_tag: latest,
)]
#[ignore]
fn test_real_groth16_verifier_accepts_fixture_proof() {
    let verifier = IGroth16VerifierBN254LibraryDispatcher { class_hash: declare_real_verifier() };
    let proof = load_real_peek_proof();
    let result = verifier.verify_groth16_proof_bn254(proof.span());
    let public_inputs = match result {
        Result::Ok(values) => values,
        Result::Err(_) => panic_with_felt252('VERIFIER_REJECTED'),
    };

    assert(public_inputs.len() == 11, 'PUBLIC_INPUT_LEN');
    assert(
        *public_inputs
            .at(
                0,
            ) == u256_words(0x68a8a7a0c0dc2da720044f67e4723e3d, 0x2b977049c4c6197e4f21bf81de1a3dc7),
        'HAND_HASH_PI',
    );
    assert(
        *public_inputs
            .at(
                1,
            ) == u256_words(0x3ff3c820a102097d8bf3ff29a3ebfb10, 0x287dcca72d0d67c4b482a1f6fb2d6ecf),
        'ROOT_PI',
    );
    assert(*public_inputs.at(2) == u256_from_u64(3_u64), 'INDEX_PI');
    assert(*public_inputs.at(3) == u256_from_u8(1_u8), 'UPCARD_CLASS_PI');
    assert(*public_inputs.at(4) == u256_zero(), 'PEEK_RESULT_PI');
    assert(*public_inputs.at(5) == u256_from_u64(1_u64), 'HAND_ID_PI');
    assert(*public_inputs.at(6) == u256_from_u64(TABLE_ID), 'TABLE_ID_PI');
    assert(*public_inputs.at(7) == u256_from_u128(DEFAULT_WAGER), 'WAGER_PI');
    assert(*public_inputs.at(8) == u256_from_u8(1_u8), 'UPCARD_PI');
    assert(*public_inputs.at(9) == u256_from_u8(10_u8), 'PLAYER_FIRST_PI');
    assert(*public_inputs.at(10) == u256_from_u8(9_u8), 'PLAYER_SECOND_PI');
}

#[test]
#[fork(
    url: "https://starknet-sepolia.g.alchemy.com/starknet/version/rpc/v0_10/demo",
    block_tag: latest,
)]
#[ignore]
#[should_panic]
fn test_real_groth16_verifier_rejects_corrupted_proof() {
    let verifier = IGroth16VerifierBN254LibraryDispatcher { class_hash: declare_real_verifier() };
    let proof = load_real_peek_proof();
    let mut corrupted: Array<felt252> = array![];
    let len = proof.len();
    let mut index = 0;
    loop {
        if index >= len {
            break;
        }
        if index == 33 {
            corrupted.append(1);
        } else {
            corrupted.append(*proof.at(index));
        }
        index += 1;
    }
    let _ = verifier.verify_groth16_proof_bn254(corrupted.span());
}
