// Copy this file to `src/main.rs` after copying the required crate modules.
use atcoder_heuristic::{anneal, AnnealConfig, Random};

#[derive(Clone)]
struct State {
    score: i64,
    // TODO: problem specific fields
}

struct Move {
    delta: i64,
    // TODO: problem specific move data
}

fn main() {
    // TODO: problem specific input and initial solution
    let initial = State { score: 0 };
    let config = AnnealConfig::new(1900, 100.0, 1.0);
    let result = anneal(
        initial,
        config,
        |state| state.score,
        |state, random: &mut Random| {
            // TODO: problem specific: choose a move and calculate exact diff.
            let delta = if random.bool() { 1 } else { -1 };
            let _ = state;
            (Move { delta }, delta)
        },
        |state, mv| {
            // TODO: problem specific: apply exactly the move used for diff.
            state.score += mv.delta;
        },
    );
    // TODO: problem specific output
    println!("{}", result.score);
}
