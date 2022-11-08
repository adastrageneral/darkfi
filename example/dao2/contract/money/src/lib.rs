use darkfi_sdk::{
    crypto::{ContractId, PublicKey},
    db::{db_init, db_lookup, db_set},
    define_contract,
    msg,
    error::ContractResult,
    pasta::pallas,
    tx::ContractCall,
    util::set_return_data,
};
use darkfi_serial::{serialize, Encodable, SerialDecodable, SerialEncodable, WriteExt, deserialize};

#[repr(u8)]
pub enum MoneyFunction {
    Transfer = 0x00,
}

impl From<u8> for MoneyFunction {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Self::Transfer,
            _ => panic!("Invalid function ID: {:#04x?}", b),
        }
    }
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct MoneyTransferParams {
    /// Clear inputs
    pub clear_inputs: Vec<ClearInput>,
    /// Anonymous inputs
    pub inputs: Vec<Input>,
    /// Anonymous outputs
    pub outputs: Vec<Output>,
}
#[derive(SerialEncodable, SerialDecodable)]
pub struct MoneyTransferUpdate {
    // nullifiers
    // coins
}

/// A transaction's clear input
#[derive(SerialEncodable, SerialDecodable)]
pub struct ClearInput {
    /// Input's value (amount)
    pub value: u64,
    /// Input's token ID
    pub token_id: pallas::Base,
    /// Blinding factor for `value`
    pub value_blind: pallas::Scalar,
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Scalar,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// A transaction's anonymous input
#[derive(SerialEncodable, SerialDecodable)]
pub struct Input {
    // Public inputs for the zero-knowledge proof
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
    pub nullifier: pallas::Base,
    pub merkle_root: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

/// A transaction's anonymous output
#[derive(SerialEncodable, SerialDecodable)]
pub struct Output {
    // Public inputs for the zero-knowledge proof
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
    pub coin: pallas::Base,
    /// The encrypted note ciphertext
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
}

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    let db_handle = db_init(cid, "wagies")?;

    Ok(())
}
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let zk_public_values: Vec<(String, Vec<pallas::Base>)> = Vec::new();
    let signature_public_keys: Vec<pallas::Point> = Vec::new();

    let mut metadata = Vec::new();
    zk_public_values.encode(&mut metadata)?;
    signature_public_keys.encode(&mut metadata)?;
    set_return_data(&metadata)?;

    Ok(())
}
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;

    assert!(call_idx < call.len() as u32);
    let self_ = &call[call_idx as usize];

    match MoneyFunction::from(self_.data[0]) {
        MoneyFunction::Transfer => {
            let update = MoneyTransferUpdate {};

            let mut update_data = Vec::new();
            update_data.write_u8(MoneyFunction::Transfer as u8)?;
            update.encode(&mut update_data)?;
            set_return_data(&update_data)?;
            msg!("update is set!");
        }
    }
    Ok(())
}
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MoneyFunction::from(update_data[0]) {
        MoneyFunction::Transfer => {
            let db_handle = db_lookup(cid, "wagies")?;
            db_set(db_handle, &serialize(&"jason_gulag".to_string()), &serialize(&110))?;
        }
    }

    Ok(())
}