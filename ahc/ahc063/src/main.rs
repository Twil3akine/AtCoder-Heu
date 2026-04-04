#![allow(nonstandard_style)]
#![allow(unused_assignments)]

use std::io::{BufRead, stdin};
use std::time::{Duration, Instant};

pub struct Scanner<R: std::io::BufRead> {
    pub reader: R,
    pub buf_str: Vec<u8>,
    pub buf_iter: std::str::SplitWhitespace<'static>,
}

impl<R: std::io::BufRead> Scanner<R> {
    pub fn with_reader(reader: R) -> Self {
        Self {
            reader,
            buf_str: vec![],
            buf_iter: "".split_whitespace(),
        }
    }
    pub fn token<T: std::str::FromStr>(&mut self) -> T {
        loop {
            if let Some(token) = self.buf_iter.next() {
                return token.parse().ok().expect("Failed to parse token");
            }
            self.buf_str.clear();
            self.reader.read_until(b'\n', &mut self.buf_str).unwrap();
            self.buf_iter = unsafe {
                let slice = std::str::from_utf8_unchecked(&self.buf_str);
                std::mem::transmute(slice.split_whitespace())
            }
        }
    }
}
impl Scanner<std::io::StdinLock<'static>> {
    pub fn new() -> Self {
        Self::with_reader(stdin().lock())
    }
}

#[macro_export]
macro_rules! read_value {
    ($sc:expr, [$t:tt; $len:expr]) => {
        (0..$len).map(|_| read_value!($sc, $t)).collect::<Vec<_>>()
    };
    ($sc:expr, $t:ty) => {
        $sc.token::<$t>()
    };
}

#[macro_export]
macro_rules! input {
    ($sc:expr, $($var:ident),+ : $t:tt $(, $($r:tt)*)?) => {
        $( let $var = read_value!($sc, $t); )+
        $(input!($sc, $($r)*);)?
    };
}

const TIME_LIMIT_MS: u64 = 1950;
const BEAM_TIME_LIMIT_MS: u64 = 1500; // ビームサーチはここで打ち切り、残り200msをpost_processへ
const SA_TIME_LIMIT_MS: u64 = 800;
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
const DIR_CHARS: [char; 4] = ['U', 'D', 'L', 'R'];

struct XorShift(u32);
impl XorShift {
    fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn next_usize(&mut self, m: usize) -> usize {
        (self.next() as usize) % m
    }
}

#[derive(Clone, Debug)]
pub struct Input {
    pub N: usize,
    pub M: usize,
    pub C: usize,
    pub d: Vec<usize>,
    pub f: Vec<Vec<usize>>,
}

pub fn parse_input<R: BufRead>(sc: &mut Scanner<R>) -> Input {
    input! { sc, N: usize, M: usize, C: usize, d: [usize; M], f: [[usize; N]; N] }
    Input { N, M, C, d, f }
}

#[inline(always)]
fn set_bit(bits: &mut [u64; 4], pos: u8) {
    bits[(pos >> 6) as usize] |= 1 << (pos & 63);
}
#[inline(always)]
fn clear_bit(bits: &mut [u64; 4], pos: u8) {
    bits[(pos >> 6) as usize] &= !(1 << (pos & 63));
}
#[inline(always)]
fn get_bit(bits: &[u64; 4], pos: u8) -> bool {
    (bits[(pos >> 6) as usize] >> (pos & 63)) & 1 != 0
}

