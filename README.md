# Lodis

> Local Distributed Server

_(pedantic: "loh-dih-ess", informal: "loh-dis")_

**Powered by [Actix](https://github.com/actix/actix-web).** _(revealer of [issue(s)?](https://github.com/actix/actix-web/issues/2932))_

![Screenshot_2022-11-25-16-10-53-97_572064f74bd5f9fa804b05334aa4f912](https://user-images.githubusercontent.com/8174594/204055821-d7004451-7e52-4212-a4c4-61f9b0f8f31c.jpg)

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

## Results

I was able to achieve a binary executable migration on a still-running group of hosts by using `cargo build` which indicates (but does not prove, of course) the viability of an executable-overwrite oriented continuous integration system. The web service in question is objectively far more stable within the `lodis` framework than it is outside of it, and this is demonstrated by the continuous sustainment of a time-service despite process abortion at a rate of 50% every second. In production, that can be of course tuned down to a graceful shutdown triggered by an external signal when it's time to migrate--but the continuous-`abort` resilience capability remains nonetheless.

1. It's possible for a relatively small (N=10) cohort of processes to sustain the availability of their services via network *(not system)* interfaces, even despite a strange condition existing in the world where each process in the cohort has a 50% chance of `abort`-ing itself every second (or on boot, even during initial deployment).
2. The minimum viable continuous integration of a Rust binary is (in theory) `cargo build` and, in-fact, one can achieve that without loss of availability in their web services via this design I've demonstrated above--the desirability of `cargo build` as a continuous integration solution is a question left up to readers, however the _feasibility_ of such (or similar, such as overwriting the executable binary by some other means) has been at this point proven "in the lab, and therefore in practice." (joking)
3. There seems to be **strong evidence** (although I will not say "conclusive" or "decisive") that there exists a race-condition in levels at-or-below the Actix Web framework, down through the Rust language all the way to (and perhaps including) OSX itself, that results in a violation of the "exclusive port-binding invariant" that exists in most (all?) modern systems: [(see issue)](https://github.com/actix/actix-web/issues/2932)

[(see discussion)](https://rust-lang.zulipchat.com/#narrow/stream/122651-general/topic/Distributed.20web-server.20research/near/312117377)
