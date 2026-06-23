//! Incident replay / forensics tool.
//!
//! Usage: replay [scenario_index] [strategy] [--all]
//!   scenario_index : which seeded incident to reconstruct (default 3)
//!   strategy       : reactive | memory | full   (default full)
//!   --all          : show every tick (default: keyframes only)
//!
//! Example: `cargo run --release --bin replay -- 3 full --all`

use aegis::decision::{Arm, DecisionContext, Policy};
use aegis::experiment::train_memory;
use aegis::model::Params;
use aegis::replay::{render, trace};
use aegis::rng::Rng;
use aegis::sim::{gen_scenario, observe, seed_for, ControlConfig};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let idx: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3);
    let strategy = args.get(2).map(String::as_str).unwrap_or("full");
    let show_all = args.iter().any(|a| a == "--all");

    let seed: u64 = 0x5151;
    let p = Params::ground_truth();
    let twin = Params::ground_truth();
    let policy = Policy;
    let memory = train_memory(4000, seed);

    let arm = match strategy {
        "reactive" => Arm::Reactive,
        "memory" => Arm::MemoryOnly,
        _ => Arm::FullAegis,
    };

    let cfg = gen_scenario(&mut Rng::new(seed_for(seed, idx)));
    let mut br = Rng::new(seed_for(seed ^ 0xBEEF, idx));
    let control = ControlConfig::default_loop();

    let t = trace(&cfg, &p, &control, |state, sym| {
        let belief = observe(state, 1.0, &mut br);
        let ctx = DecisionContext {
            symptom: sym,
            belief,
            confidence: 1.0,
            horizon: p.horizon,
            decision_tick: state.decision_tick,
            twin_params: &twin,
            memory: &memory,
            policy: &policy,
        };
        arm.decide(&ctx)
    });

    println!(
        "Aegis Fabric — incident replay  (seed {seed:#x}, scenario #{idx}, strategy: {})\n",
        arm.name()
    );
    render(&t, !show_all);
}