fn optimize_targets(input: &Input, time_limit: Duration) -> Vec<u8> {
    let start = Instant::now();
    let mut rng = XorShift::new(42);
    let mut foods = vec![vec![]; input.C + 1];
    for i in 0..input.N {
        for j in 0..input.N {
            if input.f[i][j] > 0 {
                foods[input.f[i][j]].push((i as isize, j as isize));
            }
        }
    }

    let mut targets = vec![(0isize, 0isize); input.M];
    targets[0] = (0, 0);
    targets[1] = (1, 0);
    targets[2] = (2, 0);
    targets[3] = (3, 0);
    targets[4] = (4, 0);

    let mut usage = vec![0; input.C + 1];
    for k in 5..input.M {
        let c = input.d[k];
        targets[k] = foods[c][usage[c]];
        usage[c] += 1;
    }

    let calc_score = |t: &[(isize, isize)]| -> i32 {
        let mut score = 0;
        for k in 5..input.M {
            let dy = t[k].0 as i32 - t[k - 1].0 as i32;
            let dx = t[k].1 as i32 - t[k - 1].1 as i32;
            score += dy.abs() + dx.abs();
            if k >= 6 {
                let p_dy = t[k - 1].0 as i32 - t[k - 2].0 as i32;
                let p_dx = t[k - 1].1 as i32 - t[k - 2].1 as i32;
                let dot = dy * p_dy + dx * p_dx;
                if dot < 0 {
                    score += 1;
                }
            }
        }
        score
    };

    let mut cur_score = calc_score(&targets);
    let mut best_targets = targets.clone();
    let mut best_score = cur_score;

    let mut pool_by_color = vec![vec![]; input.C + 1];
    for k in 5..input.M {
        pool_by_color[input.d[k]].push(k);
    }
    let valid: Vec<usize> = (1..=input.C)
        .filter(|&c| pool_by_color[c].len() >= 2)
        .collect();

    if !valid.is_empty() {
        let (t0, t1) = (10.0, 0.1);
        let mut iter = 0;
        loop {
            if iter & 127 == 0 && start.elapsed() >= time_limit {
                break;
            }
            iter += 1;
            let c = valid[rng.next_usize(valid.len())];
            let pool = &pool_by_color[c];
            let u = pool[rng.next_usize(pool.len())];
            let mut v = pool[rng.next_usize(pool.len())];
            while u == v {
                v = pool[rng.next_usize(pool.len())];
            }

            targets.swap(u, v);
            let next_score = calc_score(&targets);
            let accept = next_score <= cur_score || {
                let temp =
                    t0 + (t1 - t0) * (start.elapsed().as_secs_f64() / time_limit.as_secs_f64());
                (rng.next() as f64 / std::u32::MAX as f64)
                    < f64::exp((cur_score - next_score) as f64 / temp)
            };
            if accept {
                cur_score = next_score;
                if cur_score < best_score {
                    best_score = cur_score;
                    best_targets.copy_from_slice(&targets);
                }
            } else {
                targets.swap(u, v);
            }
        }
    }
    best_targets
        .iter()
        .map(|&(i, j)| (i * 16 + j) as u8)
        .collect()
}

#[derive(Clone)]
struct State {
    f: [u8; 256],
    ij: [u8; 256],
    c: [u8; 256],
    body_bits: [u64; 4],
    head_ptr: u8,
    len: usize,
    turn: usize,
    score: i64,
    history: Vec<u8>,
}

impl State {
    fn new(input: &Input, target_seq: &[u8], target_path_dist: &[i64]) -> Self {
        let mut f = [0u8; 256];
        for i in 0..input.N {
            for j in 0..input.N {
                f[i * 16 + j] = input.f[i][j] as u8;
            }
        }
        let mut ij = [0u8; 256];
        let mut body_bits = [0u64; 4];
        for i in 0..5 {
            let pos = ((4 - i) * 16) as u8;
            ij[i] = pos;
            set_bit(&mut body_bits, pos);
        }
        let c = [1u8; 256];
        let mut state = Self {
            f,
            ij,
            c,
            body_bits,
            head_ptr: 0,
            len: 5,
            turn: 0,
            score: 0,
            history: Vec::with_capacity(1024),
        };
        state.score = state.evaluate(input, target_seq, target_path_dist);
        state
    }

    #[inline(always)]
    fn get_pos(&self, idx: usize) -> u8 {
        self.ij[self.head_ptr.wrapping_add(idx as u8) as usize]
    }

