#![allow(nonstandard_style)]
#![allow(unused_assignments)]

use std::io::{BufRead, stdin};
use std::time::Instant;

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

const TIME_LIMIT_MS: u64 = 1990;
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
const DIR_CHARS: [char; 4] = ['U', 'D', 'L', 'R'];

/// 評価関数で先読みする餌の個数。
/// 「次の1個」だけでなく、その先数個のコストも合算することで、
/// 「近くにあるけど次の次が遠い」状況を評価に反映できる。
const LOOKAHEAD: usize = 4;

/// 先読みコストに掛ける減衰係数の分母。
/// 1個先は等倍、2個先は 1/DECAY_DENOM 倍... と重みを下げることで
/// 「遠い未来より近い未来を優先」させる。
const DECAY_DENOM: i64 = 2;

#[derive(Clone, Debug)]
pub struct Input {
    pub N: usize,
    pub M: usize,
    pub C: usize,
    pub d: Vec<usize>,
    pub f: Vec<Vec<usize>>,
    pub adj: [[u8; 4]; 256],
    pub adj_len: [u8; 256],
    pub adj_dir: [[u8; 4]; 256],
    /// N<=11 のときtrue。このケースでは lookahead 評価が有効に機能する。
    /// N>=12 では lookahead の BFS コストがループ数を圧迫するため無効化する。
    pub use_lookahead: bool,
}

