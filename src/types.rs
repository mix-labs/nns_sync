use candid::Principal;
use dfn_protobuf::{ProtoBuf, ToProto};
use ic_agent::Agent;
use ic_types::CanisterId;
use on_wire::{FromWire, IntoWire};
use rbatis::{crud::CRUD, crud_table};
use serde::{Deserialize, Serialize};

pub const LOGFIX: &str = "================================================================================================";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Error {
    MalformedMessage(String),
    KeyNotPresent(Vec<u8>),
    KeyAlreadyPresent(Vec<u8>),
    VersionNotLatest(Vec<u8>),
    VersionBeyondLatest(Vec<u8>),
    RegistryUnreachable(String),
    UnknownError(String),
}

pub async fn query<Payload: ToProto, Res: ToProto>(
    agent: &Agent,
    canister_id: Principal,
    method: &str,
    payload: Payload,
) -> Result<Res, String> {
    let arg = ProtoBuf(payload).into_bytes()?;
    let bytes = agent
        .query(&canister_id, method)
        .with_arg(arg)
        .call()
        .await
        .expect("query pb interface error.");
    ProtoBuf::from_bytes(bytes).map(|c| c.0)
}

#[crud_table(table_name:"transactions" | table_columns:"id,tx_hash,block_hash,tx_type,created_time,tx_from,tx_to,amount,fee,memo")]
#[derive(Clone, Debug)]
pub struct Transaction {
    pub id: u64,
    pub tx_hash: String,
    pub block_hash: String,
    pub tx_type: String,
    pub created_time: u64,
    pub tx_from: String,
    pub tx_to: String,
    pub amount: u64,
    pub fee: u64,
    pub memo: String,
}
