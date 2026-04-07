#![allow(nonstandard_style)]
#![allow(unused_assignments)]

use std::io::{BufRead, stdin};
use std::time::Instant;

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
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
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
    fn new(input: &Input, tree: &mut MoveTree, bfs_ctx: &mut BfsContext) -> Self {
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
        state.score = state.evaluate(input, bfs_ctx, false);
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
        self.score = self.evaluate(input, bfs_ctx, panic_mode);
        true
    }

    fn cost_to_target(&self, input: &Input, target_color: u8, ctx: &mut BfsContext) -> i32 {
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

            // 特定のマスではなく、欲しい「色」が見つかったらその距離を返す！
            if self.f[u as usize] == target_color {
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
                        // 間違った色も障害物扱い
                        let is_wrong_food =
                            self.f[v as usize] != 0 && self.f[v as usize] != target_color;
                        if is_wrong_food {
                            continue;
                        }

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
        25000 // 盤面のどこを探しても欲しい色に到達できない場合のみ詰み
    }

    fn evaluate(&self, input: &Input, bfs_ctx: &mut BfsContext, panic_mode: bool) -> i64 {
        let e = self.error_count + 1;
        if self.len == input.M {
            return self.turn as i64 + 10000 * e as i64;
        }
        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));
        let target_color = input.d[self.len] as u8;

        // マンハッタン距離でターゲットを絞り込む無駄な処理を全削除し、直接色を探させる
        let cost = self.cost_to_target(input, target_color, bfs_ctx) as i64;

        let mut penalty_weight = 32500;

        if panic_mode || cost >= 20000 {
            penalty_weight = 5000;
        }

        let heuristic_error_penalty = (penalty_weight * e * e * 2 / 3) as i64;

        base + heuristic_error_penalty + cost
    }
}

fn main() {
    let start_time = Instant::now();

    let mut sc = Scanner::new();
    let input = parse_input(&mut sc);

    let mut tree = MoveTree::new();
    let mut bfs_ctx = BfsContext::new(); // BFS用コンテキストの初期化

    let initial_state = State::new(&input, &mut tree, &mut bfs_ctx);
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
                if next_state.apply(dir, &input, dummy_id, &mut bfs_ctx, panic_mode) {
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
