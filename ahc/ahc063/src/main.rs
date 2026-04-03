#![allow(nonstandard_style)]

use std::io::{BufRead, stdin};
use std::time::{Duration, Instant};

// =============================================
// Scanner & Macros
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
            self.reader
                .read_until(b'\n', &mut self.buf_str)
                .expect("Failed to read line");
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
    ($sc:expr, ($($t:tt),*)) => { ( $(read_value!($sc, $t)),* ) };
    ($sc:expr, [$t:tt; $len:expr]) => { (0..$len).map(|_| read_value!($sc, $t)).collect::<Vec<_>>() };
    ($sc:expr, chars) => { $sc.token::<String>().chars().collect::<Vec<char>>() };
    ($sc:expr, usize1) => { $sc.token::<usize>() - 1 };
    ($sc:expr, isize1) => { $sc.token::<isize>() - 1 };
    ($sc:expr, $t:ty) => { $sc.token::<$t>() };
}

#[macro_export]
macro_rules! input {
    ($sc:expr $(,)*) => {};
    ($sc:expr, mut $($var:ident),+ : $t:tt $(, $($r:tt)*)?) => {
        $( let mut $var = read_value!($sc, $t); )+
        $(input!($sc, $($r)*);)?
    };
    ($sc:expr, $($var:ident),+ : $t:tt $(, $($r:tt)*)?) => {
        $( let $var = read_value!($sc, $t); )+
        $(input!($sc, $($r)*);)?
    };
}

// =============================================
// Main Logic
// =============================================

const TIME_LIMIT_MS: u64 = 1900;
const SA_TIME_LIMIT_MS: u64 = 100;
const BEAM_WIDTH: usize = 1000;
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)]; // U, D, L, R
const DIR_CHARS: [char; 4] = ['U', 'D', 'L', 'R'];

// 簡易Xorshift（外部クレート不要の高速乱数）
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
    input! {
        sc,
        N: usize, M: usize, C: usize,
        d: [usize; M],
        f: [[usize; N]; N],
    }
    Input { N, M, C, d, f }
}

// ---------------------------------------------------------
// 上位レイヤー：焼きなまし法による「食べる順番」の最適化
// ---------------------------------------------------------
fn optimize_targets(input: &Input, time_limit: Duration) -> Vec<u8> {
    let start = Instant::now();
    let mut rng = XorShift::new(42);

    let mut foods_by_color = vec![vec![]; input.C + 1];
    for i in 0..input.N {
        for j in 0..input.N {
            let color = input.f[i][j];
            if color > 0 {
                foods_by_color[color].push((i as isize, j as isize));
            }
        }
    }

    // 初期解の作成
    let mut targets = vec![(0isize, 0isize); input.M];
    targets[4] = (4, 0); // 初期状態の頭の位置
    let mut color_usage = vec![0; input.C + 1];

    for k in 5..input.M {
        let c = input.d[k];
        let idx = color_usage[c];
        color_usage[c] += 1;
        targets[k] = foods_by_color[c][idx];
    }

    let calc_score = |t: &[(isize, isize)]| -> i32 {
        let mut score = 0;
        for k in 5..input.M {
            score += (t[k].0 - t[k - 1].0).abs() + (t[k].1 - t[k - 1].1).abs();
        }
        score as i32
    };

    let mut current_score = calc_score(&targets);
    let mut best_targets = targets.clone();
    let mut best_score = current_score;

    // スワップ可能な（同色の）インデックス群
    let mut indices_by_color = vec![vec![]; input.C + 1];
    for k in 5..input.M {
        indices_by_color[input.d[k]].push(k);
    }
    let valid_colors: Vec<usize> = (1..=input.C)
        .filter(|&c| indices_by_color[c].len() >= 2)
        .collect();

    if !valid_colors.is_empty() {
        let t0 = 10.0;
        let t1 = 0.1;
        let mut iter = 0;

        loop {
            if iter & 127 == 0 && start.elapsed() >= time_limit {
                break;
            }
            iter += 1;

            let c = valid_colors[rng.next_usize(valid_colors.len())];
            let pool = &indices_by_color[c];
            let i_idx = rng.next_usize(pool.len());
            let mut j_idx = rng.next_usize(pool.len());
            while i_idx == j_idx {
                j_idx = rng.next_usize(pool.len());
            }

            let u = pool[i_idx];
            let v = pool[j_idx];

            targets.swap(u, v);
            let next_score = calc_score(&targets);

            let accept = if next_score <= current_score {
                true
            } else {
                let temp =
                    t0 + (t1 - t0) * (start.elapsed().as_secs_f64() / time_limit.as_secs_f64());
                let prob = f64::exp((current_score - next_score) as f64 / temp);
                (rng.next() as f64 / std::u32::MAX as f64) < prob
            };

            if accept {
                current_score = next_score;
                if current_score < best_score {
                    best_score = current_score;
                    best_targets.copy_from_slice(&targets);
                }
            } else {
                targets.swap(u, v); // 棄却なら戻す
            }
        }
    }

    // 1D座標(0~255)に変換して返す
    best_targets
        .iter()
        .map(|&(i, j)| (i * 16 + j) as u8)
        .collect()
}

