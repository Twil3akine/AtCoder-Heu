use std::time::Instant;

use crate::timer::Timer;

#[derive(Clone, Copy)]
pub struct BeamConfig {
    pub time_limit_ms: u64,
    pub initial_width: usize,
    pub min_width: usize,
    pub max_width: usize,
    pub safety: f64,
    pub ema: f64,
    pub width_change: f64,
    pub branch_hint: usize,
}

impl BeamConfig {
    pub fn new(time_limit_ms: u64) -> Self {
        Self {
            time_limit_ms,
            initial_width: 100,
            min_width: 30,
            max_width: 20_000,
            safety: 0.90,
            ema: 0.10,
            width_change: 0.20,
            branch_hint: 32,
        }
    }
}

pub struct BeamResult<S, A> {
    pub state: S,
    pub eval: i64,
    pub actions: Vec<A>,
}

struct BeamNode<A> {
    parent: Option<usize>,
    action: Option<A>,
}

struct Entry<S> {
    state: S,
    eval: i64,
    node_id: usize,
}

struct Candidate<S, A> {
    state: S,
    eval: i64,
    parent_node_id: usize,
    action: A,
}

#[cfg(test)]
#[derive(Default)]
struct BeamDiagnostics {
    widths: Vec<usize>,
    expanded: Vec<usize>,
    selected: Vec<usize>,
    node_count: usize,
    ema_cost: Option<f64>,
}

/// Maximizing beam search. `eval` is an absolute value (larger is better).
///
/// The root has evaluation 0 because the public API deliberately does not
/// require an initial evaluator. If a turn has no surviving candidates or time
/// expires, the deepest completed beam is returned and its action sequence may
/// be shorter than `turns`.
pub fn beam_search<S, A, F>(
    initial: S,
    turns: usize,
    config: BeamConfig,
    expand: F,
) -> BeamResult<S, A>
where
    A: Clone,
    F: FnMut(&S, usize, &mut Vec<(S, i64, A)>),
{
    beam_search_inner(initial, turns, config, expand, None)
}

fn beam_search_inner<S, A, F>(
    initial: S,
    turns: usize,
    config: BeamConfig,
    mut expand: F,
    #[cfg(test)] mut diagnostics: Option<&mut BeamDiagnostics>,
    #[cfg(not(test))] _diagnostics: Option<()>,
) -> BeamResult<S, A>
where
    A: Clone,
    F: FnMut(&S, usize, &mut Vec<(S, i64, A)>),
{
    validate_config(config);
    let timer = Timer::new(config.time_limit_ms);
    let mut width = config
        .initial_width
        .clamp(config.min_width, config.max_width);
    let mut nodes = vec![BeamNode {
        parent: None,
        action: None,
    }];
    let mut beam = vec![Entry {
        state: initial,
        eval: 0,
        node_id: 0,
    }];
    let mut next_beam = Vec::with_capacity(width);
    let mut scratch = Vec::with_capacity(config.branch_hint);
    let mut candidates = Vec::with_capacity(width.saturating_mul(config.branch_hint));
    let mut ema_cost = None::<f64>;

    for turn in 0..turns {
        if timer.is_over() {
            break;
        }
        let turn_start = Instant::now();
        candidates.clear();
        let mut expanded = 0usize;
        for entry in &beam {
            if expanded.is_multiple_of(64) && timer.is_over() {
                break;
            }
            scratch.clear();
            expand(&entry.state, turn, &mut scratch);
            candidates.reserve(scratch.len());
            for (state, eval, action) in scratch.drain(..) {
                candidates.push(Candidate {
                    state,
                    eval,
                    parent_node_id: entry.node_id,
                    action,
                });
            }
            expanded += 1;
        }
        if candidates.is_empty() {
            break;
        }

        let selected_len = candidates
            .len()
            .min(width);
        if candidates.len() > selected_len {
            candidates.select_nth_unstable_by(selected_len, |a, b| {
                b.eval
                    .cmp(&a.eval)
            });
            candidates.truncate(selected_len);
        }
        next_beam.clear();
        next_beam.reserve(selected_len);
        for candidate in candidates.drain(..) {
            let node_id = nodes.len();
            nodes.push(BeamNode {
                parent: Some(candidate.parent_node_id),
                action: Some(candidate.action),
            });
            next_beam.push(Entry {
                state: candidate.state,
                eval: candidate.eval,
                node_id,
            });
        }
        std::mem::swap(&mut beam, &mut next_beam);

        let elapsed = turn_start
            .elapsed()
            .as_secs_f64();
        let sample = (expanded > 0).then(|| elapsed / expanded as f64);
        ema_cost = update_ema(ema_cost, sample, config.ema);
        #[cfg(test)]
        if let Some(diag) = diagnostics.as_mut() {
            diag.widths
                .push(width);
            diag.expanded
                .push(expanded);
            diag.selected
                .push(selected_len);
            diag.ema_cost = ema_cost;
        }

        if turn + 1 < turns {
            width = next_width(
                width,
                config,
                timer
                    .remaining()
                    .as_secs_f64(),
                turns - turn - 1,
                ema_cost,
            );
        }
    }

    let best_index = beam
        .iter()
        .enumerate()
        .max_by_key(|(_, entry)| entry.eval)
        .map(|(index, _)| index)
        .expect("beam always retains its root");
    let best = beam.swap_remove(best_index);
    let actions = restore_actions(best.node_id, &nodes);
    #[cfg(test)]
    if let Some(diag) = diagnostics.as_mut() {
        diag.node_count = nodes.len();
    }
    BeamResult {
        state: best.state,
        eval: best.eval,
        actions,
    }
}

