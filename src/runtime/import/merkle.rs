use std::io::Cursor;

use darkfi_sdk::{
    crypto::{constants::MERKLE_DEPTH, MerkleNode},
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
};
use darkfi_serial::{serialize, Decodable, Encodable, WriteExt};
use log::{debug, error};
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::vm_runtime::{ContractSection, Env};

type MerkleTree = BridgeTree<MerkleNode, { MERKLE_DEPTH }>;

pub(crate) fn merkle_add(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Update => {
            let memory_view = env.memory_view(&ctx);

            let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
                error!(target: "wasm_runtime::merkle_add", "Failed to make slice from ptr");
                return -2
            };

            let mut buf = vec![0_u8; len as usize];
            if let Err(e) = mem_slice.read_slice(&mut buf) {
                error!(target: "wasm_runtime:merkle_add", "Failed to read from memory slice: {}", e);
                return -2
            };

            let mut buf_reader = Cursor::new(buf);

            // FIXME: There's a type DbHandle=u32, but this should maybe be renamed
            let db_info: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::merkle_add", "Failed to decode db_info DbHandle: {}", e);
                    return -2
                }
            };
            let db_info = db_info as usize;

            let db_roots: u32 = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::merkle_add", "Failed to decode db_roots DbHandle: {}", e);
                    return -2
                }
            };
            let db_roots = db_roots as usize;

            let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::merkle_add", "Failed to decode key vec: {}", e);
                    return -2
                }
            };

            let coin: MerkleNode = match Decodable::decode(&mut buf_reader) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::merkle_add", "Failed to decode MerkleNode: {}", e);
                    return -2
                }
            };

            // TODO: Ensure we've read the entire buffer above.

            let db_handles = env.db_handles.borrow();
            let mut db_batches = env.db_batches.borrow_mut();

            if db_handles.len() <= db_info || db_batches.len() <= db_info {
                error!(target: "wasm_runtime::merkle_add", "Requested db_info DbHandle that is out of bounds");
                return -2
            }
            if db_handles.len() <= db_roots || db_batches.len() <= db_roots {
                error!(target: "wasm_runtime::merkle_add", "Requested db_roots DbHandle that is out of bounds");
                return -2
            }

            let handle_idx = db_info;
            let db_info = &db_handles[handle_idx];
            let db_info_batch = &mut db_batches[handle_idx];
            let handle_idx = db_roots;
            //let db_roots = &db_handles[handle_idx];

            // Read the current tree

            let ret = match db_info.get(&key) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::merkle_add", "Internal error getting from tree: {}", e);
                    return -2
                }
            };

            let Some(return_data) = ret else {
                error!(target: "wasm_runtime::merkle_add", "Return data is empty");
                return -2
            };

            debug!(
                target: "wasm_runtime::merkle_add",
                "Serialized tree: {} bytes",
                return_data.len()
            );
            debug!(
                target: "wasm_runtime::merkle_add",
                "                 {:02x?}",
                return_data
            );

            let mut decoder = Cursor::new(&return_data);
            let set_size: u32 = match Decodable::decode(&mut decoder) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::merkle_add", "Unable to read set size: {}", e);
                    return -2
                }
            };
            let mut tree: MerkleTree = match Decodable::decode(&mut decoder) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "wasm_runtime::merkle_add", "Unable to deserialize tree: {}", e);
                    return -2
                }
            };

            tree.append(&coin);
            let Some(root) = tree.root(0) else {
                error!(target: "wasm_runtime::merkle_add", "Unable to read the root of tree");
                return -2;
            };

            if db_info.contract_id != env.contract_id {
                error!(target: "wasm_runtime::merkle_add", "Unauthorized to write to DbHandle");
                return -2
            }

            let mut tree_data = Vec::new();
            if tree_data.write_u32(set_size + 1).is_err() || tree.encode(&mut tree_data).is_err() {
                error!(target: "wasm_runtime::merkle_add", "Couldn't reserialize modified tree");
                return -2
            }
            db_info_batch.insert(key, tree_data);

            let db_roots_batch = &mut db_batches[handle_idx];
            let root_index: Vec<u8> = serialize(&(set_size as u32));
            assert_eq!(root_index.len(), 4);
            let root_value: Vec<u8> = serialize(&root);
            assert_eq!(root_value.len(), 32);
            db_roots_batch.insert(root_index, root_value);

            0
        }
        _ => -1,
    }
}