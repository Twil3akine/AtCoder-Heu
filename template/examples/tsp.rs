//! TSP simulated annealing: 2-opt evaluates only the two replaced edges.
use std::io::{self, Read};

use atcoder_heuristic::{anneal, AnnealConfig, Random};

type Point = (i64, i64);

#[derive(Clone)]
struct State {
    route: Vec<usize>,
    position: Vec<usize>,
    /// Negative tour length: annealing always maximizes.
    score: i64,
}

struct TwoOpt {
    left: usize,
    right: usize,
    diff: i64,
}

fn edge_cost(points: &[Point], a: usize, b: usize) -> i64 {
    let dx = points[a].0 - points[b].0;
    let dy = points[a].1 - points[b].1;
    dx * dx + dy * dy
}

fn tour_cost(route: &[usize], points: &[Point]) -> i64 {
    (0..route.len())
        .map(|i| edge_cost(points, route[i], route[(i + 1) % route.len()]))
        .sum()
}

/// Score change for reversing route[left..=right]. Only two edges change.
fn two_opt_diff(state: &State, left: usize, right: usize, points: &[Point]) -> i64 {
    let n = state.route.len();
    let a = state.route[left - 1];
    let b = state.route[left];
    let c = state.route[right];
    let d = state.route[(right + 1) % n];
    let old = edge_cost(points, a, b) + edge_cost(points, c, d);
    let new = edge_cost(points, a, c) + edge_cost(points, b, d);
    old - new
}

fn apply_two_opt(state: &mut State, mv: TwoOpt) {
    state.route[mv.left..=mv.right].reverse();
    for index in mv.left..=mv.right {
        state.position[state.route[index]] = index;
    }
    state.score += mv.diff;
}

fn nearest_candidates(points: &[Point], k: usize) -> Vec<Vec<usize>> {
    (0..points.len())
        .map(|from| {
            let mut others: Vec<usize> = (0..points.len()).filter(|&to| to != from).collect();
            others.sort_unstable_by_key(|&to| edge_cost(points, from, to));
            others.truncate(k.min(others.len()));
            others
        })
        .collect()
}

fn read_points() -> Vec<Point> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut it = input.split_whitespace();
    let Some(n) = it.next().and_then(|x| x.parse::<usize>().ok()) else {
        return vec![(0, 0), (4, 0), (7, 3), (6, 8), (1, 9), (-2, 4)];
    };
    (0..n)
        .map(|_| {
            let x = it.next().unwrap().parse().unwrap();
            let y = it.next().unwrap().parse().unwrap();
            (x, y)
        })
        .collect()
}

fn main() {
    let points = read_points();
    if points.len() < 4 {
        for city in 0..points.len() {
            println!("{}", city);
        }
        return;
    }
    let route: Vec<usize> = (0..points.len()).collect();
    let mut position = vec![0; points.len()];
    for (index, &city) in route.iter().enumerate() {
        position[city] = index;
    }
    let initial = State {
        score: -tour_cost(&route, &points),
        route,
        position,
    };
    // Cheap nearest-city candidates avoid testing arbitrary 2-opt endpoints first.
    let nearest = nearest_candidates(&points, 16);
    let result = anneal(
        initial,
        AnnealConfig::new(1900, 5_000.0, 1.0),
        |state| state.score,
        |state, random: &mut Random| {
            let n = state.route.len();
            let city = state.route[random.usize(1..n)];
            let mut endpoint = city;
            for _ in 0..4 {
                let list = &nearest[city];
                let candidate = list[random.usize(0..list.len())];
                if state.position[candidate] != 0 {
                    endpoint = candidate;
                    break;
                }
            }
            let (left, right) = if endpoint != city {
                let x = state.position[city];
                let y = state.position[endpoint];
                if x < y {
                    (x, y)
                } else {
                    (y, x)
                }
            } else {
                let left = random.usize(1..n - 1);
                (left, random.usize(left + 1..n))
            };
            let diff = two_opt_diff(state, left, right, &points);
            (TwoOpt { left, right, diff }, diff)
        },
        apply_two_opt,
    );
    for city in result.state.route {
        println!("{}", city);
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_two_opt, tour_cost, two_opt_diff, Point, State, TwoOpt};

    #[test]
    fn two_opt_diff_and_apply_match_full_tour_cost() {
        let points: Vec<Point> = vec![(0, 0), (3, 1), (6, 0), (7, 5), (3, 8), (-1, 5)];
        let route: Vec<usize> = (0..points.len()).collect();
        let position: Vec<usize> = (0..points.len()).collect();
        let before = tour_cost(&route, &points);
        let mut state = State {
            route,
            position,
            score: -before,
        };
        let diff = two_opt_diff(&state, 1, 4, &points);
        apply_two_opt(
            &mut state,
            TwoOpt {
                left: 1,
                right: 4,
                diff,
            },
        );
        let after = tour_cost(&state.route, &points);
        assert_eq!(diff, before - after);
        assert_eq!(state.score, -after);
        let mut permutation = state.route.clone();
        permutation.sort_unstable();
        assert_eq!(permutation, (0..points.len()).collect::<Vec<_>>());
        for (index, &city) in state.route.iter().enumerate() {
            assert_eq!(state.position[city], index);
        }
    }
}
