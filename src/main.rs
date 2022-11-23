/// This is a hard-to-kill web-server, and an easy-to-upgrade binary!
// "Upgrade all hosts" is ~TRIVIAL: `cargo build` (overwrite binary).
// "Killing all hosts" is ~IMPOSSIBLE (out-of-band kill-all required):
// > Use `chmod 000 host` to slay all of the hosted servers at once!
use actix_web::web::Buf;
use actix_web::{get, App, HttpResponse, HttpServer, Responder};
use async_std::sync::RwLock;
use async_std::task;
use futures::FutureExt;
use lazy_static::lazy_static;
use mapcomp::{hashmapc, vecc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::future::Future;
use std::hash::Hash;
use std::io::prelude::*;

use file_lock::{FileLock, FileOptions};
use rand::Rng;
use std::process::{abort, id, Command};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, io};

#[get("/__/time")]
async fn _gen_time() -> impl Responder {
  ok(&now())
}

#[get("/__/kill")] // Good luck!
async fn _gen_kill() -> impl Responder {
  abort();
  ok(&true) // NOTE: Unreachable.
}

#[actix_web::main]
async fn main() -> io::Result<()> {
  let argv = env::args().collect::<Vec<String>>();
  let exec = &argv[0];
  let port = argv.get(1).map(|s| s.parse::<Port>().x()).unwrap_or(8080);
  let card = argv // target cardinality of healthy server distribution.
    .get(2)
    .map(|s| s.parse::<usize>().x())
    .unwrap_or_else(num_cpus::get_physical);

  // Create the server and start listening on nearest available port.
  let port_deploy = if card > 0 {
    gen_deploy((port, None)) // Deploy FROM port (with wandering).
  } else {
    gen_deploy((port, Some(port))) // Deploy AT port (zero wandering).
  }
  .await
  .expect("Failed to deploy the binary"); // Many will try; one wins.

  // HAPPENS ONCE: Recursive self-execution to achieve N-cardinality.
  if card > 1 {
    let mut exec_init = Command::new(exec);
    exec_init.arg((port_deploy + 1).to_string()); // Wanders from last.
    exec_init.arg((card as u16 - 1).to_string());
    exec_init.spawn().expect("Failed to spawn the child"); // No block.
  } // When single-servers get re-deployed, cardinality 0 is provided.

  // EVERY SECOND: The server has a 50% chance to abort itself QUIETLY!
  // > This is just for demonstration and SHOULD NOT be produced.
  let mut rng = rand::thread_rng();
  loop {
    for host_port in get_host_ports() {
      let response =
        reqwest::get(format!("http://localhost:{}/__/time", host_port))
          .await;
      match response {
        Ok(_) => {}
        Err(_) => gen_spawn(host_port).await,
      }
    }
    if rng.gen::<f64>() > 0.5 {
      abort();
    } else {
      task::sleep(Duration::from_secs(1)).await;
    }
  }
}

/* PROJECT INFRASTRUCTURE */

async fn gen_deploy(ports: (Port, Option<Port>)) -> Option<Port> {
  gen_bind(ports, |port_try| {
    async move {
      let srv = HttpServer::new(|| {
        App::new().service(_gen_time).service(_gen_kill)
      });
      let res = srv
        .bind(("localhost", port_try))
        .map(|s| (&mut s.run()).now_or_never());
      match res {
        Ok(_) => {
          _append_one(
            "host",
            &LocalHost {
              pid: id(),
              port: port_try,
            },
          );
          Ok(port_try)
        }
        _ => Err(port_try),
      }
    }
  })
  .await
}

async fn gen_bind<F, Fut>(
  range: (Port, Option<Port>),
  bind: F,
) -> Option<Port>
where
  F: Fn(Port) -> Fut,
  Fut: Future<Output = Result<Port, Port>>,
{
  let mut result = None;
  let lo = range.0;
  let hi = match range.1 {
    Some(p) => p + 1,
    None => u16::MAX,
  };
  for port in lo..hi {
    match (bind)(port).await {
      Ok(_) => {
        result = Some(port);
        break;
      }
      Err(_) => {
        result = None;
      }
    }
  }
  result
}

fn ok<T: Serialize>(t: &T) -> impl Responder {
  HttpResponse::Ok().body(json_encode(t))
}

type Port = u16;
type Pid = u32;
#[derive(Serialize, Deserialize)]
struct LocalHost {
  pid: Pid,
  port: Port,
}

fn get_hosts() -> Vec<LocalHost> {
  let hosts = _read_multi::<LocalHost>("host")
    .iter()
    .map(|host| (host.port, host.pid))
    .collect::<Vec<(Port, Pid)>>();
  HashMap::from_entries_multi(&hosts)
    .iter()
    .map(|(port, pids)| {
      LocalHost {
        pid: **pids.last().x(),
        port: **port,
      }
    })
    .collect()
}

fn get_host_ports() -> HashSet<Port> {
  get_hosts()
    .iter()
    .map(|h| h.port)
    .collect::<HashSet<Port>>()
}

async fn gen_spawn(port: Port) {
  let argv = env::args().collect::<Vec<String>>();
  let exec = &argv[0];
  let mut exec_init = Command::new(exec);
  exec_init.arg(port.to_string());
  exec_init.arg(0.to_string());
  exec_init.spawn().expect("Failed to spawn the child");
}

/* GENERAL INFRASTRUCTURE */

type Time = u128;

fn now() -> Time {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_micros()
}

