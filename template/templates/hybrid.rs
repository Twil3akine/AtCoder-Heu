// Copy this file to `src/main.rs` after copying the required crate modules.
use atcoder_heuristic::{anneal, AnnealConfig, Random, Timer};

#[derive(Clone)]
struct State {
    score: i64,
    // TODO: problem specific fields
}

fn main() {
    // TODO: problem specific input and constructive greedy initialization.
    let timer = Timer::new(100);
    let mut random = Random::new(0);
    let mut initial = State { score: 0 };
    while !timer.is_over() {
        // TODO: problem specific greedy construction; retain the best initial state.
        initial.score = initial.score.max(if random.bool() { 1 } else { 0 });
    }

    let result = anneal(
        initial,
        AnnealConfig::new(1800, 100.0, 1.0),
        |state| state.score,
        |_, random| (random.bool(), 0), // TODO: problem specific propose + exact diff
        |_, _| {},                      // TODO: problem specific apply
    );
    // TODO: problem specific output
    println!("{}", result.score);
}
