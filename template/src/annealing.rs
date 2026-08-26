use crate::{random::Random, timer::Timer};

#[derive(Clone, Copy)]
pub struct AnnealConfig {
    pub time_limit_ms: u64,
    pub start_temp: f64,
    pub end_temp: f64,
    pub seed: u64,
    pub check_interval: usize,
}

impl AnnealConfig {
    pub fn new(
        time_limit_ms: u64,
        start_temp: f64,
        end_temp: f64,
    ) -> Self {
        Self {
            time_limit_ms,
            start_temp,
            end_temp,
            seed: 0,
            check_interval: 256,
        }
    }
}

pub struct AnnealResult<S> {
    pub state: S,
    pub score: i64,
    pub iterations: usize,
}

struct AnnealRun<S> {
    #[cfg(test)]
    current: S,
    #[cfg(test)]
    current_score: i64,
    best: S,
    best_score: i64,
    iterations: usize,
}

/// Maximizing simulated annealing.
///
/// `propose` must return `(move, score_after_apply - score_before_apply)`. The
/// move is applied only after acceptance; candidate states are never cloned.
pub fn anneal<S, M, Score, Propose, Apply>(
    initial: S,
    config: AnnealConfig,
    score: Score,
    propose: Propose,
    apply: Apply,
) -> AnnealResult<S>
where
    S: Clone,
    Score: FnMut(&S) -> i64,
    Propose: FnMut(&S, &mut Random) -> (M, i64),
    Apply: FnMut(&mut S, M),
{
    assert!(
        config
            .start_temp
            .is_finite()
            && config.start_temp > 0.0
    );
    assert!(
        config
            .end_temp
            .is_finite()
            && config.end_temp > 0.0
    );
    assert!(config.check_interval >= 1);

    let timer = Timer::new(config.time_limit_ms);
    let run = anneal_with_checkpoint(initial, config, score, propose, apply, |_| {
        (!timer.is_over()).then(|| timer.progress())
    });

    AnnealResult {
        state: run.best,
        score: run.best_score,
        iterations: run.iterations,
    }
}

/// Shared loop with an injectable checkpoint so tests do not depend on wall time.
fn anneal_with_checkpoint<S, M, Score, Propose, Apply, Check>(
    initial: S,
    config: AnnealConfig,
    mut score: Score,
    mut propose: Propose,
    mut apply: Apply,
    mut checkpoint: Check,
) -> AnnealRun<S>
where
    S: Clone,
    Score: FnMut(&S) -> i64,
    Propose: FnMut(&S, &mut Random) -> (M, i64),
    Apply: FnMut(&mut S, M),
    Check: FnMut(usize) -> Option<f64>,
{
    let mut random = Random::new(config.seed);
    let mut current = initial;
    let mut current_score = score(&current);
    let mut best = current.clone();
    let mut best_score = current_score;
    let mut iterations = 0usize;
    let mut temp = config.start_temp;
    loop {
        if iterations.is_multiple_of(config.check_interval) {
            let Some(progress) = checkpoint(iterations) else {
                break;
            };
            temp = config.start_temp * (config.end_temp / config.start_temp).powf(progress);
        }
        let (mv, diff) = propose(&current, &mut random);
        if diff >= 0 || random.f64() < (diff as f64 / temp).exp() {
            apply(&mut current, mv);
            current_score += diff;
            if current_score > best_score {
                best_score = current_score;
                best = current.clone();
            }
        }
        iterations += 1;
    }
    AnnealRun {
        #[cfg(test)]
        current,
        #[cfg(test)]
        current_score,
        best,
        best_score,
        iterations,
    }
}

#[cfg(test)]
mod tests {
    use super::{anneal, anneal_with_checkpoint, AnnealConfig};

    #[derive(Clone)]
    struct State(i64);

    #[test]
    fn improvement_is_accepted_and_diff_matches_state() {
        let mut moves = [7i64].into_iter();
        let run = anneal_with_checkpoint(
            State(0),
            AnnealConfig {
                time_limit_ms: 0,
                start_temp: 1e100,
                end_temp: 1e100,
                seed: 1,
                check_interval: 1,
            },
            |s| s.0,
            |_, _| {
                (
                    moves
                        .next()
                        .unwrap(),
                    7,
                )
            },
            |s, delta| s.0 += delta,
            |iteration| (iteration == 0).then_some(0.0),
        );
        assert_eq!(
            run.current
                .0,
            7
        );
        assert_eq!(run.current_score, 7);
        assert_eq!(
            run.best
                .0,
            7
        );
        assert_eq!(run.best_score, 7);
    }

    #[test]
    fn best_is_kept_separately_from_later_current_state() {
        let mut moves = [5i64, -1, -1].into_iter();
        let run = anneal_with_checkpoint(
            State(0),
            AnnealConfig {
                time_limit_ms: 0,
                start_temp: 1e100,
                end_temp: 1e100,
                seed: 1,
                check_interval: 1,
            },
            |s| s.0,
            |_, _| {
                let delta = moves
                    .next()
                    .unwrap();
                (delta, delta)
            },
            |s, delta| s.0 += delta,
            |iteration| (iteration < 3).then_some(0.0),
        );
        assert_eq!(
            run.current
                .0,
            3
        );
        assert_eq!(run.current_score, 3);
        assert_eq!(
            run.best
                .0,
            5
        );
        assert_eq!(run.best_score, 5);
    }

    #[test]
    fn zero_ms_does_not_propose_a_move() {
        let mut proposed = false;
        let result = anneal(
            State(0),
            AnnealConfig::new(0, 10.0, 1.0),
            |s| s.0,
            |_, _| {
                proposed = true;
                (1i64, 1)
            },
            |s, delta| s.0 += delta,
        );
        assert!(!proposed);
        assert_eq!(result.iterations, 0);
        assert_eq!(result.score, 0);
    }
}
