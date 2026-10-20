#![cfg(test)]

use crate::{BountyEscrowContract, BountyEscrowContractClient, Error, EscrowStatus, RefundMode};
use soroban_sdk::{testutils::{Address as _, Ledger}, token, Address, Env};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract = e.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    (
        token::Client::new(e, &contract_address),
        token::StellarAssetClient::new(e, &contract_address),
    )
}

struct S<'a> {
    env: Env,
    depositor: Address,
    contributor: Address,
    escrow: BountyEscrowContractClient<'a>,
}

impl<'a> S<'a> {
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
        S { env, depositor, contributor, escrow }
    }
}

#[test]
fn test_refund_driving_draft_state_typed_error_not_mutating() {
    let s = S::new();
    let bid = 201;
    let amt = 500;
    let dl = s.env.ledger().timestamp() + 10;
    s.escrow.lock_funds(&s.depositor, &bid, &amt, &dl);
    assert_eq!(s.escrow.get_escrow_info(&bid).status, EscrowStatus::Draft);
    s.env.ledger().set_timestamp(dl + 1);
    let res = s.escrow.try_refund(&bid);
    assert_eq!(res.unwrap_err().unwrap(), Error::FundsNotLocked);
    let info = s.escrow.get_escrow_info(&bid);
    assert_eq!(info.status, EscrowStatus::Draft);
    assert_eq!(info.remaining_amount, amt);
}

#[test]
fn test_refund_driving_locked_state_typed_error_not_mutating() {
    let s = S::new();
    let bid = 202;
    let amt = 500;
    let dl = s.env.ledger().timestamp() + 1000;
    s.escrow.lock_funds(&s.depositor, &bid, &amt, &dl);
    s.escrow.publish(&bid);
    assert_eq!(s.escrow.get_escrow_info(&bid).status, EscrowStatus::Locked);
    let res = s.escrow.try_refund(&bid);
    assert_eq!(res.unwrap_err().unwrap(), Error::DeadlineNotPassed);
    let info = s.escrow.get_escrow_info(&bid);
    assert_eq!(info.status, EscrowStatus::Locked);
    assert_eq!(info.remaining_amount, amt);
}

#[test]
fn test_refund_driving_released_state_typed_error_not_mutating() {
    let s = S::new();
    let bid = 203;
    let amt = 500;
    let dl = s.env.ledger().timestamp() + 10;
    s.escrow.lock_funds(&s.depositor, &bid, &amt, &dl);
    s.escrow.publish(&bid);
    s.escrow.release_funds(&bid, &s.contributor);
    assert_eq!(s.escrow.get_escrow_info(&bid).status, EscrowStatus::Released);
    let res = s.escrow.try_refund(&bid);
    assert_eq!(res.unwrap_err().unwrap(), Error::FundsNotLocked);
    let info = s.escrow.get_escrow_info(&bid);
    assert_eq!(info.status, EscrowStatus::Released);
    assert_eq!(info.remaining_amount, 0);
}

#[test]
fn test_refund_driving_refunded_state_typed_error_not_mutating() {
    let s = S::new();
    let bid = 204;
    let amt = 500;
    let dl = s.env.ledger().timestamp() + 10;
    s.escrow.lock_funds(&s.depositor, &bid, &amt, &dl);
    s.escrow.publish(&bid);
    s.env.ledger().set_timestamp(dl + 1);
    s.escrow.refund(&bid);
    assert_eq!(s.escrow.get_escrow_info(&bid).status, EscrowStatus::Refunded);
    let rem = s.escrow.get_escrow_info(&bid).remaining_amount;
    let res = s.escrow.try_refund(&bid);
    assert_eq!(res.unwrap_err().unwrap(), Error::FundsNotLocked);
    let info = s.escrow.get_escrow_info(&bid);
    assert_eq!(info.status, EscrowStatus::Refunded);
    assert_eq!(info.remaining_amount, rem);
}

#[test]
fn test_refund_driving_partially_refunded_state_typed_error_not_mutating() {
    let s = S::new();
    let bid = 205;
    let amt = 1000;
    let dl = s.env.ledger().timestamp() + 10000;
    s.escrow.lock_funds(&s.depositor, &bid, &amt, &dl);
    s.escrow.publish(&bid);
    s.escrow.approve_refund(&bid, &250, &s.depositor, &RefundMode::Partial);
    s.escrow.refund(&bid);
    assert_eq!(s.escrow.get_escrow_info(&bid).status, EscrowStatus::PartiallyRefunded);
    let rem = s.escrow.get_escrow_info(&bid).remaining_amount;
    let res = s.escrow.try_refund(&bid);
    assert_eq!(res.unwrap_err().unwrap(), Error::DeadlineNotPassed);
    let info = s.escrow.get_escrow_info(&bid);
    assert_eq!(info.status, EscrowStatus::PartiallyRefunded);
    assert_eq!(info.remaining_amount, rem);
}
