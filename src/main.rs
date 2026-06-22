//! Aegis Fabric MVP runner.
//!
//! Usage: aegis [n_eval] [n_train]
//!   n_eval  — number of evaluation scenarios per arm (default 4000)
//!   n_train — number of memory-training scenarios     (default 8000)

use aegis::experiment;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n_eval: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4000);
    let n_train: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8000);
    let seed: u64 = 0x5151;
    experiment::run(n_eval, n_train, seed);
}
