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

const TIME_LIMIT_MS: u64 = 1900;
const SA_TIME_LIMIT_MS: u64 = 100;
const BEAM_WIDTH: usize = 1000;
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)]; // U, D, L, R
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
// Bitboard Utils
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
// 上位レイヤー：焼きなまし法 (変更なしのため一部省略形)
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
    targets[4] = (4, 0);
    let mut usage = vec![0; input.C + 1];
    for k in 5..input.M {
        let c = input.d[k];
        targets[k] = foods[c][usage[c]];
        usage[c] += 1;
    }
    let calc_score = |t: &[(isize, isize)]| -> i32 {
        (5..input.M)
            .map(|k| {
                (t[k].0 as i32 - t[k - 1].0 as i32).abs()
                    + (t[k].1 as i32 - t[k - 1].1 as i32).abs()
            })
            .sum()
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

// ---------------------------------------------------------
// 下位レイヤー：ビームサーチ用状態管理 (最適化版)
// ---------------------------------------------------------
#[derive(Clone)]
struct State {
    f: [u8; 256],
    ij: [u8; 256], // リングバッファ化
    c: [u8; 256],
    body_bits: [u64; 4], // Bitboard導入
    head_ptr: u8,        // リングバッファの先頭インデックス
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
        state.score = state.evaluate(input, target_seq);
        state
    }

    // リングバッファから特定のインデックスの座標を取得する O(1)
    #[inline(always)]
    fn get_pos(&self, idx: usize) -> u8 {
        self.ij[self.head_ptr.wrapping_add(idx as u8) as usize]
    }

    fn apply(&mut self, dir: usize, input: &Input, target_seq: &[u8]) -> bool {
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
        } // Uターン禁止

        let eaten_color = self.f[new_pos as usize];

        if eaten_color != 0 {
            // 食事：長さが伸びるため尻尾はそのまま
            self.f[new_pos as usize] = 0;
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            self.c[self.len] = eaten_color;
            self.len += 1;
            set_bit(&mut self.body_bits, new_pos);
        } else {
            // 移動：尻尾が離れるのでBitboardから消す（噛みちぎり判定の前に消すのが味噌）
            let tail_pos = self.get_pos(self.len - 1);
            clear_bit(&mut self.body_bits, tail_pos);
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            set_bit(&mut self.body_bits, new_pos);
        }

        // 噛みちぎりチェック (Bitboardにより衝突判定がO(1))
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
                    clear_bit(&mut self.body_bits, pos); // Bitboardからも削除
                }
                self.len = bite_idx + 1;
            }
        }

        self.turn += 1;
        self.history.push(dir as u8);
        self.score = self.evaluate(input, target_seq);
        true
    }

    // Bitboardの恩恵で最軽量化されたBFS
    fn bfs_to_target(&self, input: &Input, target: u8) -> i32 {
        if target == 255 {
            return 255;
        }
        let mut dist = [255u8; 256];
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
                    let v = (ni * 16 + nj) as u8;
                    // 配列ではなくビット演算で障害物確認
                    if dist[v as usize] == 255 && !get_bit(&self.body_bits, v) {
                        dist[v as usize] = d + 1;
                        q[tail] = v;
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

        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));

        let mut dist_penalty = 0;
        if self.len < input.M {
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

            let bfs_dist = self.bfs_to_target(input, actual_target);
            dist_penalty = if bfs_dist == 255 {
                10000
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

    let target_seq = optimize_targets(&input, sa_time_limit);

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

        // [最適化4] 構造体そのものではなく「インデックス」だけをソートしてメモリ帯域を節約
        let mut indices: Vec<usize> = (0..next_beam.len()).collect();
        indices.sort_unstable_by_key(|&i| next_beam[i].score);

        let mut unique_states = Vec::with_capacity(BEAM_WIDTH);
        seen.fill(false);

        for &i in &indices {
            let state = &next_beam[i];
            let key = (state.get_pos(0) as usize) << 8 | state.len;
            if !seen[key] {
                seen[key] = true;
                unique_states.push(state.clone());
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
