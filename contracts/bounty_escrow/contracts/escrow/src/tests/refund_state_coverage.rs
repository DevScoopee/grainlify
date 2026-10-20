use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// ============================================================================
// Refund coverage from every EscrowStatus state (Draft, Locked, Released,
// Refunded, PartiallyRefunded). Every outcome is asserted as a value or a
// typed error via try_refund — never via panic. Terminal states (Released,
// Refunded) are asserted byte-for-byte unchanged after a refused call.
// ============================================================================

const AMOUNT: i128 = 1_000;

struct Setup<'a> {
    env: Env,
    contract_id: Address,
    client: BountyEscrowContractClient<'a>,
    #[allow(dead_code)]
    token: token::Client<'a>,
    token_admin: token::StellarAssetClient<'a>,
    admin: Address,
    depositor: Address,
    contributor: Address,
}

impl<'a> Setup<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let depositor = Address::generate(&env);
        let contributor = Address::generate(&env);

        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token::Client::new(&env, &sac.address());
        let token_admin = token::StellarAssetClient::new(&env, &sac.address());

        let contract_id = env.register_contract(None, BountyEscrowContract);
        let client = BountyEscrowContractClient::new(&env, &contract_id);
        client.init(&admin, &sac.address());

        Self {
            env,
            contract_id,
            client,
            token,
            token_admin,
            admin,
            depositor,
            contributor,
        }
    }

    /// Inject an escrow record directly into storage in the requested status,
    /// funding the contract so any permitted transfer would succeed.
    fn escrow_in_state(&self, status: EscrowStatus, bounty_id: u64) {
        let deadline = self.env.ledger().timestamp() + 1_000;
        let escrow = Escrow {
            depositor: self.depositor.clone(),
            amount: AMOUNT,
            remaining_amount: AMOUNT,
            status,
            deadline,
            refund_history: soroban_sdk::vec![&self.env],
            archived: false,
            archived_at: None,
        };
        self.token_admin.mint(&self.contract_id, &AMOUNT);
        self.env.as_contract(&self.contract_id, || {
            self.env
                .storage()
                .persistent()
                .set(&DataKey::Escrow(bounty_id), &escrow);
        });
    }

    fn advance_past_deadline(&self, bounty_id: u64) {
        let info = self.client.get_escrow_info(&bounty_id);
        self.env.ledger().set_timestamp(info.deadline + 1);
    }
}

// --- Draft (initial, non-refundable) ---

#[test]
fn test_refund_from_draft_state_refused() {
    let setup = Setup::new();
    let bounty_id = 1;
    setup.escrow_in_state(EscrowStatus::Draft, bounty_id);
    setup.advance_past_deadline(bounty_id);

    let res = setup.client.try_refund(&bounty_id);
    assert!(
        matches!(res, Err(Ok(Error::FundsNotLocked))),
        "refund from Draft must fail with FundsNotLocked, got {:?}",
        res
    );

    let stored = setup.client.get_escrow_info(&bounty_id);
    assert_eq!(stored.status, EscrowStatus::Draft);
    assert_eq!(stored.remaining_amount, AMOUNT);
    assert_eq!(stored.refund_history.len(), 0);
}

// --- Locked (refundable) ---

#[test]
fn test_refund_from_locked_state_succeeds() {
    let setup = Setup::new();
    let bounty_id = 2;
    setup.escrow_in_state(EscrowStatus::Locked, bounty_id);
    setup.advance_past_deadline(bounty_id);

    let res = setup.client.try_refund(&bounty_id);
    assert!(
        matches!(res, Ok(Ok(()))),
        "refund from Locked must succeed, got {:?}",
        res
    );

    let stored = setup.client.get_escrow_info(&bounty_id);
    assert_eq!(stored.status, EscrowStatus::Refunded);
    assert_eq!(stored.remaining_amount, 0);
    assert_eq!(stored.refund_history.len(), 1);
}

