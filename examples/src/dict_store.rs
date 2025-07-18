// Example for dispatch mode.
//
// This is a simple key-value store.
// We divide items into several shards. Each shard runs on a separate
// thread. So the gRPC get/set/delete/list requests need to be
// dispatched to the corresponding thread.

use std::collections::HashMap;
use std::sync::mpsc;

use pajamax::status::{Code, Status};

use dict_store::*;

// import the generated code from .proto
mod dict_store {
    pajamax::include_proto!("dict_store");
}

// Here we have 2 servers: MyDictDispatch and MyDictShard.

// This is the MyDictDispatch.
//
// It dispatches requests to backend shards by channels, so it
// contains the channel-send-end list.
//
// The instance of this struct is global. All connections share the
// same instence. Pajamax will wrap an `Arc` on this. This is the
// same with `tonic`.
struct MyDictDispatch {
    req_txs: Vec<DictStoreRequestTx>,
}

impl DictStoreDispatch for MyDictDispatch {
    // Return the channel send-end where the request will be dispatched to.
    fn dispatch_to(&self, req: &DictStoreRequest) -> &DictStoreRequestTx {
        match req {
            // hashed by req.key
            DictStoreRequest::Get(req) => self.pick_req_tx(&req.key),
            DictStoreRequest::Set(req) => self.pick_req_tx(&req.key),
            DictStoreRequest::Delete(req) => self.pick_req_tx(&req.key),
            // by req.shard
            DictStoreRequest::ListShard(req) => {
                let i = req.shard as usize % self.req_txs.len();
                &self.req_txs[i]
            }
        }
    }
}

// This is the MyDictShard.
//
// Contains the key-value items in one shard. Requests are dispatched
// to some shard by item key or shard number.
//
// Each backend shard thread owns one instance. So it's mutable and no
// locking for handlers.
struct MyDictShard {
    dict: HashMap<String, f64>,
}

// All methods for this gRPC server.
//
// Compared to the local-mode, here the `self` is `&mut` because each
// shard thread owns one server instance (MyDictShard).
impl DictStoreShard for MyDictShard {
    fn set(&mut self, req: Entry) -> Result<SetReply, Status> {
        let old_value = self.dict.insert(req.key, req.value);
        Ok(SetReply { old_value })
    }
    fn get(&mut self, req: Key) -> Result<Value, Status> {
        match self.dict.get(&req.key) {
            Some(&value) => Ok(Value { value }),
            None => Err(Status {
                code: Code::NotFound,
                message: format!("key: {}", req.key),
            }),
        }
    }
    fn delete(&mut self, req: Key) -> Result<Value, Status> {
        match self.dict.remove(&req.key) {
            Some(value) => Ok(Value { value }),
            None => Err(Status {
                code: Code::NotFound,
                message: format!("key: {}", req.key),
            }),
        }
    }

    // list the items in the current shard only
    fn list_shard(&mut self, _req: ListShardRequest) -> Result<ListShardReply, Status> {
        Ok(ListShardReply {
            count: self.dict.len() as u32,
            entries: self
                .dict
                .iter()
                .map(|(key, value)| Entry {
                    key: key.clone(),
                    value: *value,
                })
                .collect(),
        })
    }
}

// some business code

// pick one channel by hashing the key
impl MyDictDispatch {
    fn pick_req_tx(&self, key: &str) -> &DictStoreRequestTx {
        let hash = hash_key(key) as usize;
        let i = hash % self.req_txs.len();
        &self.req_txs[i]
    }
}

// a common hash util helper
fn hash_key<K>(key: &K) -> u64
where
    K: std::hash::Hash + ?Sized,
{
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

// backend shard routine
fn shard_routine(req_rx: DictStoreRequestRx) {
    let shard = MyDictShard {
        dict: HashMap::new(),
    };
    let mut shard = DictStoreShardServer::new(shard);

    while let Ok(req) = req_rx.recv() {
        shard.handle(req); // handle the request!
    }
}

fn main() {
    // start 8 backend shard threads
    let mut req_txs = Vec::new();
    for _ in 0..8 {
        let (req_tx, req_rx) = mpsc::sync_channel(1000);
        std::thread::spawn(move || shard_routine(req_rx));
        req_txs.push(req_tx);
    }

    let addr = "127.0.0.1:50051";
    let dict = MyDictDispatch { req_txs };

    println!("DictStoreServer listening on {}", addr);

    // start the server
    pajamax::Config::new()
        .add_service(DictStoreServer::new(dict))
        .serve(addr)
        .unwrap();
}
