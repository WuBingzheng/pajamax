// Example for dispatch mode.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use pajamax::status::{Code, Status};

use dict_store::*;

// import the generated code from .proto
mod dict_store {
    pajamax::include_proto!("dict_store");
}

// Here we have 2 servers: MyDictFront and MyDictShard

// This is the front server.
//
// It dispatches most requests to backend shards by channels, so it
// contains the channel-send-end list.
// However, it handles some requests directly too, e.g. `Stats`.
//
// The instance of this struct is not global. Each connection has
// its own instance. So it need implement `Clone`, and we use `Arc`
// to swap the channel list.
struct MyDictFront {
    req_txs: Vec<DictStoreRequestTx>,
}

// This is the backend server.
//
// Most requests are dispatched to some shard by item key or shard number.
//
// Each backend server thread has one instance. It's permanent. It's
// created in each thread, so it's not need to be `Clone`.
struct MyDictShard {
    dict: HashMap<String, f64>,
}

// Methods for front server.
//
// Here is no get/set/delete methods which are always handled in backend shard server.
impl DictStoreDispatch for MyDictFront {
    fn set(&self, req: &Entry) -> &DictStoreRequestTx {
        self.pick_req_tx(&req.key)
    }

    fn get(&self, req: &Key) -> &DictStoreRequestTx {
        self.pick_req_tx(&req.key)
    }

    fn delete(&self, req: &Key) -> &DictStoreRequestTx {
        self.pick_req_tx(&req.key)
    }

    fn list_shard(&self, req: &ListShardRequest) -> &DictStoreRequestTx {
        todo!()
    }

    fn stats(&self, req: &EmptyRequest) -> &DictStoreRequestTx {
        todo!()
    }
}

// Methods for backend server.
//
// Here is no stats methods which is always handled in front server.
impl DictStoreShard for MyDictShard {
    fn set(&mut self, req: Entry) -> Result<SetReply, Status> {
        let old_value = self.dict.insert(req.key, req.value);
        if old_value.is_none() {
            TOTAL_COUNT.fetch_add(1, Ordering::Relaxed);
        }
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
            Some(value) => {
                TOTAL_COUNT.fetch_sub(1, Ordering::Relaxed);
                Ok(Value { value })
            }
            None => Err(Status {
                code: Code::NotFound,
                message: format!("key: {}", req.key),
            }),
        }
    }

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
    fn stats(&mut self, _req: EmptyRequest) -> Result<StatsReply, Status> {
        todo!()
    }
}

// some business code
static TOTAL_COUNT: AtomicUsize = AtomicUsize::new(0);

impl MyDictFront {
    fn pick_req_tx(&self, key: &str) -> &DictStoreRequestTx {
        let hash = hash_key(key) as usize;
        let i = hash % self.req_txs.len();
        &self.req_txs[i]
    }
}

fn hash_key<K>(key: &K) -> u64
where
    K: std::hash::Hash + ?Sized,
{
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn shard_routine(req_rx: DictStoreRequestRx) {
    let shard = MyDictShard {
        dict: HashMap::new(),
    };
    let mut shard = DictStoreShardServer::new(shard);

    while let Ok(req) = req_rx.recv() {
        shard.handle(req);
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
    let dict = MyDictFront { req_txs };

    println!("DictStoreServer listening on {}", addr);

    // start the server
    // By now we have not support configurations and multiple service,
    // so this API is simpler than tonic's.
    pajamax::Config::new()
        .add_service(DictStoreServer::new(dict))
        .serve(addr)
        .unwrap();
}
