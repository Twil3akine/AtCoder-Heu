use std::collections::VecDeque;
use std::io::{self, Read};
use std::time::Instant;

const MAX_CELLS: usize = 256;
const INF: i16 = 1 << 14;
const TIME_LIMIT_MS: u128 = 1990;
const BITE_THRESHOLD: i64 = 5000;
const FUTURE_WINDOW: usize = 8;

type Pos = u8;
type Color = u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    U,
    D,
    L,
    R,
}

impl Dir {
    fn all() -> [Dir; 4] {
        [Dir::U, Dir::D, Dir::L, Dir::R]
    }

    fn to_char(self) -> char {
        match self {
            Dir::U => 'U',
            Dir::D => 'D',
            Dir::L => 'L',
            Dir::R => 'R',
        }
    }

    fn code(self) -> u8 {
        match self {
            Dir::U => 0,
            Dir::D => 1,
            Dir::L => 2,
            Dir::R => 3,
        }
    }

    fn opposite(self) -> Dir {
        match self {
            Dir::U => Dir::D,
            Dir::D => Dir::U,
            Dir::L => Dir::R,
            Dir::R => Dir::L,
        }
    }
}

#[derive(Clone)]
struct Input {
    n: usize,
    m: usize,
    c: usize,
    target: Vec<Color>,
    food: [Color; MAX_CELLS],
}

#[derive(Clone)]
struct State {
    n: usize,
    body: Vec<Pos>,   // head -> tail
    colors: Vec<Color>,
    food: [Color; MAX_CELLS],
}

#[derive(Clone, Debug)]
struct BiteCandidate {
    moves: Vec<Dir>,
    cut_index: usize,
    new_len: usize,
    lcp_now: usize,
    lcp_after: usize,
    bad_removed: i64,
    ordered_match: i64,
    color_match: i64,
    removed_len: i64,
    move_len: i64,
    score: i64,
}

struct Solver {
    input: Input,
    state: State,
    answer: Vec<Dir>,
    dist: [i16; MAX_CELLS],
    first_move: [u8; MAX_CELLS],
    occupied: [bool; MAX_CELLS],
    queue: VecDeque<usize>,
    start_time: Instant,
}

impl Solver {
    fn new(input: Input) -> Self {
        let body = vec![
            pos(4, 0, input.n),
            pos(3, 0, input.n),
            pos(2, 0, input.n),
            pos(1, 0, input.n),
            pos(0, 0, input.n),
        ];
        let colors = vec![1; 5];
        let state = State {
            n: input.n,
            body,
            colors,
            food: input.food,
        };
        Self {
            input,
            state,
            answer: Vec::new(),
            dist: [INF; MAX_CELLS],
            first_move: [255; MAX_CELLS],
            occupied: [false; MAX_CELLS],
            queue: VecDeque::with_capacity(MAX_CELLS),
            start_time: Instant::now(),
        }
    }

    fn solve(&mut self) {
        while self.answer.len() < 100_000 {
            if self.start_time.elapsed().as_millis() >= TIME_LIMIT_MS {
                break;
            }

            if let Some(best_bite) = self.try_best_bite() {
                let threshold = if self.state.body.len() >= 18 {
                    BITE_THRESHOLD / 2
                } else {
                    BITE_THRESHOLD
                };
                if best_bite.score > threshold {
                    for &d in &best_bite.moves {
                        self.apply_real_move(d);
                        self.answer.push(d);
                        if self.answer.len() >= 100_000 {
                            break;
                        }
                    }
                    continue;
                }
            }

            if let Some(d) = self.choose_normal_move() {
                self.apply_real_move(d);
                self.answer.push(d);
            } else {
                break;
            }
        }
    }