    fn apply(
        &mut self,
        dir: usize,
        input: &Input,
        target_seq: &[u8],
        target_path_dist: &[i64],
    ) -> bool {
        let head_pos = self.get_pos(0);
        let hi = (head_pos / 16) as isize;
        let hj = (head_pos % 16) as isize;
        let (di, dj) = DIJ[dir];
        let ni = hi + di;
        let nj = hj + dj;

        if ni < 0 || ni >= input.N as isize || nj < 0 || nj >= input.N as isize {
            return false;
        }

        let new_pos = (ni * 16 + nj) as u8;
        if self.len > 1 && new_pos == self.get_pos(1) {
            return false;
        }

        let eaten_color = self.f[new_pos as usize];
        if eaten_color != 0 {
            self.f[new_pos as usize] = 0;
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            self.c[self.len] = eaten_color;
            self.len += 1;
            set_bit(&mut self.body_bits, new_pos);
        } else {
            let tail_pos = self.get_pos(self.len - 1);
            clear_bit(&mut self.body_bits, tail_pos);
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            set_bit(&mut self.body_bits, new_pos);
        }

        if self.len >= 3 && get_bit(&self.body_bits, new_pos) {
            let mut bite_idx = 0;
            for h in 1..self.len - 1 {
                if self.get_pos(h) == new_pos {
                    bite_idx = h;
                    break;
                }
            }
            if bite_idx > 0 {
                for p in bite_idx + 1..self.len {
                    let pos = self.get_pos(p);
                    self.f[pos as usize] = self.c[p];
                    clear_bit(&mut self.body_bits, pos);
                }
                self.len = bite_idx + 1;
            }
        }

        self.turn += 1;
        self.history.push(dir as u8);
        self.score = self.evaluate(input, target_seq, target_path_dist);
        true
    }

    fn cost_to_target(&self, input: &Input, target: u8) -> i32 {
        if target == 255 {
            return 25000;
        }
        let mut dist = [25000i32; 256];
        let mut q = [0u8; 256];
        let mut head = 0;
        let mut tail = 0;

        let start = self.get_pos(0);
        dist[start as usize] = 0;
        q[tail] = start;
        tail += 1;

        while head < tail {
            let u = q[head];
            head += 1;
            if u == target {
                return dist[u as usize];
            }
            let d = dist[u as usize];
            let ui = (u / 16) as isize;
            let uj = (u % 16) as isize;
            for &(di, dj) in &DIJ {
                let ni = ui + di;
                let nj = uj + dj;
                if ni >= 0 && ni < input.N as isize && nj >= 0 && nj < input.N as isize {
                    let v = (ni * 16 + nj) as u8;
                    if !get_bit(&self.body_bits, v) {
                        if self.f[v as usize] != 0 && v != target {
                            continue;
                        }
                        if dist[v as usize] > d + 10 {
                            dist[v as usize] = d + 10;
                            q[tail] = v;
                            tail += 1;
                        }
                    }
                }
            }
        }
        25000
    }