// ---------------------------------------------------------
// 下位レイヤー：ビームサーチ用状態管理
// ---------------------------------------------------------
#[derive(Clone)]
struct State {
    f: [u8; 256],
    ij: [u8; 256],
    c: [u8; 256],
    len: usize,
    turn: usize,
    score: i64,
    history: Vec<u8>,
}

impl State {
    fn new(input: &Input, target_seq: &[u8]) -> Self {
        let mut f = [0u8; 256];
        for i in 0..input.N {
            for j in 0..input.N {
                f[i * 16 + j] = input.f[i][j] as u8;
            }
        }

        let mut ij = [0u8; 256];
        for i in 0..5 {
            ij[i] = ((4 - i) * 16) as u8;
        }

        let mut c = [0u8; 256];
        for i in 0..5 {
            c[i] = 1;
        }

        let mut state = Self {
            f,
            ij,
            c,
            len: 5,
            turn: 0,
            score: 0,
            history: Vec::with_capacity(1024),
        };
        state.score = state.evaluate(input, target_seq);
        state
    }

    fn apply(&mut self, dir: usize, input: &Input, target_seq: &[u8]) -> bool {
        let head_pos = self.ij[0];
        let hi = (head_pos / 16) as isize;
        let hj = (head_pos % 16) as isize;
        let (di, dj) = DIJ[dir];
        let ni = hi + di;
        let nj = hj + dj;

        if ni < 0 || ni >= input.N as isize || nj < 0 || nj >= input.N as isize {
            return false;
        }

        let new_pos = (ni * 16 + nj) as u8;
        if self.len > 1 && new_pos == self.ij[1] {
            return false;
        }

        let eaten_color = self.f[new_pos as usize];

        if eaten_color != 0 {
            self.f[new_pos as usize] = 0;
            self.ij.copy_within(0..self.len, 1);
            self.ij[0] = new_pos;
            self.c[self.len] = eaten_color;
            self.len += 1;
        } else {
            self.ij.copy_within(0..self.len - 1, 1);
            self.ij[0] = new_pos;
        }

        // 噛みちぎりチェック
        if self.len >= 3 {
            let mut bite_idx = None;
            for h in 1..=(self.len - 2) {
                if self.ij[h] == new_pos {
                    bite_idx = Some(h);
                    break;
                }
            }

            if let Some(h) = bite_idx {
                for p in h + 1..self.len {
                    let pos = self.ij[p];
                    let col = self.c[p];
                    self.f[pos as usize] = col;
                }
                self.len = h + 1;
            }
        }

        self.turn += 1;
        self.history.push(dir as u8);
        self.score = self.evaluate(input, target_seq);
        true
    }