    fn choose_normal_move(&mut self) -> Option<Dir> {
        self.build_occupied();
        self.run_bfs();

        let k = self.state.colors.len();
        if k >= self.input.m {
            return self.safe_fallback_move();
        }

        let wanted = self.input.target[k];
        let mut best_same: Option<(i64, Pos)> = None;
        let mut best_any: Option<(i64, Pos)> = None;

        for p in 0..self.input.n * self.input.n {
            let color = self.state.food[p];
            if color == 0 {
                continue;
            }
            if self.dist[p] >= INF {
                continue;
            }
            let score = self.eval_food(p as Pos);
            if color == wanted {
                if best_same.map_or(true, |x| score > x.0) {
                    best_same = Some((score, p as Pos));
                }
            }
            if best_any.map_or(true, |x| score > x.0) {
                best_any = Some((score, p as Pos));
            }
        }

        let target_pos = best_same.or(best_any)?.1 as usize;
        let code = self.first_move[target_pos];
        decode_dir(code)
    }

    fn eval_food(&self, p: Pos) -> i64 {
        let idx = p as usize;
        let dist = self.dist[idx] as i64;
        let color = self.state.food[idx];
        let k = self.state.colors.len();
        let mut score = 20_000 - 120 * dist;
        if k < self.input.m && color == self.input.target[k] {
            score += 10_000;
        }
        if k + 1 < self.input.m && color == self.input.target[k + 1] {
            score += 3_000;
        }
        score += 1_000 * future_count(&self.input.target, k, color);
        score
    }

    fn try_best_bite(&self) -> Option<BiteCandidate> {
        let mut best: Option<BiteCandidate> = None;
        let head = self.state.body[0];
        let prev_dir = if self.state.body.len() >= 2 {
            dir_between(self.state.body[1], self.state.body[0], self.state.n)
        } else {
            None
        };

        let mut stack = Vec::new();
        self.dfs_bite(head, prev_dir, 0, &mut stack, &mut best);
        best
    }

    fn dfs_bite(
        &self,
        current: Pos,
        prev_dir: Option<Dir>,
        depth: usize,
        path: &mut Vec<Dir>,
        best: &mut Option<BiteCandidate>,
    ) {
        if depth >= 4 {
            return;
        }
        for d in Dir::all() {
            if let Some(pd) = prev_dir {
                if d == pd.opposite() {
                    continue;
                }
            }
            let Some(np) = step_pos(current, d, self.state.n) else {
                continue;
            };
            path.push(d);
            if let Some(cand) = self.evaluate_bite_path(path) {
                if best.as_ref().map_or(true, |b| cand.score > b.score) {
                    *best = Some(cand);
                }
            }
            self.dfs_bite(np, Some(d), depth + 1, path, best);
            path.pop();
        }
    }

    fn evaluate_bite_path(&self, moves: &[Dir]) -> Option<BiteCandidate> {
        let sim = self.simulate_moves(moves)?;
        let cut_index = sim.cut_index?;

        let lcp_now = lcp(&self.state.colors, &self.input.target);
        let lcp_after = lcp(&sim.state.colors, &self.input.target);
        let new_len = sim.state.colors.len();
        if lcp_after < lcp_now || new_len < 5 || moves.len() > 4 {
            return None;
        }

        let old_len = self.state.colors.len();
        let removed_suffix = &sim.removed_colors;
        let removed_len = (old_len - (cut_index + 1)) as i64;
        let bad_removed = bad_removed(&self.state.colors, &self.input.target, cut_index + 1);
        let ordered_match = ordered_match(removed_suffix, &self.input.target, new_len, FUTURE_WINDOW);
        let color_match = color_match(
            removed_suffix,
            &self.input.target,
            new_len,
            FUTURE_WINDOW,
            self.input.c,
        );
        if bad_removed + ordered_match + color_match == 0 {
            return None;
        }

        let move_len = moves.len() as i64;
        let too_short = (6usize.saturating_sub(new_len)) as i64;
        let score = 1_000_000 * (lcp_after as i64 - lcp_now as i64)
            + 12_000 * bad_removed
            + 4_000 * ordered_match
            + 1_500 * color_match
            + 100 * removed_len
            - 100 * move_len
            - 3_000 * too_short;

        Some(BiteCandidate {
            moves: moves.to_vec(),
            cut_index,
            new_len,
            lcp_now,
            lcp_after,
            bad_removed,
            ordered_match,
            color_match,
            removed_len,
            move_len,
            score,
        })
    }