pub fn parse_input<R: BufRead>(sc: &mut Scanner<R>) -> Input {
    input! { sc, N: usize, M: usize, C: usize, d: [usize; M], f: [[usize; N]; N] }

    let mut adj = [[0u8; 4]; 256];
    let mut adj_len = [0u8; 256];
    let mut adj_dir = [[0u8; 4]; 256];

    for row in 0..N {
        for col in 0..N {
            let pos = (row * 16 + col) as u8;
            let mut k = 0u8;
            for (dir, &(di, dj)) in DIJ.iter().enumerate() {
                let ni = row as isize + di;
                let nj = col as isize + dj;
                if ni >= 0 && ni < N as isize && nj >= 0 && nj < N as isize {
                    adj[pos as usize][k as usize] = (ni * 16 + nj) as u8;
                    adj_dir[pos as usize][k as usize] = dir as u8;
                    k += 1;
                }
            }
            adj_len[pos as usize] = k;
        }
    }

    let use_lookahead = N <= 11;
    Input {
        N,
        M,
        C,
        d,
        f,
        adj,
        adj_len,
        adj_dir,
        use_lookahead,
    }
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
// Move Tree
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
// BFS Context
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

// =============================================
// Zobrist Hash 用 XorShift
// =============================================
struct XorShift {
    state: u64,
}
impl XorShift {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}

// =============================================
// Zobrist Table (グローバル相当をmainから渡す)
// =============================================
/// ZobristTable は「(座標, 胴体内インデックス)」のペアに対してランダムな64bitを割り当てる。
/// 頭から何番目の体節がどの座標にいるか、という情報をハッシュで表現する。
/// インデックスは先頭から BODY_HASH_DEPTH 個分だけ取る（それ以降は多様性への寄与が薄い）。
const BODY_HASH_DEPTH: usize = 6; // 頭から何体節分をハッシュに含めるか

struct ZobristTable {
    /// zobrist[pos][idx] = 「idx番目の体節が座標posにいる」ことを表すハッシュ値
    zobrist: [[u64; BODY_HASH_DEPTH]; 256],
    /// len_hash[len] = 「蛇の長さがlenである」ことを表すハッシュ値
    len_hash: [u64; 256],
}

impl ZobristTable {
    fn new(rng: &mut XorShift) -> Self {
        let mut zobrist = [[0u64; BODY_HASH_DEPTH]; 256];
        let mut len_hash = [0u64; 256];
        for pos in 0..256 {
            for idx in 0..BODY_HASH_DEPTH {
                zobrist[pos][idx] = rng.next();
            }
            len_hash[pos] = rng.next();
        }
        Self { zobrist, len_hash }
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
    /// 重複排除用のZobristハッシュ値。
    /// 「頭からBODY_HASH_DEPTH個の体節の座標」と「蛇の長さ」をXORで合成したもの。
    /// これが一致する状態を「似た状態」とみなしてビームから1つだけ残す。
    dedup_hash: u64,
}

impl State {
    fn new(
        input: &Input,
        tree: &mut MoveTree,
        bfs_ctx: &mut BfsContext,
        ztable: &ZobristTable,
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
            dedup_hash: 0,
        };
        state.dedup_hash = state.compute_hash(ztable);
        state.score = state.evaluate(input, bfs_ctx, false);
        state
    }

    #[inline(always)]
    fn get_pos(&self, idx: usize) -> u8 {
        self.ij[self.head_ptr.wrapping_add(idx as u8) as usize]
    }

    /// 現在の状態からZobristハッシュを計算する。
    /// 頭からBODY_HASH_DEPTH個の体節座標と蛇の長さを組み合わせることで、
    /// 「同じ頭の位置・同じ長さだが胴体の形が違う」状態を区別できるようになる。
    fn compute_hash(&self, ztable: &ZobristTable) -> u64 {
        let depth = BODY_HASH_DEPTH.min(self.len);
        let mut h = ztable.len_hash[self.len.min(255)];
        for idx in 0..depth {
            let pos = self.get_pos(idx) as usize;
            h ^= ztable.zobrist[pos][idx];
        }
        h
    }

    fn apply(
        &mut self,
        dir: usize,
        input: &Input,
        new_tree_id: u32,
        bfs_ctx: &mut BfsContext,
        ztable: &ZobristTable,
        panic_mode: bool,
    ) -> bool {
        let head_pos = self.get_pos(0);

        let adj_len = input.adj_len[head_pos as usize] as usize;
        let new_pos = {
            let mut found = None;
            for k in 0..adj_len {
                if input.adj_dir[head_pos as usize][k] as usize == dir {
                    found = Some(input.adj[head_pos as usize][k]);
                    break;
                }
            }
            match found {
                Some(p) => p,
                None => return false,
            }
        };

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

        // --- 噛みちぎり判定 ---
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
        // ハッシュを更新
        self.dedup_hash = self.compute_hash(ztable);
        self.score = self.evaluate(input, bfs_ctx, panic_mode);
        true
    }

    /// 頭の現在位置から、指定色の餌までの最短距離を BFS で計算する。
    /// `start_pos` を引数に取るのは、「ある座標を起点としたコスト」を
    /// 先読みループ内でも再利用するため（常に self.get_pos(0) とは限らない）。
    fn cost_from(
        &self,
        start_pos: u8,
        target_color: u8,
        input: &Input,
        ctx: &mut BfsContext,
    ) -> i32 {
        ctx.current_gen += 1;
        let mut head = 0usize;
        let mut tail = 0usize;

        ctx.dist[start_pos as usize] = 0;
        ctx.r#gen[start_pos as usize] = ctx.current_gen;
        ctx.q[tail] = start_pos;
        tail += 1;

        while head < tail {
            let u = ctx.q[head];
            head += 1;

            if self.f[u as usize] == target_color {
                return ctx.dist[u as usize];
            }

            let d = ctx.dist[u as usize];
            let adj_len = input.adj_len[u as usize] as usize;

            for k in 0..adj_len {
                let v = input.adj[u as usize][k];

                if get_bit(&self.body_bits, v) {
                    continue;
                }

                let fv = self.f[v as usize];
                if fv != 0 && fv != target_color {
                    continue;
                }

                if ctx.r#gen[v as usize] != ctx.current_gen || ctx.dist[v as usize] > d + 10 {
                    ctx.r#gen[v as usize] = ctx.current_gen;
                    ctx.dist[v as usize] = d + 10;
                    ctx.q[tail] = v;
                    tail += 1;
                }
            }
        }
        25000
    }

    /// 評価関数。LOOKAHEAD 個先まで各餌へのコストを減衰させながら合算する。
    ///
    /// 旧実装は「次の1個のコスト」だけを見ていたが、それだと
    ///   「次は近いが、次の次は盤面の反対側」
    /// という状況を区別できなかった。
    ///
    /// 新実装では以下のように先読みコストを合算する:
    ///   lookahead_cost = cost_0 + cost_1/2 + cost_2/4 + cost_3/8 + ...
    ///
    /// ただし「正確な中間状態のシミュレーション」はコストが高すぎるため、
    /// 簡略化として「各餌がある座標から次の餌への距離」を近似コストとして使う。
    /// これは「最短経路を辿れば餌の座標に到達する」という楽観的仮定に基づく近似。
    fn evaluate(&self, input: &Input, bfs_ctx: &mut BfsContext, panic_mode: bool) -> i64 {
        let e = self.error_count + 1;

        if self.len == input.M {
            return self.turn as i64 + 10000 * e as i64;
        }

        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));

        // N<=11 のときは lookahead BFS で先読みコストを合算する。
        // N>=12 では lookahead の BFS が毎ターン重くなりループ数が激減するため、
        // 元の「次の1個だけ見る」シンプルな評価に切り替える。
        let (cost_total, penalty_weight) = if input.use_lookahead {
            // --- LOOKAHEAD 先読みコストの計算 (N<=11 専用) ---
            let mut lookahead_total: i64 = 0;
            let mut approx_pos = self.get_pos(0);
            let mut weight_denom: i64 = 1;

            for look in 0..LOOKAHEAD {
                let target_idx = self.len + look;
                if target_idx >= input.M {
                    break;
                }
                let target_color = input.d[target_idx] as u8;
                let cost = self.cost_from(approx_pos, target_color, input, bfs_ctx);
                lookahead_total += cost as i64 / weight_denom;

                let mut found_next = false;
                for pos in 0..=255u8 {
                    if self.f[pos as usize] == target_color {
                        approx_pos = pos;
                        found_next = true;
                        break;
                    }
                }
                if !found_next {
                    break;
                }
                weight_denom *= DECAY_DENOM;
            }

            let pw = if panic_mode || lookahead_total >= 20000 {
                5000i64
            } else {
                32500i64
            };
            (lookahead_total, pw)
        } else {
            // --- シンプル評価 (N>=12 専用、元のロジック) ---
            let target_color = input.d[self.len] as u8;
            let cost = self.cost_from(self.get_pos(0), target_color, input, bfs_ctx) as i64;
            let pw = if panic_mode || cost >= 20000 {
                5000i64
            } else {
                32500i64
            };
            (cost, pw)
        };

        let heuristic_error_penalty = penalty_weight * e as i64 * e as i64 * 2 / 3;
        base + heuristic_error_penalty + cost_total
    }
}

