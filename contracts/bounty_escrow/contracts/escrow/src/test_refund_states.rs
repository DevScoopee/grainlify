
#![cfg(test)]

use crate::{BountyEscrowContract, BountyEscrowContractClient, Error, EscrowStatus, RefundMode};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract = e.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    (
        token::Client::new(e, &contract_address),
        token::StellarAssetClient::new(e, &contract_address),
    )
}

struct TestSetup<'a> {
    env: Env,
    admin: Address,
    depositor: Address,
    contributor: Address,
    token: token::Client<'a>,
    token_admin: token::StellarAssetClient<'a>,
    escrow: BountyEscrowContractClient<'a>,
}

impl<'a> TestSetup<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let depositor = Address::generate(&env);
        let contributor = Address::generate(&env);
        let (token, token_admin) = create_token_contract(&env, &admin);
        let contract_id = env.register_contract(None, BountyEscrowContract);
        let escrow = BountyEscrowContractClient::new(&env, &contract_id);
        escrow.init(&admin, &token.address);
        token_admin.mint(&depositor, &1_000_000);
        TestSetup {
            env,
            admin,
            depositor,
            contributor,
            token,
            token_admin,
            escrow,
        }
    }
}

#[test]
fn test_refund_from_draft_state_returns_typed_error() {
    let setup = TestSetup::new();
    let bounty_id = 1;
    let amount = 1000;
    let deadline = setup.env.ledger().timestamp() + 1000;
    
    setup.escrow.lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);
    let info_before = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_before.status, EscrowStatus::Draft);
    
    setup.env.ledger().set_timestamp(deadline + 1);
    let res = setup.escrow.try_refund(&bounty_id);
    assert_eq!(res.unwrap_err().unwrap(), Error::FundsNotLocked);
    
    let info_after = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_after.status, EscrowStatus::Draft);
}

#[test]
fn test_refund_from_locked_state_too_early_returns_typed_error() {
    let setup = TestSetup::new();
    let bounty_id = 2;
    let amount = 1000;
    let deadline = setup.env.ledger().timestamp() + 1000;
    
    setup.escrow.lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);
    setup.escrow.publish(&bounty_id);
    let info_before = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_before.status, EscrowStatus::Locked);
    
    let res = setup.escrow.try_refund(&bounty_id);
    assert_eq!(res.unwrap_err().unwrap(), Error::DeadlineNotPassed);
    
    let info_after = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_after.status, EscrowStatus::Locked);
    assert_eq!(info_after.remaining_amount, amount);
}

#[test]
fn test_refund_from_locked_state_after_deadline_succeeds() {
    let setup = TestSetup::new();
    let bounty_id = 3;
    let amount = 1000;
    let deadline = setup.env.ledger().timestamp() + 1000;
    
    setup.escrow.lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);
    setup.escrow.publish(&bounty_id);
    
    setup.env.ledger().set_timestamp(deadline + 1);
    setup.escrow.refund(&bounty_id);
    
    let info = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info.status, EscrowStatus::Refunded);
}

#[test]
fn test_refund_from_released_state_returns_typed_error_and_no_mutation() {
    let setup = TestSetup::new();
    let bounty_id = 4;
    let amount = 1000;
    let deadline = setup.env.ledger().timestamp() + 1000;
    
    setup.escrow.lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);
    setup.escrow.publish(&bounty_id);
    setup.escrow.release_funds(&bounty_id, &setup.contributor);
    
    let info_before = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_before.status, EscrowStatus::Released);
    
    let res = setup.escrow.try_refund(&bounty_id);
    assert_eq!(res.unwrap_err().unwrap(), Error::FundsNotLocked);
    
    let info_after = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_after.status, EscrowStatus::Released);
    assert_eq!(info_after.remaining_amount, 0);
}

#[test]
fn test_refund_from_refunded_state_returns_typed_error_and_no_mutation() {
    let setup = TestSetup::new();
    let bounty_id = 5;
    let amount = 1000;
    let deadline = setup.env.ledger().timestamp() + 1000;
    
    setup.escrow.lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);
    setup.escrow.publish(&bounty_id);
    setup.env.ledger().set_timestamp(deadline + 1);
    setup.escrow.refund(&bounty_id);
    
    let info_before = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_before.status, EscrowStatus::Refunded);
    let remaining_before = info_before.remaining_amount;
    
    let res = setup.escrow.try_refund(&bounty_id);
    assert_eq!(res.unwrap_err().unwrap(), Error::FundsNotLocked);
    
    let info_after = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_after.status, EscrowStatus::Refunded);
    assert_eq!(info_after.remaining_amount, remaining_before);
}

#[test]
fn test_refund_from_partially_refunded_state_returns_typed_error_without_mutation() {
    let setup = TestSetup::new();
    let bounty_id = 6;
    let amount = 1000;
    let deadline = setup.env.ledger().timestamp() + 10000;
    
    setup.escrow.lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);
    setup.escrow.publish(&bounty_id);
    setup.escrow.approve_refund(&bounty_id, &400, &setup.depositor, &RefundMode::Partial);
    setup.escrow.refund(&bounty_id);
    
    let info_before = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_before.status, EscrowStatus::PartiallyRefunded);
    assert_eq!(info_before.remaining_amount, 600);
    
    // Try to refund again without proper approval/new conditions - should fail and not mutate
    let res = setup.escrow.try_refund(&bounty_id);
    assert_eq!(res.unwrap_err().unwrap(), Error::DeadlineNotPassed);
    
    let info_after = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(info_after.status, EscrowStatus::PartiallyRefunded);
    assert_eq!(info_after.remaining_amount, 600);
}
