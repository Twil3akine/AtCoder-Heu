//! A compact light-placement SA example for N <= 16.
//!
//! It precomputes rays, uses `BitSet<4>`, retains coverage counts, samples a
//! static candidate list, and evaluates only cells touched by one moved light.
use std::cmp::Reverse;
use std::io::{self, Read};

use atcoder_heuristic::{anneal, AnnealConfig, BitSet, Random};

const MAX_CELLS: usize = 256;
const ADJ_PENALTY: i64 = 8;

#[derive(Clone)]
struct State {
    placed: BitSet<4>,
    covered: BitSet<4>,
    lights: Vec<usize>,
    count: Vec<u8>,
    score: i64,
}

struct Move {
    light_index: usize,
    to: usize,
    diff: i64,
}

struct Grid<'a> {
    rays: &'a [Vec<usize>],
    neighbors: &'a [Vec<usize>],
    weights: &'a [i64],
}

fn profit(weight: i64, count: u8) -> i64 {
    match count {
        0 => 0,
        1 => weight,
        2 => weight + weight / 3,
        k => weight - (k as i64 - 2) * weight / 2,
    }
}

fn id(n: usize, r: usize, c: usize) -> usize {
    r * n + c
}

fn build_rays(n: usize, wall: &[bool]) -> (Vec<BitSet<4>>, Vec<Vec<usize>>) {
    let mut masks = vec![BitSet::new(); n * n];
    let mut cells = vec![Vec::new(); n * n];
    for r in 0..n {
        for c in 0..n {
            let source = id(n, r, c);
            if wall[source] {
                continue;
            }
            let mut ray = Vec::with_capacity(n * 4);
            ray.push(source);
            for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                let (mut nr, mut nc) = (r as isize + dr, c as isize + dc);
                while nr >= 0 && nr < n as isize && nc >= 0 && nc < n as isize {
                    let target = id(n, nr as usize, nc as usize);
                    if wall[target] {
                        break;
                    }
                    ray.push(target);
                    nr += dr;
                    nc += dc;
                }
            }
            let mut mask = BitSet::new();
            for &cell in &ray {
                mask.insert(cell);
            }
            masks[source] = mask;
            cells[source] = ray;
        }
    }
    (masks, cells)
}

fn build_neighbors(n: usize, wall: &[bool]) -> Vec<Vec<usize>> {
    let mut result = vec![Vec::new(); n * n];
    for r in 0..n {
        for c in 0..n {
            let cell = id(n, r, c);
            if wall[cell] {
                continue;
            }
            for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                let (nr, nc) = (r as isize + dr, c as isize + dc);
                if nr >= 0 && nr < n as isize && nc >= 0 && nc < n as isize {
                    let next = id(n, nr as usize, nc as usize);
                    if !wall[next] {
                        result[cell].push(next);
                    }
                }
            }
        }
    }
    result
}

fn add_light(
    state: &mut State,
    at: usize,
    rays: &[Vec<usize>],
    neighbors: &[Vec<usize>],
    weights: &[i64],
) {
    for &cell in &rays[at] {
        state.score +=
            profit(weights[cell], state.count[cell] + 1) - profit(weights[cell], state.count[cell]);
        state.count[cell] += 1;
        state.covered.insert(cell);
    }
    for &next in &neighbors[at] {
        if state.placed.contains(next) {
            state.score -= ADJ_PENALTY;
        }
    }
    state.placed.insert(at);
    state.lights.push(at);
}