fn validate_config(config: BeamConfig) {
    assert!(config.min_width >= 1, "min_width must be at least one");
    assert!(config.min_width <= config.max_width);
    assert!(
        config
            .safety
            .is_finite()
            && config.safety > 0.0
            && config.safety <= 1.0
    );
    assert!(
        config
            .ema
            .is_finite()
            && (0.0..=1.0).contains(&config.ema)
    );
    assert!(
        config
            .width_change
            .is_finite()
            && (0.0..=1.0).contains(&config.width_change)
    );
}

fn update_ema(
    previous: Option<f64>,
    sample: Option<f64>,
    alpha: f64,
) -> Option<f64> {
    let Some(sample) = sample.filter(|value| value.is_finite() && *value > 0.0) else {
        return previous;
    };
    Some(match previous {
        Some(old) if old.is_finite() && old > 0.0 => old * (1.0 - alpha) + sample * alpha,
        _ => sample,
    })
}

fn next_width(
    current: usize,
    config: BeamConfig,
    remaining_seconds: f64,
    remaining_turns: usize,
    ema_cost: Option<f64>,
) -> usize {
    let Some(cost) = ema_cost else { return current };
    if remaining_turns == 0 || !cost.is_finite() || cost <= 0.0 {
        return current;
    }
    let budget = remaining_seconds / remaining_turns as f64;
    if !budget.is_finite() || budget <= 0.0 {
        return config.min_width;
    }
    let raw = budget / cost * config.safety;
    if !raw.is_finite() || raw <= 0.0 {
        return current;
    }
    let desired = raw.min(usize::MAX as f64) as usize;
    let delta = if config.width_change == 0.0 {
        0
    } else {
        ((current as f64 * config.width_change).ceil() as usize).max(1)
    };
    desired
        .clamp(current.saturating_sub(delta), current.saturating_add(delta))
        .clamp(config.min_width, config.max_width)
}

fn restore_actions<A: Clone>(
    node_id: usize,
    nodes: &[BeamNode<A>],
) -> Vec<A> {
    let mut actions = Vec::new();
    let mut current = node_id;
    while let Some(parent) = nodes[current].parent {
        actions.push(
            nodes[current]
                .action
                .as_ref()
                .expect("non-root node has an action")
                .clone(),
        );
        current = parent;
    }
    actions.reverse();
    actions
}