    fn evaluate(&self, input: &Input, target_seq: &[u8], target_path_dist: &[i64]) -> i64 {
        let mut e = 0;
        for p in 0..self.len {
            if input.d[p] != self.c[p] as usize {
                e += 1;
            }
        }
        if self.len == input.M {
            return self.turn as i64 + 10000 * e as i64;
        }
        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));
        let expected_pos = target_seq[self.len];
        let target_color = input.d[self.len] as u8;
        let actual_target = if self.f[expected_pos as usize] == target_color {
            expected_pos
        } else {
            let mut best_pos = 255;
            let mut min_d = 1000;
            let hi = (self.get_pos(0) / 16) as isize;
            let hj = (self.get_pos(0) % 16) as isize;
            for i in 0..input.N {
                for j in 0..input.N {
                    let idx = i * 16 + j;
                    if self.f[idx] == target_color {
                        let d = (hi - i as isize).abs() + (hj - j as isize).abs();
                        if d < min_d {
                            min_d = d;
                            best_pos = idx as u8;
                        }
                    }
                }
            }
            best_pos
        };
        let cost = self.cost_to_target(input, actual_target) as i64;
        let heuristic_error_penalty = 15000 * e as i64;
        let future_cost = target_path_dist[self.len] * 10;
        base + heuristic_error_penalty + cost + future_cost
    }

    fn bfs_path_to(
        &self,
        input: &Input,
        target: u8,
        extra_walls: &[u64; 4],
        ignore_foods: bool,
    ) -> Option<Vec<usize>> {
        if self.get_pos(0) == target {
            return Some(vec![]);
        }
        let n = input.N as isize;
        let mut dist = [u8::MAX; 256];
        let mut prev_pos = [255u8; 256];
        let mut prev_dir = [0u8; 256];
        let mut q = [0u8; 512];
        let mut head = 0usize;
        let mut tail = 0usize;

        let start = self.get_pos(0);
        dist[start as usize] = 0;
        q[tail] = start;
        tail += 1;

        while head < tail {
            let u = q[head % 512];
            head += 1;
            if u == target {
                let mut path = vec![];
                let mut cur = u;
                while cur != start {
                    path.push(prev_dir[cur as usize] as usize);
                    cur = prev_pos[cur as usize];
                }
                path.reverse();
                return Some(path);
            }
            let ui = (u / 16) as isize;
            let uj = (u % 16) as isize;
            let d = dist[u as usize];
            for dir in 0..4 {
                let (di, dj) = DIJ[dir];
                let ni = ui + di;
                let nj = uj + dj;
                if ni < 0 || ni >= n || nj < 0 || nj >= n {
                    continue;
                }
                let v = (ni * 16 + nj) as u8;
                if get_bit(extra_walls, v) {
                    continue;
                }
                if ignore_foods && self.f[v as usize] != 0 && v != target {
                    continue;
                }
                if dist[v as usize] == u8::MAX {
                    dist[v as usize] = d + 1;
                    prev_pos[v as usize] = u;
                    prev_dir[v as usize] = dir as u8;
                    q[tail % 512] = v;
                    tail += 1;
                }
            }
        }
        None
    }
}

fn try_fix_at(
    state: &State,
    p: usize,
    input: &Input,
    target_seq: &[u8],
    target_path_dist: &[i64],
    deadline: Instant,
) -> Option<State> {
    if p < 2 {
        return None;
    }

    let bite_target = state.get_pos(p - 2);
    let mut extra_walls = [0u64; 4];
    for h in 0..p - 1 {
        set_bit(&mut extra_walls, state.get_pos(h));
    }

    let path = state.bfs_path_to(input, bite_target, &extra_walls, true)?;
    if path.len() > 100 {
        return None;
    }

    let mut candidate = state.clone();
    for &dir in &path {
        candidate.apply(dir, input, target_seq, target_path_dist);
    }
    if candidate.len >= state.len {
        return None;
    }

    while candidate.len < input.M {
        if Instant::now() >= deadline {
            break;
        }
        // target_seqが指定した位置の餌がまだ残っていればそこへ、
        // なければ同色の最近傍へ（ただし色は必ず正しいものを）
        let want_color = input.d[candidate.len] as u8;
        let sa_pos = target_seq[candidate.len];
        let best_food = if candidate.f[sa_pos as usize] == want_color {
            sa_pos
        } else {
            let hi = (candidate.get_pos(0) / 16) as isize;
            let hj = (candidate.get_pos(0) % 16) as isize;
            let mut best = 255u8;
            let mut best_dist = isize::MAX;
            for i in 0..input.N {
                for j in 0..input.N {
                    let idx = i * 16 + j;
                    if candidate.f[idx] == want_color {
                        let d = (hi - i as isize).abs() + (hj - j as isize).abs();
                        if d < best_dist {
                            best_dist = d;
                            best = idx as u8;
                        }
                    }
                }
            }
            best
        };
        if best_food == 255 {
            break;
        }

        let body_walls = candidate.body_bits.clone();
        let Some(food_path) = candidate.bfs_path_to(input, best_food, &body_walls, true) else {
            break;
        };
        if food_path.len() > 200 {
            break;
        }

        for &dir in &food_path {
            candidate.apply(dir, input, target_seq, target_path_dist);
        }
    }

    Some(candidate)
}