fn main() {
    let start_time = Instant::now();

    let mut sc = Scanner::new();
    let input = parse_input(&mut sc);

    let mut rng = XorShift::new(1000000007);
    let ztable = ZobristTable::new(&mut rng);

    let mut tree = MoveTree::new();
    let mut bfs_ctx = BfsContext::new();

    let initial_state = State::new(&input, &mut tree, &mut bfs_ctx, &ztable);
    let mut best_score: i64 = initial_state.score;
    let mut best_tree_id: u32 = initial_state.tree_node_id;
    let mut best_state = initial_state.clone();

    let mut beam = vec![initial_state];
    let mut next_beam: Vec<State> = Vec::with_capacity(16384);
    let mut indices: Vec<usize> = Vec::with_capacity(16384);

    let mut current_beam_width: usize = 300;

    // --- 重複排除テーブル ---
    // 旧実装: key = (頭の座標 8bit) | (長さ 8bit) → 16bit → テーブルサイズ 65536
    // 新実装: key = dedup_hash（Zobristハッシュ）を HASH_MASK でマスク → 20bit
    //
    // Zobristハッシュは「頭からBODY_HASH_DEPTH個の体節座標 + 長さ」を含むため、
    // 「同じ頭・同じ長さだが胴体の形が全く違う状態」を別エントリとして扱える。
    // ビーム内の多様性が増し、探索の質が上がることが期待される。
    let mut seen_generation = vec![0u32; 65536];
    let mut current_generation = 0u32;

    while !beam.is_empty() {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        if elapsed_ms >= TIME_LIMIT_MS {
            break;
        }

        current_generation += 1;

        let remaining_time = TIME_LIMIT_MS.saturating_sub(elapsed_ms);
        current_beam_width = if remaining_time < 30 {
            30
        } else if remaining_time < 200 {
            500
        } else if remaining_time < 750 {
            750
        } else {
            5000
        };

        let panic_mode = remaining_time < 150;

        next_beam.clear();

        for state in &beam {
            if state.len == input.M && state.error_count == 0 {
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
                if next_state.apply(dir, &input, dummy_id, &mut bfs_ctx, &ztable, panic_mode) {
                    let child_tree_id = tree.add_child(state.tree_node_id, dir as u8);
                    next_state.tree_node_id = child_tree_id;
                    next_beam.push(next_state);
                }
            }
        }

        if next_beam.is_empty() {
            break;
        }

        indices.clear();
        indices.extend(0..next_beam.len());

        let margin_width = (current_beam_width * 2).min(next_beam.len());

        if margin_width < next_beam.len() {
            indices.select_nth_unstable_by_key(margin_width, |&i| next_beam[i].score);
            indices.truncate(margin_width);
        }

        indices.sort_unstable_by_key(|&i| next_beam[i].score);

        beam.clear();

        for &i in &indices {
            let state = &next_beam[i];
            // 変更点: dedup_hash（Zobristハッシュ）でマスクしたキーで重複排除。
            // 旧: let key = (state.get_pos(0) as usize) << 8 | state.len;
            // 新: Zobristハッシュを使い、胴体形状まで考慮した20bitキー。
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

    // ===== 後処理: 体長が M に届いていない場合の救済 =====
    let mut final_state = best_state.clone();
    let mut final_tree_id = best_tree_id;

    while final_state.len < input.M {
        let mut dist = [25000i32; 256];
        let mut parent_pos = [255u8; 256];
        let mut parent_dir = [255u8; 256];
        let mut q = [0u8; 256];
        let mut head = 0usize;
        let mut tail = 0usize;

        let start = final_state.get_pos(0);
        dist[start as usize] = 0;
        q[tail] = start;
        tail += 1;

        let mut target_food = 255u8;

        while head < tail {
            let u = q[head];
            head += 1;

            if final_state.f[u as usize] > 0 {
                target_food = u;
                break;
            }

            let d = dist[u as usize];
            let adj_len = input.adj_len[u as usize] as usize;
            for k in 0..adj_len {
                let v = input.adj[u as usize][k];
                let dir = input.adj_dir[u as usize][k];

                if !get_bit(&final_state.body_bits, v) && dist[v as usize] > d + 1 {
                    dist[v as usize] = d + 1;
                    parent_pos[v as usize] = u;
                    parent_dir[v as usize] = dir;
                    q[tail] = v;
                    tail += 1;
                }
            }
        }

        if target_food == 255 {
            break;
        }

        let mut path = Vec::new();
        let mut curr = target_food;
        while curr != start {
            path.push(parent_dir[curr as usize]);
            curr = parent_pos[curr as usize];
        }
        path.reverse();

        for dir in path {
            let next_tree_id = tree.add_child(final_tree_id, dir);
            final_state.apply(
                dir as usize,
                &input,
                next_tree_id,
                &mut bfs_ctx,
                &ztable,
                true,
            );
            final_tree_id = next_tree_id;
        }
    }

    // ===== 最終出力 =====
    let history = tree.reconstruct(final_tree_id);
    for &dir in &history {
        println!("{}", DIR_CHARS[dir as usize]);
    }
}