#[cfg(test)]
mod tests {
    use super::{beam_search_inner, next_width, update_ema, BeamConfig, BeamDiagnostics};

    fn width_config() -> BeamConfig {
        BeamConfig {
            initial_width: 10,
            min_width: 1,
            max_width: 100,
            ..BeamConfig::new(100)
        }
    }

    #[test]
    fn restores_real_parent_sequence_and_only_keeps_selected_nodes() {
        let config = BeamConfig {
            initial_width: 2,
            min_width: 1,
            max_width: 2,
            width_change: 0.0,
            time_limit_ms: 100,
            ..BeamConfig::new(100)
        };
        let mut diagnostics = BeamDiagnostics::default();
        let result = beam_search_inner(
            0i32,
            4,
            config,
            |state, turn, out| {
                for action in [1i32, 2, 3] {
                    out.push((
                        state + action,
                        (state + action + turn as i32) as i64,
                        action,
                    ));
                }
            },
            Some(&mut diagnostics),
        );
        assert_eq!(
            result
                .actions
                .len(),
            4
        );
        assert_eq!(
            result.state,
            result
                .actions
                .iter()
                .sum()
        );
        assert!(diagnostics
            .widths
            .iter()
            .all(|&w| (1..=2).contains(&w)));
        assert!(diagnostics
            .selected
            .iter()
            .all(|&n| n <= 2));
        assert_eq!(
            diagnostics.node_count,
            1 + diagnostics
                .selected
                .iter()
                .sum::<usize>()
        );
        let parent_action_slots = diagnostics.node_count - 1;
        let copied_history_slots: usize = diagnostics
            .selected
            .iter()
            .enumerate()
            .map(|(turn, &selected)| selected * (turn + 1))
            .sum();
        assert!(parent_action_slots < copied_history_slots);
        assert!(diagnostics
            .ema_cost
            .is_none_or(|x| x.is_finite() && x > 0.0));
    }

    #[test]
    fn zero_turn_returns_root() {
        let result = super::beam_search(
            7usize,
            0,
            BeamConfig::new(1),
            |_, _, out: &mut Vec<(usize, i64, u8)>| {
                out.clear();
            },
        );
        assert_eq!(result.state, 7);
        assert_eq!(result.eval, 0);
        assert!(result
            .actions
            .is_empty());
    }

    #[test]
    fn ema_handles_alpha_and_invalid_samples() {
        assert_eq!(update_ema(None, Some(2.0), 0.1), Some(2.0));
        assert_eq!(update_ema(Some(2.0), Some(10.0), 0.25), Some(4.0));
        assert_eq!(update_ema(Some(2.0), Some(f64::NAN), 0.1), Some(2.0));
        assert_eq!(update_ema(Some(2.0), Some(0.0), 0.1), Some(2.0));
    }

    #[test]
    fn dynamic_width_obeys_rate_and_boundary_rules() {
        let config = width_config();
        // Desired width is large, but default ±20% makes 10 become only 12.
        assert_eq!(next_width(10, config, 100.0, 1, Some(1.0)), 12);
        // A nonzero rate still lets a narrow beam grow by one.
        assert_eq!(next_width(1, config, 100.0, 1, Some(1.0)), 2);
        let frozen = BeamConfig {
            width_change: 0.0,
            ..config
        };
        assert_eq!(next_width(10, frozen, 0.0, 1, Some(1.0)), 1);
        assert_eq!(next_width(10, frozen, 100.0, 1, Some(1.0)), 10);
        assert_eq!(next_width(10, config, 100.0, 0, Some(1.0)), 10);
        assert_eq!(next_width(10, config, 100.0, 1, Some(f64::NAN)), 10);
        assert_eq!(next_width(1, config, 0.0, 1, Some(1.0)), 1);
        let capped = BeamConfig {
            min_width: 5,
            max_width: 6,
            ..config
        };
        assert_eq!(next_width(6, capped, 100.0, 1, Some(1.0)), 6);
        let shrinking = BeamConfig {
            initial_width: 100,
            max_width: 200,
            ..config
        };
        assert_eq!(next_width(100, shrinking, 1.0, 1, Some(1.0)), 80);
    }

