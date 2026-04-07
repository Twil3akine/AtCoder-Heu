#![allow(nonstandard_style)]
#![allow(unused_assignments)]

use std::io::{BufRead, stdin};
use std::time::{Duration, Instant};

// =============================================
// Scanner & Macros (unchanged)
// =============================================
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

// =============================================
// Main Logic
// =============================================

const TIME_LIMIT_MS: u64 = 1990;
const SA_TIME_LIMIT_MS: u64 = 300;
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

// ---------------------------------------------------------
// Bitboard Utils (unchanged)
// ---------------------------------------------------------
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

// ---------------------------------------------------------
// SA Layer (unchanged)
// ---------------------------------------------------------
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
        let (t0, t1) = (15.0, 0.1);
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

// ---------------------------------------------------------
// Move Tree: Vec<u8> historyを各Stateから除去
// ---------------------------------------------------------
struct MoveTree {
    parent: Vec<u32>,
    dir: Vec<u8>,
}

impl MoveTree {
    fn new() -> Self {
        Self {
            parent: Vec::with_capacity(1 << 20),
            dir: Vec::with_capacity(1 << 20),
        }
    }

    #[inline]
    fn add_root(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(u32::MAX);
        self.dir.push(0);
        id
    }

    #[inline]
    fn add_child(&mut self, parent_id: u32, d: u8) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(parent_id);
        self.dir.push(d);
        id
    }

    fn reconstruct(&self, mut node_id: u32) -> Vec<u8> {
        let mut dirs = Vec::new();
        while node_id != u32::MAX {
            let p = self.parent[node_id as usize];
            if p != u32::MAX {
                dirs.push(self.dir[node_id as usize]);
            }
            node_id = p;
        }
        dirs.reverse();
        dirs
    }
}

// ---------------------------------------------------------
// Beam Search State (history removed, tree_node_id added)
// ---------------------------------------------------------

// ---------------------------------------------------------
// BFS Context (世代管理用)
// ---------------------------------------------------------
struct BfsContext {
    dist: [i32; 256],
    r#gen: [u32; 256],
    current_gen: u32,
    q: [u8; 256],
}

impl BfsContext {
    fn new() -> Self {
        Self {
            dist: [0; 256],
            r#gen: [0; 256],
            current_gen: 0,
            q: [0; 256],
        }
    }
}

// ---------------------------------------------------------
// Beam Search State
// ---------------------------------------------------------
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
    error_count: usize,
    tree_node_id: u32,
}