    fn simulate_moves(&self, moves: &[Dir]) -> Option<SimResult> {
        let mut st = self.state.clone();
        let mut cut_index = None;
        let mut removed_colors = Vec::new();
        for &d in moves {
            let result = st.apply_move(d)?;
            if result.cut_index.is_some() {
                cut_index = result.cut_index;
                removed_colors = result.removed_colors;
            }
        }
        Some(SimResult {
            state: st,
            cut_index,
            removed_colors,
        })
    }

    fn safe_fallback_move(&self) -> Option<Dir> {
        let head = self.state.body[0];
        let neck = if self.state.body.len() >= 2 {
            Some(self.state.body[1])
        } else {
            None
        };
        let mut occ = [false; MAX_CELLS];
        for &p in &self.state.body {
            occ[p as usize] = true;
        }
        for d in Dir::all() {
            let Some(np) = step_pos(head, d, self.state.n) else {
                continue;
            };
            if Some(np) == neck {
                continue;
            }
            if occ[np as usize] {
                continue;
            }
            return Some(d);
        }
        None
    }

    fn apply_real_move(&mut self, d: Dir) {
        let _ = self.state.apply_move(d);
    }

    fn build_occupied(&mut self) {
        self.occupied.fill(false);
        for &p in &self.state.body {
            self.occupied[p as usize] = true;
        }
    }

    fn run_bfs(&mut self) {
        self.dist.fill(INF);
        self.first_move.fill(255);
        self.queue.clear();

        let head = self.state.body[0] as usize;
        let neck = if self.state.body.len() >= 2 {
            Some(self.state.body[1])
        } else {
            None
        };
        self.dist[head] = 0;
        self.queue.push_back(head);

        while let Some(v) = self.queue.pop_front() {
            let base_dist = self.dist[v];
            let vp = v as Pos;
            for d in Dir::all() {
                let Some(np) = step_pos(vp, d, self.state.n) else {
                    continue;
                };
                let ni = np as usize;
                if self.occupied[ni] {
                    continue;
                }
                if Some(np) == neck {
                    continue;
                }
                if self.dist[ni] != INF {
                    continue;
                }
                self.dist[ni] = base_dist + 1;
                self.first_move[ni] = if v == head {
                    d.code()
                } else {
                    self.first_move[v]
                };
                self.queue.push_back(ni);
            }
        }
    }
}

#[derive(Clone)]
struct SimResult {
    state: State,
    cut_index: Option<usize>,
    removed_colors: Vec<Color>,
}

#[derive(Clone)]
struct ApplyMoveResult {
    cut_index: Option<usize>,
    removed_colors: Vec<Color>,
}

impl State {
    fn apply_move(&mut self, d: Dir) -> Option<ApplyMoveResult> {
        let head = self.body[0];
        let neck = if self.body.len() >= 2 { Some(self.body[1]) } else { None };
        let new_head = step_pos(head, d, self.n)?;
        if Some(new_head) == neck {
            return None;
        }

        let old_body = self.body.clone();
        let old_colors = self.colors.clone();
        let old_len = old_body.len();

        let mut new_body = Vec::with_capacity(old_len + 1);
        new_body.push(new_head);
        for &p in old_body.iter().take(old_len - 1) {
            new_body.push(p);
        }
        let mut new_colors = old_colors.clone();

        let food_color = self.food[new_head as usize];
        if food_color != 0 {
            new_body.push(*old_body.last().unwrap());
            new_colors.push(food_color);
            self.food[new_head as usize] = 0;
        }

        let mut cut_index = None;
        let mut removed_colors = Vec::new();
        if food_color == 0 {
            for i in 1..new_body.len().saturating_sub(1) {
                if new_body[0] == new_body[i] {
                    cut_index = Some(i);
                    removed_colors = new_colors[(i + 1)..].to_vec();
                    let removed_positions = new_body[(i + 1)..].to_vec();
                    for (p, c) in removed_positions.into_iter().zip(removed_colors.iter().copied()) {
                        self.food[p as usize] = c;
                    }
                    new_body.truncate(i + 1);
                    new_colors.truncate(i + 1);
                    break;
                }
            }
        }

        self.body = new_body;
        self.colors = new_colors;
        Some(ApplyMoveResult {
            cut_index,
            removed_colors,
        })
    }
}