    #[test]
    #[ignore = "environment-dependent performance sanity check"]
    fn top_k_selection_performance_sanity() {
        use std::time::Instant;

        const N: usize = 400_000;
        const K: usize = 10_000;
        let values: Vec<i64> = (0..N)
            .map(|i| {
                ((i as u64)
                    .wrapping_mul(1_103_515_245)
                    .rotate_left(17)
                    >> 1) as i64
            })
            .collect();

        let mut sorted = values.clone();
        let sort_start = Instant::now();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        sorted.truncate(K);
        let sort_elapsed = sort_start.elapsed();

        let mut selected = values;
        let select_start = Instant::now();
        selected.select_nth_unstable_by(K, |a, b| b.cmp(a));
        selected.truncate(K);
        let select_elapsed = select_start.elapsed();

        sorted.sort_unstable();
        selected.sort_unstable();
        assert_eq!(selected, sorted);
        let ratio = sort_elapsed.as_secs_f64()
            / select_elapsed
                .as_secs_f64()
                .max(f64::MIN_POSITIVE);
        eprintln!(
            "top-k: sort={sort_elapsed:?}, select={select_elapsed:?}, sort/select={ratio:.2}"
        );
    }

    #[test]
    #[ignore = "environment-dependent time-budget sanity check"]
    fn dynamic_width_uses_time_budget_sanity() {
        use std::hint::black_box;
        use std::time::Instant;

        let config = BeamConfig {
            time_limit_ms: 100,
            initial_width: 8,
            min_width: 8,
            max_width: 512,
            branch_hint: 3,
            ..BeamConfig::new(100)
        };
        let mut diagnostics = BeamDiagnostics::default();
        let start = Instant::now();
        let result = beam_search_inner(
            0u32,
            10_000,
            config,
            |state, _turn, out| {
                let mut work = *state as u64;
                for i in 0..2_000u64 {
                    work = work
                        .wrapping_mul(1_664_525)
                        .wrapping_add(i);
                }
                black_box(work);
                out.push((state + 1, (state + 1) as i64, 1u8));
                out.push((state + 2, (state + 2) as i64, 2u8));
                out.push((state + 3, (state + 3) as i64, 3u8));
            },
            Some(&mut diagnostics),
        );
        let elapsed = start.elapsed();
        assert!(!diagnostics
            .widths
            .is_empty());
        assert!(
            diagnostics
                .widths
                .len()
                >= 2
        );
        assert_eq!(
            result
                .actions
                .len(),
            diagnostics
                .widths
                .len()
        );
        assert_eq!(
            diagnostics
                .expanded
                .len(),
            diagnostics
                .selected
                .len()
        );
        assert_eq!(
            diagnostics
                .expanded
                .len(),
            diagnostics
                .widths
                .len()
        );
        assert!(diagnostics
            .expanded
            .iter()
            .all(|&expanded| expanded > 0));
        assert!(diagnostics
            .selected
            .iter()
            .all(|&selected| selected > 0 && selected <= 512));
        assert!(diagnostics
            .widths
            .iter()
            .all(|&width| (8..=512).contains(&width) && width > 0));
        assert!(diagnostics
            .ema_cost
            .is_some_and(|cost| cost.is_finite() && cost > 0.0));
        for pair in diagnostics
            .widths
            .windows(2)
        {
            let delta = ((pair[0] as f64 * config.width_change).ceil() as usize).max(1);
            assert!(pair[1] >= pair[0].saturating_sub(delta));
            assert!(pair[1] <= pair[0].saturating_add(delta));
        }
        assert!((0.050..=1.0).contains(&elapsed.as_secs_f64()));
        eprintln!(
            "dynamic-width: elapsed={elapsed:?}, expanded={}, nodes={}",
            diagnostics
                .expanded
                .iter()
                .sum::<usize>(),
            diagnostics.node_count,
        );
    }
}