fn post_process(
    best_state: &mut State,
    input: &Input,
    target_seq: &[u8],
    target_path_dist: &[i64],
    deadline: Instant,
) {
    let mut improved = true;
    while improved && Instant::now() < deadline {
        improved = false;

        let mismatches: Vec<usize> = (0..best_state.len)
            .filter(|&p| input.d[p] != best_state.c[p] as usize)
            .collect();

        if mismatches.is_empty() {
            break;
        }

        let mut best_candidate: Option<State> = None;

        for &p in &mismatches {
            if Instant::now() >= deadline {
                break;
            }
            if let Some(candidate) =
                try_fix_at(best_state, p, input, target_seq, target_path_dist, deadline)
            {
                if candidate.score < best_state.score {
                    let is_better = match &best_candidate {
                        None => true,
                        Some(prev) => candidate.score < prev.score,
                    };
                    if is_better {
                        best_candidate = Some(candidate);
                    }
                }
            }
        }

        if let Some(c) = best_candidate {
            *best_state = c;
            improved = true;
        }
    }
}

fn main() {
    let start_time = Instant::now();
    let sa_time_limit = Duration::from_millis(SA_TIME_LIMIT_MS);

    let mut sc = Scanner::new();
    let input = parse_input(&mut sc);

    let target_seq = optimize_targets(&input, sa_time_limit);

    let mut target_path_dist = vec![0i64; input.M + 1];
    for len in (0..input.M - 1).rev() {
        let p1 = target_seq[len];
        let p2 = target_seq[len + 1];
        let d = ((p1 / 16) as i64 - (p2 / 16) as i64).abs()
            + ((p1 % 16) as i64 - (p2 % 16) as i64).abs();
        target_path_dist[len] = target_path_dist[len + 1] + d;
    }

    let initial_state = State::new(&input, &target_seq, &target_path_dist);
    let mut beam = vec![initial_state];
    let mut best_state = beam[0].clone();
    let mut current_beam_width = 300;
    let mut seen_generation = vec![0u32; 65536];
    let mut current_generation = 0;

    while !beam.is_empty() {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        if elapsed_ms >= BEAM_TIME_LIMIT_MS {
            break;
        } // ビームはここで打ち切り

        current_generation += 1;
        let remaining_time = BEAM_TIME_LIMIT_MS.saturating_sub(elapsed_ms);

        if remaining_time < 200 {
            current_beam_width = 100;
        } else if remaining_time < 500 {
            current_beam_width = 300;
        } else {
            current_beam_width = 800;
        }

        let mut next_beam = Vec::with_capacity(beam.len() * 4);

        for state in &beam {
            if state.len == input.M && state.score == state.turn as i64 {
                if state.score < best_state.score {
                    best_state = state.clone();
                }
                continue;
            }
            for dir in 0..4 {
                let mut next_state = state.clone();
                if next_state.apply(dir, &input, &target_seq, &target_path_dist) {
                    next_beam.push(next_state);
                }
            }
        }

        if next_beam.is_empty() {
            break;
        }

        let mut indices: Vec<usize> = (0..next_beam.len()).collect();
        indices.sort_unstable_by_key(|&i| next_beam[i].score);

        let mut unique_states = Vec::with_capacity(current_beam_width);
        for &i in &indices {
            let state = &next_beam[i];
            let key = (state.get_pos(0) as usize) << 8 | state.len;
            if seen_generation[key] != current_generation {
                seen_generation[key] = current_generation;
                unique_states.push(state.clone());
                if unique_states.len() == current_beam_width {
                    break;
                }
            }
        }

        if !unique_states.is_empty() && unique_states[0].score < best_state.score {
            best_state = unique_states[0].clone();
        }
        beam = unique_states;
    }

    // 残り~200msをpost_processへ
    let deadline = start_time + Duration::from_millis(TIME_LIMIT_MS);
    post_process(
        &mut best_state,
        &input,
        &target_seq,
        &target_path_dist,
        deadline,
    );

    for &dir in &best_state.history {
        println!("{}", DIR_CHARS[dir as usize]);
    }
}