fn lcp(cur: &[Color], target: &[Color]) -> usize {
    let mut i = 0;
    while i < cur.len() && i < target.len() && cur[i] == target[i] {
        i += 1;
    }
    i
}

fn bad_removed(cur: &[Color], target: &[Color], cut_from: usize) -> i64 {
    let mut res = 0i64;
    for i in cut_from..cur.len() {
        if i < target.len() && cur[i] != target[i] {
            res += 1;
        }
    }
    res
}

fn ordered_match(suffix: &[Color], target: &[Color], start: usize, w: usize) -> i64 {
    let len = suffix.len().min(w).min(target.len().saturating_sub(start));
    let mut res = 0i64;
    for i in 0..len {
        if suffix[i] == target[start + i] {
            res += 1;
        }
    }
    res
}

fn color_match(suffix: &[Color], target: &[Color], start: usize, w: usize, c: usize) -> i64 {
    let mut a = vec![0i64; c + 1];
    let mut b = vec![0i64; c + 1];
    let len1 = suffix.len().min(w);
    for &x in &suffix[..len1] {
        a[x as usize] += 1;
    }
    let len2 = target.len().saturating_sub(start).min(w);
    for i in 0..len2 {
        b[target[start + i] as usize] += 1;
    }
    let mut res = 0i64;
    for color in 1..=c {
        res += a[color].min(b[color]);
    }
    res
}

fn future_count(target: &[Color], start: usize, color: Color) -> i64 {
    let end = (start + FUTURE_WINDOW).min(target.len());
    let mut cnt = 0i64;
    for &x in &target[start..end] {
        if x == color {
            cnt += 1;
        }
    }
    cnt
}

fn pos(r: usize, c: usize, n: usize) -> Pos {
    (r * n + c) as Pos
}

fn rc(p: Pos, n: usize) -> (usize, usize) {
    let x = p as usize;
    (x / n, x % n)
}

fn step_pos(p: Pos, d: Dir, n: usize) -> Option<Pos> {
    let (r, c) = rc(p, n);
    match d {
        Dir::U => (r > 0).then(|| pos(r - 1, c, n)),
        Dir::D => (r + 1 < n).then(|| pos(r + 1, c, n)),
        Dir::L => (c > 0).then(|| pos(r, c - 1, n)),
        Dir::R => (c + 1 < n).then(|| pos(r, c + 1, n)),
    }
}

fn dir_between(from: Pos, to: Pos, n: usize) -> Option<Dir> {
    let (fr, fc) = rc(from, n);
    let (tr, tc) = rc(to, n);
    if fr + 1 == tr && fc == tc {
        Some(Dir::D)
    } else if tr + 1 == fr && fc == tc {
        Some(Dir::U)
    } else if fr == tr && fc + 1 == tc {
        Some(Dir::R)
    } else if fr == tr && tc + 1 == fc {
        Some(Dir::L)
    } else {
        None
    }
}

fn decode_dir(code: u8) -> Option<Dir> {
    match code {
        0 => Some(Dir::U),
        1 => Some(Dir::D),
        2 => Some(Dir::L),
        3 => Some(Dir::R),
        _ => None,
    }
}

fn read_input() -> Input {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s).unwrap();
    let mut it = s.split_whitespace();

    let n: usize = it.next().unwrap().parse().unwrap();
    let m: usize = it.next().unwrap().parse().unwrap();
    let c: usize = it.next().unwrap().parse().unwrap();
    let mut target = vec![0u8; m];
    for x in &mut target {
        *x = it.next().unwrap().parse().unwrap();
    }
    let mut food = [0u8; MAX_CELLS];
    for r in 0..n {
        for col in 0..n {
            food[r * n + col] = it.next().unwrap().parse().unwrap();
        }
    }

    Input {
        n,
        m,
        c,
        target,
        food,
    }
}

fn main() {
    let input = read_input();
    let mut solver = Solver::new(input);
    solver.solve();
    let out = solver
        .answer
        .iter()
        .map(|&d| d.to_char().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    println!("{}", out);
}