fn exact_move_diff(
    state: &State,
    from: usize,
    to: usize,
    grid: &Grid<'_>,
    delta: &mut [i16],
    touched: &mut Vec<usize>,
) -> i64 {
    for &cell in &grid.rays[from] {
        if delta[cell] == 0 {
            touched.push(cell);
        }
        delta[cell] -= 1;
    }
    for &cell in &grid.rays[to] {
        if delta[cell] == 0 {
            touched.push(cell);
        }
        delta[cell] += 1;
    }
    let mut diff = 0;
    for &cell in touched.iter() {
        let old = state.count[cell];
        let new = (old as i16 + delta[cell]) as u8;
        diff += profit(grid.weights[cell], new) - profit(grid.weights[cell], old);
        delta[cell] = 0;
    }
    touched.clear();
    for &next in &grid.neighbors[from] {
        if state.placed.contains(next) {
            diff += ADJ_PENALTY;
        }
    }
    for &next in &grid.neighbors[to] {
        if next != from && state.placed.contains(next) {
            diff -= ADJ_PENALTY;
        }
    }
    diff
}

fn apply_move(state: &mut State, mv: Move, rays: &[Vec<usize>]) {
    let from = state.lights[mv.light_index];
    for &cell in &rays[from] {
        state.count[cell] -= 1;
        if state.count[cell] == 0 {
            state.covered.remove(cell);
        }
    }
    state.placed.remove(from);
    for &cell in &rays[mv.to] {
        state.count[cell] += 1;
        state.covered.insert(cell);
    }
    state.placed.insert(mv.to);
    state.lights[mv.light_index] = mv.to;
    state.score += mv.diff;
}

fn default_instance() -> (usize, usize, Vec<bool>, Vec<i64>) {
    let n = 8;
    let mut wall = vec![false; n * n];
    for &(r, c) in &[(2, 2), (2, 5), (5, 3), (6, 6)] {
        wall[id(n, r, c)] = true;
    }
    let weights = (0..n * n)
        .map(|cell| {
            if wall[cell] {
                0
            } else {
                3 + (cell * 7 % 17) as i64
            }
        })
        .collect();
    (n, 6, wall, weights)
}

fn read_instance() -> (usize, usize, Vec<bool>, Vec<i64>) {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut it = input.split_whitespace();
    let (Some(n), Some(m)) = (
        it.next().and_then(|x| x.parse::<usize>().ok()),
        it.next().and_then(|x| x.parse::<usize>().ok()),
    ) else {
        return default_instance();
    };
    assert!(
        n <= 16 && n * n <= MAX_CELLS,
        "this example supports N <= 16"
    );
    let mut wall = vec![false; n * n];
    for r in 0..n {
        let row = it.next().expect("missing grid row").as_bytes();
        assert_eq!(row.len(), n, "grid width mismatch");
        for c in 0..n {
            wall[id(n, r, c)] = row[c] == b'#';
        }
    }
    let weights = (0..n * n)
        .map(|cell| {
            if wall[cell] {
                0
            } else {
                it.next()
                    .expect("missing weight")
                    .parse()
                    .expect("invalid weight")
            }
        })
        .collect();
    (n, m, wall, weights)
}

