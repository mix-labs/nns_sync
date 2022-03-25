use ic_agent::{
    agent::http_transport::ReqwestHttpReplicaV2Transport, ic_types::Principal,
    identity::AnonymousIdentity, Agent,
};
use ic_protobuf::registry::routing_table::v1::RoutingTable;
use ic_registry_transport::pb::v1::{RegistryGetValueRequest, RegistryGetValueResponse};
use ledger_canister::EncodedBlock;
use ledger_canister::{Block, Operation};
use mysql::{Opts, Pool};
use on_wire::NewType;
use prost::Message;
use rbatis::crud::CRUD;
use rbatis::rbatis::Rbatis;
use std::cmp::min;
use std::env;
use types::Transaction;
mod ledger;
mod registry;
mod types;
use crate::ledger::{block_pb, tip_of_chain_pb};

use mysql::prelude::*;
use mysql::*;
use std::sync::Arc;
use std::sync::RwLock;
use std::{thread, time};
use tokio::task::JoinHandle;
const DEFAULT_IC_GATEWAY: &str = "https://ic0.app";

pub async fn insert_to_mysql(data: Vec<Transaction>, conn: &Rbatis) -> u8 {
    //     let stmt = conn.prep("INSERT INTO transactions_test (id, hash, blockhash, type_, createdtime, from_, to_, amount, fee, memo) VALUES (:id, :hash, :blockhash, :type_, :createdtime, :from_, :to_, :amount, :fee, :memo)")
    //  .unwrap();
    //     for i in 0..data.len() {
    //         conn.exec_drop(
    //             &stmt,
    //             params! {
    //                 "id" => data[i].id,
    //                 "hash" => data[i].hash.clone(),
    //                 "blockhash" => data[i].blockhash.clone(),
    //                 "type_" => data[i].type_.clone(),
    //                 "createdtime" => data[i].createdtime,
    //                 "from_" => data[i].from.clone(),
    //                 "to_" => data[i].to.clone(),
    //                 "amount" => data[i].amount,
    //                 "fee" => data[i].fee,
    //                 "memo" => data[i].memo.clone(),
    //             },
    //         )
    //         .unwrap()
    //     }
    let result = conn.save_batch(&data, &[]).await;
    if let Err(err) = result {
        println!("{:#?}", err);
        0
    } else {
        1
    }
    //println!("last generated key: {}")
}
pub fn get_block_height(conn: &mut PooledConn) -> u64 {
    let res: Result<Option<u64>> = conn.query_first("select count(1) from transactions");
    return res.unwrap().unwrap();
}
pub fn convert_to_mysqldata(block: Block, id: u64) -> Transaction {
    let mut transaction = Transaction {
        id: id,
        tx_hash: hex::encode(block.transaction.hash().into_bytes()),
        block_hash: String::from(""),
        tx_type: String::from(""),
        created_time: block.transaction.created_at_time.timestamp_nanos,
        tx_from: String::from(""),
        tx_to: String::from(""),
        amount: 0,
        fee: 0,
        memo: block.transaction.memo.0.to_string(),
    };
    match block.transaction.operation {
        Operation::Mint {
            to: to_,
            amount: amount_,
        } => {
            transaction.amount = amount_.get_e8s();
            transaction.tx_to = to_.to_hex();
            transaction.tx_type = String::from("Mint");
        }
        Operation::Burn {
            from: from_,
            amount: amount_,
        } => {
            transaction.amount = amount_.get_e8s();
            transaction.tx_from = from_.to_hex();
            transaction.tx_type = String::from("Burn");
        }
        Operation::Transfer {
            from: from_,
            to: to_,
            fee: fee_,
            amount: amount_,
        } => {
            transaction.amount = amount_.get_e8s();
            transaction.tx_from = from_.to_hex();
            transaction.tx_to = to_.to_hex();
            transaction.fee = fee_.get_e8s();
            transaction.tx_type = String::from("Transfer");
        }
    }
    return transaction;
}