fn json_decode<T: DeserializeOwned>(s: &str) -> T {
  serde_json::from_str::<T>(s).unwrap()
}

fn json_encode<T: Serialize>(t: &T) -> String {
  serde_json::to_string(&t).unwrap()
}

trait X<D> {
  fn x(self) -> D;
}

impl<D> X<D> for Option<D> {
  fn x(self) -> D {
    self.unwrap()
  }
}

impl<D, E> X<D> for Result<D, E> {
  fn x(self) -> D {
    match self {
      Ok(d) => d,
      Err(_) => panic!("Expected a result!"),
    }
  }
}

lazy_static! {
  static ref __SLOG: RwLock<Vec<String>> = RwLock::new(Vec::new());
}

fn slog(s: &str) {
  let fo = FileOptions::new().read(false).append(true).create(true);
  let mut file_lock = match FileLock::lock("./slog", true, fo) {
    Ok(lock) => lock,
    Err(err) => panic!("Error getting file write lock: {}", err),
  };
  file_lock
    .file
    .write_all(
      format!("(time: {}) (pid: {}) {s}\n", now(), id()).as_bytes(),
    )
    .expect("Failed to write");
  file_lock.unlock().x();
}

fn _read(path: &str) -> Vec<u8> {
  let mut v = vec![];
  OpenOptions::new()
    .read(true)
    .write(false)
    .open(path)
    .expect(path)
    .read_to_end(&mut v)
    .expect(path);
  v
}

fn _read_one<T: DeserializeOwned>(path: &str) -> T {
  json_decode(&String::from_utf8(_read(path)).expect("Invalid format"))
}

fn _append_one<'a, T: Serialize>(path: &str, item: &'a T) -> &'a T {
  let fo = FileOptions::new().read(false).append(true).create(true);
  let mut file_lock = match FileLock::lock(path, true, fo) {
    Ok(lock) => lock,
    Err(err) => panic!("Error getting write lock: {}", err),
  };
  file_lock
    .file
    .write_all(format!("{}\n", json_encode(item)).as_bytes())
    .expect("Failed to write");
  file_lock.unlock().expect("Failed to unlock");
  item
}

fn _read_multi<T: DeserializeOwned>(path: &str) -> Vec<T> {
  serde_json::de::Deserializer::from_reader(_read(path).reader())
    .into_iter::<T>()
    .map(|r| r.expect("Invalid format"))
    .collect()
}

trait Index<K: Eq + Hash, V> {
  // static
  fn from_entries(entries: &[(K, V)]) -> HashMap<&K, &V>;
  fn from_entries_multi(entries: &[(K, V)]) -> HashMap<&K, Vec<&V>>;

  // object
  fn getx(&self, key: &K) -> &V;
  fn getx_mut(&mut self, key: &K) -> &mut V;
  fn entries(&self) -> Vec<(&K, &V)>;

  fn add(&mut self, key: K, val: V) -> (&V, bool);
  fn append<F>(&mut self, key: K, val: V, cat: F) -> (&V, bool)
  where
    F: Fn(&mut V, V);
}

trait Keyset<K: Hash + Eq> {}

trait Vector<T> {
  fn unique_by<F, K>(&self, f: F) -> Vec<&T>
  where
    K: Eq + Hash,
    F: Fn(&T) -> K;
}
impl<T> Vector<T> for Vec<T> {
  fn unique_by<F, K>(&self, uq: F) -> Vec<&T>
  where
    K: Eq + Hash,
    F: Fn(&T) -> K,
  {
    let seen: HashSet<K> = HashSet::new();
    let mut items = Vec::new();
    for item in self {
      let key = (uq)(item);
      if !seen.contains(&key) {
        items.push(item);
      }
    }
    items
  }
}

impl<K: Eq + Hash, V> Index<K, V> for HashMap<K, V> {
  fn from_entries(entries: &[(K, V)]) -> HashMap<&K, &V> {
    hashmapc! {
      &entry.0 => &entry.1;
      for entry in entries
    }
  }

  fn from_entries_multi(entries: &[(K, V)]) -> HashMap<&K, Vec<&V>> {
    let mut m: HashMap<&K, Vec<&V>> = HashMap::new();
    for entry in entries {
      let e = m.get_mut(&entry.0);
      if let Some(..) = e {
        e.unwrap().push(&entry.1);
      } else {
        m.insert(&entry.0, vec![&entry.1]);
      }
    }
    m
  }

  fn getx(&self, key: &K) -> &V {
    self.get(key).unwrap()
  }

  fn getx_mut(&mut self, key: &K) -> &mut V {
    self.get_mut(key).unwrap()
  }

  fn entries(&self) -> Vec<(&K, &V)> {
    vecc![
      (key, self.getx(key));
      for key in self.keys()
    ]
  }

  fn add(&mut self, key: K, val: V) -> (&V, bool) {
    let added;
    let value = match self.entry(key) {
      Entry::Occupied(o) => {
        added = false;
        o.into_mut()
      }
      Entry::Vacant(v) => {
        added = true;
        v.insert(val)
      }
    };
    (value, added)
  }

  fn append<F>(&mut self, key: K, val: V, cat: F) -> (&V, bool)
  where
    F: Fn(&mut V, V),
  {
    let added;
    let value = match self.entry(key) {
      Entry::Occupied(base) => {
        added = false;
        let v = base.into_mut();
        (cat)(v, val);
        v
      }
      Entry::Vacant(v) => {
        added = true;
        v.insert(val)
      }
    };
    (value, added)
  }
}
