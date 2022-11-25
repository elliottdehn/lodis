# Lodis

> Local Distributed Server

_Powered by [Actix](https://github.com/actix/actix-web)._ (revealer of [issue(s)?](https://github.com/actix/actix-web/issues/2932))

## Features

- Deploys, maintains and upgrades itself.
- Delivers resilience with local resources.
- Trivializes basic continuous integration.
- Built for builders: all code is in one file.

**...an excellent place to start development!**

## Contents

Since `lodis` is just one file, readers may find it easier to 
understand by reading the `main` function here:

```rust
/// This is a hard-to-kill web-server, and an easy-to-upgrade binary.
/// It does nothing but deploy, maintain & upgrade itself.. have fun!

// "Upgrade all hosts" is ~TRIVIAL: `cargo build` (overwrite binary).
// >> Use nginx (or any LB) to distribute incoming load across ports.

// "Killing all hosts" is ~IMPOSSIBLE (out-of-band kill-all REQUIRED):
// >> Use `chmod 000 host` to slay all of the hosted servers at once.

#[actix_web::main]
async fn main() -> io::Result<()> {
  let argv = env::args().collect::<Vec<String>>();
  let exec = &argv[0];
  let port = argv.get(1).map(|s| s.parse::<Port>().x()).unwrap_or(8080);
  let card = argv // Target cardinality of healthy server distribution.
    .get(2)
    .map(|s| s.parse::<usize>().x())
    .unwrap_or_else(num_cpus::get_physical);

  // Create this server and start listening on nearest available port!
  let port_deploy = if card > 0 {
    gen_deploy((port, None)) // Deploy FROM port (may/will wander).
  } else {
    gen_deploy((port, Some(port))) // Deploy TO port (never wander).
  }
  .await
  .expect("Failed to deploy the binary"); // Many will try; one wins.

  // HAPPENS ONCE: Recursive self-execution to achieve N-cardinality.
  if card > 1 {
    let mut exec_init = Command::new(exec);
    exec_init.arg((port_deploy + 1).to_string()); // Wanders from last.
    exec_init.arg((card as u16 - 1).to_string());
    exec_init.spawn().expect("Failed to spawn the child"); // No block!
  } // When single-servers get re-deployed a cardinality 0 is provided.

  // EVERY SECOND: The server has a 50% chance to abort itself QUIETLY!
  // >> This is just for demonstration and SHOULD NOT be in production.
  let mut rng = rand::thread_rng();
  loop {
    for host_port in get_host_ports() {
      let response =
        reqwest::get(format!("http://localhost:{}/__/time", host_port))
          .await;
      match response {
        Ok(_) => {}
        Err(_) => /* "Oh dear, you are dead!" */ gen_respawn(host_port)
          .await,
      }
    }
    // ...not anymore!
    if rng.gen::<f64>() > 0.5 {
      abort(); /* "Oh dear, I am dead!" */
    } else {
      task::sleep(Duration::from_secs(1)).await;
    }
  }
}
...
```