fn main() {
    let (n, light_count, wall, weights) = read_instance();
    let (masks, rays) = build_rays(n, &wall);
    let neighbors = build_neighbors(n, &wall);
    let mut candidates: Vec<usize> = (0..n * n).filter(|&cell| !wall[cell]).collect();
    // A static cheap score gives the candidate list; exact diff is still used below.
    candidates.sort_unstable_by_key(|&cell| {
        Reverse(
            rays[cell]
                .iter()
                .map(|&target| weights[target])
                .sum::<i64>(),
        )
    });
    // Keep only a compact static list: M for construction plus exploration slack.
    candidates.truncate(light_count.saturating_add(32).min(candidates.len()));
    let mut initial = State {
        placed: BitSet::new(),
        covered: BitSet::new(),
        lights: Vec::new(),
        count: vec![0; n * n],
        score: 0,
    };
    for &cell in candidates.iter().take(light_count.min(candidates.len())) {
        add_light(&mut initial, cell, &rays, &neighbors, &weights);
    }
    if initial.lights.is_empty() {
        return;
    }

    let mut delta = vec![0i16; n * n];
    let mut touched = Vec::<usize>::with_capacity(n * 2);
    let grid = Grid {
        rays: &rays,
        neighbors: &neighbors,
        weights: &weights,
    };
    let result = anneal(
        initial,
        AnnealConfig::new(1900, 30.0, 0.2),
        |state| state.score,
        |state, random: &mut Random| {
            let light_index = random.usize(0..state.lights.len());
            let from = state.lights[light_index];
            // Candidate list -> cheap new-coverage estimate -> exact local diff.
            let mut to = from;
            let mut best_cheap = 0usize;
            for _ in 0..12.min(candidates.len()) {
                let candidate = candidates[random.usize(0..candidates.len())];
                if state.placed.contains(candidate) {
                    continue;
                }
                let cheap = masks[candidate].difference(&state.covered).count_ones();
                if to == from || cheap > best_cheap {
                    to = candidate;
                    best_cheap = cheap;
                }
            }
            if to == from {
                return (
                    Move {
                        light_index,
                        to: from,
                        diff: 0,
                    },
                    0,
                );
            }
            let diff = exact_move_diff(state, from, to, &grid, &mut delta, &mut touched);
            (
                Move {
                    light_index,
                    to,
                    diff,
                },
                diff,
            )
        },
        |state, mv| apply_move(state, mv, grid.rays),
    );
    for cell in result.state.lights {
        println!("{} {}", cell / n, cell % n);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        add_light, apply_move, build_neighbors, build_rays, exact_move_diff, id, profit, BitSet,
        Grid, Move, State, ADJ_PENALTY,
    };

    fn assert_full_state(
        state: &State,
        wall: &[bool],
        rays: &[Vec<usize>],
        neighbors: &[Vec<usize>],
        weights: &[i64],
        light_count: usize,
    ) {
        let mut count = vec![0u8; wall.len()];
        let mut covered = BitSet::new();
        let mut placed = BitSet::new();
        for &light in &state.lights {
            assert!(!wall[light]);
            assert!(placed.insert(light), "duplicate light");
            for &cell in &rays[light] {
                count[cell] += 1;
                covered.insert(cell);
            }
        }
        let mut score: i64 = count
            .iter()
            .enumerate()
            .map(|(cell, &amount)| profit(weights[cell], amount))
            .sum();
        for &light in &state.lights {
            for &next in &neighbors[light] {
                if light < next && placed.contains(next) {
                    score -= ADJ_PENALTY;
                }
            }
        }
        assert_eq!(state.lights.len(), light_count);
        assert_eq!(state.placed, placed);
        assert_eq!(state.count, count);
        assert_eq!(state.covered, covered);
        assert_eq!(state.score, score);
    }

    #[test]
    fn exact_move_diff_and_apply_match_full_grid_state() {
        let n = 4;
        let mut wall = vec![false; n * n];
        wall[id(n, 1, 1)] = true;
        let weights: Vec<i64> = (0..n * n).map(|cell| 1 + (cell % 7) as i64).collect();
        let (_masks, rays) = build_rays(n, &wall);
        let neighbors = build_neighbors(n, &wall);
        let grid = Grid {
            rays: &rays,
            neighbors: &neighbors,
            weights: &weights,
        };
        let mut state = State {
            placed: BitSet::new(),
            covered: BitSet::new(),
            lights: Vec::new(),
            count: vec![0; n * n],
            score: 0,
        };
        add_light(&mut state, id(n, 0, 0), &rays, &neighbors, &weights);
        add_light(&mut state, id(n, 3, 3), &rays, &neighbors, &weights);
        assert_full_state(&state, &wall, &rays, &neighbors, &weights, 2);

        let mut delta = vec![0i16; n * n];
        let mut touched = Vec::new();
        let to = id(n, 0, 3);
        let diff = exact_move_diff(&state, state.lights[0], to, &grid, &mut delta, &mut touched);
        apply_move(
            &mut state,
            Move {
                light_index: 0,
                to,
                diff,
            },
            &rays,
        );
        assert_full_state(&state, &wall, &rays, &neighbors, &weights, 2);
    }
}
