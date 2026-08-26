// Copy this file to `src/main.rs` after copying the required crate modules.
use atcoder_heuristic::{Random, Timer};

#[derive(Clone)]
struct State {
    score: i64,
    // TODO: problem specific fields
}

fn main() {
    // TODO: problem specific input and initial solution
    let mut current = State { score: 0 };
    let mut best = current.clone();
    let timer = Timer::new(1900);
    let mut random = Random::new(0);

    while !timer.is_over() {
        // TODO: problem specific greedy construction / randomized improvement
        let candidate_score = current.score + if random.bool() { 1 } else { 0 };
        if candidate_score >= current.score {
            current.score = candidate_score;
        }
        if current.score > best.score {
            best = current.clone();
        }
    }

    // TODO: problem specific output
    println!("{}", best.score);
}
