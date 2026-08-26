// Copy this file to `src/main.rs` after copying the required crate modules.
use atcoder_heuristic::{beam_search, BeamConfig, Input, Output};

struct State {
    value: i64,
    // TODO: problem specific fields
}

#[derive(Clone)]
struct Action(i64);

fn main() {
    let mut input = Input::new();
    let _ = &mut input;
    let mut output = Output::new();
    // TODO: problem specific input, initial state, and turn count.
    let initial = State { value: 0 };
    let result = beam_search(initial, 20, BeamConfig::new(1900), |state, _turn, out| {
        // TODO: problem specific: push owned (next_state, absolute_eval, action).
        out.push((
            State {
                value: state.value + 1,
            },
            state.value + 1,
            Action(1),
        ));
    });
    // TODO: problem specific output using result.state / result.actions.
    let action_sum: i64 = result
        .actions
        .iter()
        .map(|action| action.0)
        .sum();
    output.print(result.eval);
    output.print(" ");
    output.println(action_sum);
    output.flush();
}