#[test]
fn test_refund_from_locked_state_before_deadline_refused() {
    let setup = Setup::new();
    let bounty_id = 3;
    setup.escrow_in_state(EscrowStatus::Locked, bounty_id);

    let res = setup.client.try_refund(&bounty_id);
    assert!(
        matches!(res, Err(Ok(Error::DeadlineNotPassed))),
        "refund from Locked before deadline must fail with DeadlineNotPassed, got {:?}",
        res
    );

    let stored = setup.client.get_escrow_info(&bounty_id);
    assert_eq!(stored.status, EscrowStatus::Locked);
    assert_eq!(stored.remaining_amount, AMOUNT);
    assert_eq!(stored.refund_history.len(), 0);
}

// --- Released (terminal) ---

#[test]
fn test_refund_from_released_state_refused_and_not_mutated() {
    let setup = Setup::new();
    let bounty_id = 4;
    setup.escrow_in_state(EscrowStatus::Released, bounty_id);
    setup.advance_past_deadline(bounty_id);

    let before = setup.client.get_escrow_info(&bounty_id);

    let res = setup.client.try_refund(&bounty_id);
    assert!(
        matches!(res, Err(Ok(Error::FundsNotLocked))),
        "refund from Released must fail with FundsNotLocked, got {:?}",
        res
    );

    let after = setup.client.get_escrow_info(&bounty_id);
    assert_eq!(after, before, "terminal Released record mutated by refused refund");
    assert_eq!(after.status, EscrowStatus::Released);
    assert_eq!(after.remaining_amount, AMOUNT);
    assert_eq!(after.refund_history.len(), 0);
}

// --- Refunded (terminal) ---

#[test]
fn test_refund_from_refunded_state_refused_and_not_mutated() {
    let setup = Setup::new();
    let bounty_id = 5;
    setup.escrow_in_state(EscrowStatus::Refunded, bounty_id);
    setup.advance_past_deadline(bounty_id);

    let before = setup.client.get_escrow_info(&bounty_id);

    let res = setup.client.try_refund(&bounty_id);
    assert!(
        matches!(res, Err(Ok(Error::FundsNotLocked))),
        "refund from Refunded must fail with FundsNotLocked, got {:?}",
        res
    );

    let after = setup.client.get_escrow_info(&bounty_id);
    assert_eq!(after, before, "terminal Refunded record mutated by refused refund");
    assert_eq!(after.status, EscrowStatus::Refunded);
    assert_eq!(after.remaining_amount, AMOUNT);
    assert_eq!(after.refund_history.len(), 0);
}

// --- PartiallyRefunded (refundable remainder) ---

#[test]
fn test_refund_from_partially_refunded_state_completes() {
    let setup = Setup::new();
    let bounty_id = 6;
    setup.escrow_in_state(EscrowStatus::PartiallyRefunded, bounty_id);
    setup.advance_past_deadline(bounty_id);

    let res = setup.client.try_refund(&bounty_id);
    assert!(
        matches!(res, Ok(Ok(()))),
        "refund from PartiallyRefunded must succeed, got {:?}",
        res
    );

    let stored = setup.client.get_escrow_info(&bounty_id);
    assert_eq!(stored.status, EscrowStatus::Refunded);
    assert_eq!(stored.remaining_amount, 0);
    assert_eq!(stored.refund_history.len(), 1);
}

#[test]
fn test_refund_from_partially_refunded_state_before_deadline_refused() {
    let setup = Setup::new();
    let bounty_id = 7;
    setup.escrow_in_state(EscrowStatus::PartiallyRefunded, bounty_id);

    let res = setup.client.try_refund(&bounty_id);
    assert!(
        matches!(res, Err(Ok(Error::DeadlineNotPassed))),
        "refund from PartiallyRefunded before deadline must fail with DeadlineNotPassed, got {:?}",
        res
    );

    let stored = setup.client.get_escrow_info(&bounty_id);
    assert_eq!(stored.status, EscrowStatus::PartiallyRefunded);
    assert_eq!(stored.remaining_amount, AMOUNT);
    assert_eq!(stored.refund_history.len(), 0);
}