pub async fn get_new_transaction(id: u64, agent: Agent, set: Arc<RwLock<Vec<Transaction>>>) {
    let data = block_pb(&agent, id).await;
    if let Ok(block) = data {
        let transaction = convert_to_mysqldata(block, id + 1);
        let mut s = set.write().unwrap();
        s.push(transaction);
        drop(s);
    }
}

#[tokio::main]
async fn main() {
    let agent = Agent::builder()
        .with_transport(
            ReqwestHttpReplicaV2Transport::create(DEFAULT_IC_GATEWAY)
                .expect("Failed to create Transport for Agent"),
        )
        .with_identity(AnonymousIdentity {})
        .build()
        .expect("Failed to build the Agent");
    let max_thread = 20;
    // let url = "root:xyz12345@(localhost:3306)/xyz";
    let url = "mysql://admin:Gbs1767359487@database-mysql-instance-1.ccggmi9astti.us-east-1.rds.amazonaws.com:3306/db1";
    let rb = Rbatis::new();
    rb.link(url).await.unwrap();
    let opts = Opts::from_url(url).unwrap();
    let pool = Pool::new(opts).unwrap();
    let mut height = get_block_height(&mut pool.get_conn().unwrap());
    println!("present sync blocks in database {:?}", height);
    while true {
        let b = ledger::get_blocks_pb(&agent, height, 1000).await;
        if let Some(Blocks) = b {
            let mut new_transactions: Vec<Transaction> = Vec::new();
            let mut add_height = 0;
            for block in Blocks {
                new_transactions.push(convert_to_mysqldata(block, height + add_height + 1));
                add_height += 1;
            }
            let result = insert_to_mysql(new_transactions, &rb).await;
            if result == 1 {
                height += add_height;
            }
        } else {
            println!("batch sync finish....convert to multi-thread sync");
            break;
            // let ten_seconds = time::Duration::from_secs(10);
            // thread::sleep(ten_seconds);
        }
        println!("==>>sync batch transactions to {:?}", height);
    }

    while true {
        let current_height_result = tip_of_chain_pb(&agent).await;
        let mut current_height = 0;
        if let Err(s) = current_height_result {
            println!("{:#?}", s);
            continue;
        } else {
            current_height = current_height_result.unwrap().tip_index + 1;
        }
        // let current_height = tip_of_chain_pb(&agent).await.tip_index + 1;
        println!("current blocks on IC {:?}", current_height);
        while (height < current_height) {
            let set: Vec<Transaction> = Vec::new();
            let set_arc = Arc::new(RwLock::new(set));
            let mut thread_vec: Vec<JoinHandle<()>> = Vec::new();
            let num_thread = min(max_thread, current_height - height);
            for i in 0..num_thread {
                let agent_clone = agent.clone();
                let set_ = set_arc.clone();
                let handle = tokio::spawn(async move {
                    get_new_transaction(height + i, agent_clone, set_).await
                });
                thread_vec.push(handle);
            }
            for handle in thread_vec {
                handle.await;
            }
            let mut data = (*(set_arc.read().unwrap())).clone();
            let l = data.len();
            if l as u64 != num_thread {
                println!("thread error...num not match, pre heigh {:?}, skip", height);
                continue;
            }
            let mut m: [i32; 20] = [-1; 20];
            let mut flag: bool = true;
            for transaction in &data {
                if (transaction.id <= height || transaction.id > height + l as u64) {
                    flag = false;
                    continue;
                }
                m[(transaction.id - height - 1) as usize] = 1;
            }
            if (!flag) {
                println!("idx not match...retry......, pre height {:?}", height);
                continue;
            }
            for i in 0..l {
                if (m[i] == -1) {
                    flag = false;
                    continue;
                }
            }
            if (!flag) {
                println!("idx miss...retry......, pre height {:?}", height);
                continue;
            }

            let result = insert_to_mysql(data, &rb).await;

            if result == 1 {
                height = height + l as u64;
            }
            println!("sync to {:?} blocks...", height);
        }
        let two_seconds = time::Duration::from_secs(2);
        thread::sleep(two_seconds);
    }
    //println!("{:?}", b);
}
