use std::io::{BufRead, stdin};
use std::time::{Duration, Instant};

// =============================================
// Scanner & Macros (変更なしのため省略せずにそのまま配置)
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
const BEAM_WIDTH: usize = 1000;
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)]; // U, D, L, R
const DIR_CHARS: [char; 4] = ['U', 'D', 'L', 'R'];

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
    fn new(input: &Input) -> Self {
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
        state.score = state.evaluate(input);
        state
    }

    fn apply(&mut self, dir: usize, input: &Input) -> bool {
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
        self.score = self.evaluate(input);
        true
    }

    // BFSで「目当ての色の餌」と「何でもいいから一番近い餌」までの実距離を測る
    fn bfs_dist(&self, input: &Input) -> (i32, i32) {
        let mut dist = [255u8; 256];
        let mut q = [0u8; 256];
        let mut head = 0;
        let mut tail = 0;

        let start = self.ij[0];
        dist[start as usize] = 0;
        q[tail] = start;
        tail += 1;

        // 自分の体を障害物としてマーク
        let mut is_body = [false; 256];
        for i in 1..self.len {
            is_body[self.ij[i] as usize] = true;
        }

        let mut min_dist_target = 255;
        let mut min_dist_any = 255;
        let target_color = if self.len < input.M {
            input.d[self.len] as u8
        } else {
            0
        };

        while head < tail {
            let u = q[head];
            head += 1;
            let d = dist[u as usize];

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

                        let color = self.f[v];
                        if color != 0 {
                            if min_dist_any == 255 {
                                min_dist_any = d + 1;
                            }
                            if color == target_color && min_dist_target == 255 {
                                min_dist_target = d + 1;
                            }
                        }
                    }
                }
            }
        }
        (min_dist_target as i32, min_dist_any as i32)
    }

    fn evaluate(&self, input: &Input) -> i64 {
        let mut e = 0;
        for p in 0..self.len {
            if input.d[p] != self.c[p] as usize {
                e += 1;
            }
        }

        // 競技ルールの絶対スコア（餌を食べるだけで2万点下がるので最強のインセンティブ）
        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));

        let mut dist_penalty = 0;
        if self.len < input.M {
            let (dist_target, dist_any) = self.bfs_dist(input);

            if dist_target != 255 {
                // 目当ての色に辿り着けるならそこへ向かう
                dist_penalty = dist_target as i64 * 10;
            } else if dist_any != 255 {
                // 目当ての色が体で塞がれているなら、何でもいいから近い餌を食って体を伸ばす（ペナルティは重めにして妥協ルートと認識させる）
                dist_penalty = 1000 + dist_any as i64 * 10;
            } else {
                // 完全に自分の体で詰んでいる状態（噛みちぎるしかない）
                dist_penalty = 100000;
            }
        }

        base + dist_penalty
    }
}

fn main() {
    let start_time = Instant::now();
    let time_limit = Duration::from_millis(TIME_LIMIT_MS);

    let mut sc = Scanner::new();
    let input = parse_input(&mut sc);

    let initial_state = State::new(&input);
    let mut beam = vec![initial_state];
    let mut best_state = beam[0].clone();

    // 状態重複排除用の配列をループ外で確保（メモリアロケーションを抑制）
    // 頭の座標(最大255)と長さ(最大255)の組み合わせは16ビット(65536)に収まる
    let mut seen = vec![false; 65536];

    while !beam.is_empty() {
        if start_time.elapsed() >= time_limit {
            break;
        }

        let mut next_beam = Vec::with_capacity(beam.len() * 4);

        for state in &beam {
            if state.len == input.M && state.evaluate(&input) == state.turn as i64 {
                if state.score < best_state.score {
                    best_state = state.clone();
                }
                continue;
            }

            for dir in 0..4 {
                let mut next_state = state.clone();
                if next_state.apply(dir, &input) {
                    next_beam.push(next_state);
                }
            }
        }

        if next_beam.is_empty() {
            break;
        }

        // スコアで昇順ソート
        next_beam.sort_unstable_by_key(|s| s.score);

        let mut unique_states = Vec::with_capacity(BEAM_WIDTH);
        seen.fill(false); // ターンごとにリセット

        // 重複排除：同じ頭の位置・同じ長さの状態は、一番スコアが良い（＝最速・最適な色の）ものだけ残す
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