impl State {
    fn new(
        input: &Input,
        target_seq: &[u8],
        target_path_dist: &[i64],
        tree: &mut MoveTree,
        bfs_ctx: &mut BfsContext,
    ) -> Self {
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
        let root_id = tree.add_root();

        let mut state = Self {
            f,
            ij,
            c,
            body_bits,
            head_ptr: 0,
            len: 5,
            turn: 0,
            score: 0,
            error_count: 0,
            tree_node_id: root_id,
        };
        state.score = state.evaluate(input, target_seq, target_path_dist, bfs_ctx, false);
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
        new_tree_id: u32,
        bfs_ctx: &mut BfsContext,
        panic_mode: bool,
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
            if input.d[self.len] != eaten_color as usize {
                self.error_count += 1;
            }
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
                    if input.d[p] != self.c[p] as usize {
                        self.error_count -= 1;
                    }
                }
                self.len = bite_idx + 1;
            }
        }

        self.turn += 1;
        self.tree_node_id = new_tree_id;
        self.score = self.evaluate(input, target_seq, target_path_dist, bfs_ctx, panic_mode);
        true
    }

    fn cost_to_target(&self, input: &Input, target: u8, ctx: &mut BfsContext) -> i32 {
        if target == 255 {
            return 25000;
        }

        ctx.current_gen += 1;
        let mut head = 0;
        let mut tail = 0;

        let start = self.get_pos(0);
        ctx.dist[start as usize] = 0;
        ctx.r#gen[start as usize] = ctx.current_gen;
        ctx.q[tail] = start;
        tail += 1;

        while head < tail {
            let u = ctx.q[head];
            head += 1;

            if u == target {
                return ctx.dist[u as usize];
            }

            let d = ctx.dist[u as usize];
            let ui = (u / 16) as isize;
            let uj = (u % 16) as isize;

            for &(di, dj) in &DIJ {
                let ni = ui + di;
                let nj = uj + dj;
                if ni >= 0 && ni < input.N as isize && nj >= 0 && nj < input.N as isize {
                    let v = (ni * 16 + nj) as u8;
                    if !get_bit(&self.body_bits, v) {
                        let is_wrong_food = self.f[v as usize] != 0 && v != target;
                        if is_wrong_food {
                            continue;
                        }

                        // 世代管理によるdist配列の更新チェック
                        if ctx.r#gen[v as usize] != ctx.current_gen || ctx.dist[v as usize] > d + 10
                        {
                            ctx.r#gen[v as usize] = ctx.current_gen;
                            ctx.dist[v as usize] = d + 10;
                            ctx.q[tail] = v;
                            tail += 1;
                        }
                    }
                }
            }
        }
        25000
    }

    fn evaluate(
        &self,
        input: &Input,
        target_seq: &[u8],
        target_path_dist: &[i64],
        bfs_ctx: &mut BfsContext,
        panic_mode: bool,
    ) -> i64 {
        let e = self.error_count + 1;
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

        let cost = self.cost_to_target(input, actual_target, bfs_ctx) as i64;

        let mut penalty_weight = 32500;

        if panic_mode || cost >= 20000 {
            // 時間切れ間近、または目的の餌にたどり着けない（詰み）場合、
            // 長さ不足ペナルティ(20000点)を避けるため、エラーを許容して手近な餌を食べる
            penalty_weight = 5000;
        }

        let heuristic_error_penalty = (penalty_weight * e * e * 2 / 3) as i64;

        let future_cost = target_path_dist[self.len] * 10;

        base + heuristic_error_penalty + cost + future_cost
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

    let mut tree = MoveTree::new();
    let mut bfs_ctx = BfsContext::new(); // BFS用コンテキストの初期化

    let initial_state = State::new(
        &input,
        &target_seq,
        &target_path_dist,
        &mut tree,
        &mut bfs_ctx,
    );
    let mut best_score: i64 = initial_state.score;
    let mut best_tree_id: u32 = initial_state.tree_node_id;
    let mut best_state = initial_state.clone();

    let mut beam = vec![initial_state];
    let mut next_beam: Vec<State> = Vec::with_capacity(16384);

    // 改善点1: indicesのループ外確保
    let mut indices: Vec<usize> = Vec::with_capacity(16384);

    let mut current_beam_width: usize = 300;
    let mut seen_generation = vec![0u32; 65536];
    let mut current_generation = 0u32;

    while !beam.is_empty() {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        if elapsed_ms >= TIME_LIMIT_MS {
            break;
        }

        current_generation += 1;

        let remaining_time = TIME_LIMIT_MS.saturating_sub(elapsed_ms);
        if remaining_time < 50 {
            current_beam_width = 30;
        } else if remaining_time < 100 {
            current_beam_width = 100;
        } else if remaining_time < 200 {
            current_beam_width = 200;
        } else if remaining_time < 500 {
            current_beam_width = 500;
        } else if remaining_time < 1000 {
            current_beam_width = 2000;
        } else {
            current_beam_width = 4000;
        }

        let panic_mode = remaining_time < 300; // 残り300msを切ったら妥協モード開始

        next_beam.clear();

        for state in &beam {
            if state.len == input.M && state.score == state.turn as i64 {
                if state.score < best_score {
                    best_score = state.score;
                    best_tree_id = state.tree_node_id;
                    best_state = state.clone();
                }
                continue;
            }

            for dir in 0..4 {
                let mut next_state = state.clone();
                let dummy_id = state.tree_node_id;
                if next_state.apply(
                    dir,
                    &input,
                    &target_seq,
                    &target_path_dist,
                    dummy_id,
                    &mut bfs_ctx,
                    panic_mode,
                ) {
                    let child_tree_id = tree.add_child(state.tree_node_id, dir as u8);
                    next_state.tree_node_id = child_tree_id;
                    next_beam.push(next_state);
                }
            }
        }

        if next_beam.is_empty() {
            break;
        }

        // 改善点1 & 2: indicesの使い回しとselect_nth_unstable_by_keyの適用
        indices.clear();
        indices.extend(0..next_beam.len());

        // 重複排除により数が減ることを考慮し、必要なビーム幅の2倍（余裕幅）を抽出
        let margin_width = (current_beam_width * 2).min(next_beam.len());

        if margin_width < next_beam.len() {
            indices.select_nth_unstable_by_key(margin_width, |&i| next_beam[i].score);
            indices.truncate(margin_width);
        }

        // 抽出した範囲内を厳密にソート
        indices.sort_unstable_by_key(|&i| next_beam[i].score);

        beam.clear();

        for &i in &indices {
            let state = &next_beam[i];
            let key = (state.get_pos(0) as usize) << 8 | state.len;
            if seen_generation[key] != current_generation {
                seen_generation[key] = current_generation;
                beam.push(state.clone());
                if beam.len() == current_beam_width {
                    break;
                }
            }
        }

        if !beam.is_empty() && beam[0].score < best_score {
            best_score = beam[0].score;
            best_tree_id = beam[0].tree_node_id;
            best_state = beam[0].clone();
        }
    }

    let mut final_state = best_state.clone();
    let mut final_tree_id = best_tree_id;

    // 長さが M に達していない場合、ペナルティ回避のために手近な餌を何でも食べる
    while final_state.len < input.M {
        let mut dist = [25000i32; 256];
        let mut parent_pos = [255u8; 256];
        let mut parent_dir = [255u8; 256];
        let mut q = [0u8; 256];
        let mut head = 0;
        let mut tail = 0;

        let start = final_state.get_pos(0);
        dist[start as usize] = 0;
        q[tail] = start;
        tail += 1;

        let mut target_food = 255;

        // BFSで最も近い餌（色問わず）を探す
        while head < tail {
            let u = q[head];
            head += 1;

            if final_state.f[u as usize] > 0 {
                target_food = u;
                break;
            }

            let d = dist[u as usize];
            let ui = (u / 16) as isize;
            let uj = (u % 16) as isize;

            for dir in 0..4 {
                let (di, dj) = DIJ[dir];
                let ni = ui + di;
                let nj = uj + dj;
                if ni >= 0 && ni < input.N as isize && nj >= 0 && nj < input.N as isize {
                    let v = (ni * 16 + nj) as u8;
                    // Uターンや自分の体への衝突を壁として扱う
                    if !get_bit(&final_state.body_bits, v) {
                        if dist[v as usize] > d + 1 {
                            dist[v as usize] = d + 1;
                            parent_pos[v as usize] = u;
                            parent_dir[v as usize] = dir as u8;
                            q[tail] = v;
                            tail += 1;
                        }
                    }
                }
            }
        }

        if target_food == 255 {
            break; // 完全な詰み（餌にたどり着けない）
        }

        // 経路の復元
        let mut path = Vec::new();
        let mut curr = target_food;
        while curr != start {
            path.push(parent_dir[curr as usize]);
            curr = parent_pos[curr as usize];
        }
        path.reverse();

        // 経路を適用してMoveTreeに強制的に履歴を追加
        for dir in path {
            let next_tree_id = tree.add_child(final_tree_id, dir);

            // ※ applyの引数は現在の実装に合わせて調整してください
            // もし bfs_ctx や panic_mode がある場合は、適当なダミーを渡してOKです
            final_state.apply(
                dir as usize,
                &input,
                &target_seq,
                &target_path_dist,
                next_tree_id,
                &mut bfs_ctx, // 追加
                true,         // 追加 (panic_mode)
            );
            final_tree_id = next_tree_id;
        }
    }

    // 最終的に延長された履歴を復元する
    let history = tree.reconstruct(final_tree_id);
    for &dir in &history {
        println!("{}", DIR_CHARS[dir as usize]);
    }
}