    fn bfs_to_target(&self, input: &Input, target: u8) -> i32 {
        if target == 255 {
            return 255;
        }
        let mut dist = [255u8; 256];
        let mut q = [0u8; 256];
        let mut head = 0;
        let mut tail = 0;

        let start = self.ij[0];
        dist[start as usize] = 0;
        q[tail] = start;
        tail += 1;

        let mut is_body = [false; 256];
        for i in 1..self.len {
            is_body[self.ij[i] as usize] = true;
        }

        while head < tail {
            let u = q[head];
            head += 1;
            let d = dist[u as usize];

            if u == target {
                return d as i32;
            }

            let ui = (u / 16) as isize;
            let uj = (u % 16) as isize;

            for &(di, dj) in &DIJ {
                let ni = ui + di;
                let nj = uj + dj;
                if ni >= 0 && ni < input.N as isize && nj >= 0 && nj < input.N as isize {
                    let v = (ni * 16 + nj) as usize;
                    if !is_body[v] && dist[v] == 255 {
                        dist[v] = d + 1;
                        q[tail] = v as u8;
                        tail += 1;
                    }
                }
            }
        }
        255
    }

    fn evaluate(&self, input: &Input, target_seq: &[u8]) -> i64 {
        let mut e = 0;
        for p in 0..self.len {
            if input.d[p] != self.c[p] as usize {
                e += 1;
            }
        }

        // 競技ルールの絶対スコア（長不足は2万点ペナルティ）
        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));

        let mut dist_penalty = 0;
        if self.len < input.M {
            let expected_pos = target_seq[self.len];
            let target_color = input.d[self.len] as u8;

            // SAで決めた位置に目当ての色が本当にあるか確認（噛みちぎり等でズレた場合のフォールバック）
            let actual_target = if self.f[expected_pos as usize] == target_color {
                expected_pos
            } else {
                let mut best_pos = 255;
                let mut min_d = 1000;
                let hi = (self.ij[0] / 16) as isize;
                let hj = (self.ij[0] % 16) as isize;
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

            let bfs_dist = self.bfs_to_target(input, actual_target);
            dist_penalty = if bfs_dist == 255 {
                10000 // 障害物で塞がれている場合は大きなペナルティ（噛みちぎりを誘発）
            } else {
                bfs_dist as i64 * 10
            };
        }

        base + dist_penalty
    }
}

fn main() {
    let start_time = Instant::now();
    let sa_time_limit = Duration::from_millis(SA_TIME_LIMIT_MS);
    let total_time_limit = Duration::from_millis(TIME_LIMIT_MS);

    let mut sc = Scanner::new();
    let input = parse_input(&mut sc);

    // 1. 上位レイヤー：焼きなまし法でターゲット座標の順序を最適化（100ms使用）
    let target_seq = optimize_targets(&input, sa_time_limit);

    // 2. 下位レイヤー：ビームサーチ（残り時間使用）
    let initial_state = State::new(&input, &target_seq);
    let mut beam = vec![initial_state];
    let mut best_state = beam[0].clone();
    let mut seen = vec![false; 65536];

    while !beam.is_empty() {
        if start_time.elapsed() >= total_time_limit {
            break;
        }

        let mut next_beam = Vec::with_capacity(beam.len() * 4);

        for state in &beam {
            if state.len == input.M && state.evaluate(&input, &target_seq) == state.turn as i64 {
                if state.score < best_state.score {
                    best_state = state.clone();
                }
                continue;
            }

            for dir in 0..4 {
                let mut next_state = state.clone();
                if next_state.apply(dir, &input, &target_seq) {
                    next_beam.push(next_state);
                }
            }
        }

        if next_beam.is_empty() {
            break;
        }

        next_beam.sort_unstable_by_key(|s| s.score);

        let mut unique_states = Vec::with_capacity(BEAM_WIDTH);
        seen.fill(false);

        // 重複排除：同じ頭の位置＆同じ長さのものは、一番スコアが良いものだけ残す
        for state in next_beam {
            let key = (state.ij[0] as usize) << 8 | state.len;
            if !seen[key] {
                seen[key] = true;
                unique_states.push(state);
                if unique_states.len() == BEAM_WIDTH {
                    break;
                }
            }
        }

        if unique_states[0].score < best_state.score {
            best_state = unique_states[0].clone();
        }

        beam = unique_states;
    }

    for &dir in &best_state.history {
        println!("{}", DIR_CHARS[dir as usize]);
    }
}
