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

/// 制限時間 (ms)。AHC等のローカル/提出環境で2秒制限を想定し、
/// 余裕を持って 1990ms に設定しています。
const TIME_LIMIT_MS: u64 = 1990;

/// 4方向の (di, dj) ベクトル。インデックス順は U, D, L, R に対応します。
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// DIJ と同じインデックス順の出力用文字。
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
// Bitboard Utils
// ---------------------------------------------------------
// 盤面 16x16 = 256 マスを 64bit × 4 = 256bit のビットボードで表現します。
// pos は 0..256 の値で、(row << 4) | col の形式 (row*16+col)。
// 上位 2bit (pos >> 6) でワード番号、下位 6bit (pos & 63) でビット位置を求めます。
// インライン化することでホットループ内のオーバーヘッドをゼロにしています。
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
// ビームサーチでは多数の State が共通の祖先を持ちます。
// 各 State に Vec<u8> の手順履歴を持たせると clone コストが大きいので、
// 親ポインタ方式の木構造に履歴を一元管理し、State には node_id (u32) のみ持たせます。
// これにより clone は固定サイズのコピーで済み、メモリ局所性も向上します。
struct MoveTree {
    /// parent[i] = ノード i の親ノード ID。ルートは u32::MAX。
    parent: Vec<u32>,
    /// dir[i] = 親からこのノードに来るときに行った方向 (0..4)。ルートは未使用。
    dir: Vec<u8>,
}

impl MoveTree {
    fn new() -> Self {
        Self {
            // 1<<20 ≒ 100万ノード。ビームサーチで生成されるノード数を見越して予約。
            parent: Vec::with_capacity(1 << 20),
            dir: Vec::with_capacity(1 << 20),
        }
    }

    /// ルートノード (履歴なし、初期状態) を追加してその ID を返します。
    #[inline]
    fn add_root(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(u32::MAX);
        self.dir.push(0);
        id
    }

    /// 親 ID と方向を指定して子ノードを追加し、その ID を返します。
    #[inline]
    fn add_child(&mut self, parent_id: u32, d: u8) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(parent_id);
        self.dir.push(d);
        id
    }

    /// 指定ノードからルートまでを辿り、方向列を時系列順 (古い→新しい) で返します。
    fn reconstruct(&self, mut node_id: u32) -> Vec<u8> {
        let mut dirs = Vec::new();
        while node_id != u32::MAX {
            let p = self.parent[node_id as usize];
            // ルートには対応する dir が無いのでスキップ。
            if p != u32::MAX {
                dirs.push(self.dir[node_id as usize]);
            }
            node_id = p;
        }
        // 葉から根の順で集めたので、最後に反転して時系列順に直します。
        dirs.reverse();
        dirs
    }
}

// ---------------------------------------------------------
// BFS Context
// ---------------------------------------------------------
// 評価関数で State ごとに毎回 BFS を行うため、距離配列の確保コストを避けたい。
// そこで「世代カウンタ」方式を採用: dist 配列はクリアせず、gen 配列で
// 「この世代に書き込まれた値か」を判定します。current_gen をインクリメントするだけで
// 配列全体を論理的にリセットできます。
struct BfsContext {
    /// 各マスへの距離 (有効性は gen で判定)。
    dist: [i32; 256],
    /// dist[i] が書き込まれた世代。current_gen と一致しなければ未訪問扱い。
    /// `gen` は将来の予約語のため raw identifier (r#gen) で記述しています。
    r#gen: [u32; 256],
    current_gen: u32,
    /// BFS 用キュー。盤面サイズが固定 256 なのでスタック領域に確保。
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
/// ビームサーチの 1 ノードに相当する状態。
/// クローンが頻発するため、ヒープ確保を避けて固定長配列で持っています。
#[derive(Clone)]
struct State {
    /// 盤面上に残っている餌の色。0 は空マス。
    f: [u8; 256],
    /// ヘビの体節の位置を保持するリングバッファ。
    /// インデックス head_ptr が頭で、(head_ptr + k) % 256 が k 番目の体節。
    /// 移動時に head_ptr を 1 デクリメントするだけで「先頭追加」が O(1) で行えます。
    ij: [u8; 256],
    /// 各体節がヘビに加わったときに食べた色。c[k] = 体節 k の色。
    c: [u8; 256],
    /// ヘビの体が占有しているマスのビットボード。衝突判定 O(1) 用。
    body_bits: [u64; 4],
    /// リングバッファの頭位置 (u8 で自然に mod 256)。
    head_ptr: u8,
    /// 現在のヘビの長さ。
    len: usize,
    /// 経過ターン数 (= 出力する手数)。
    turn: usize,
    /// 評価関数のスコア。小さいほど良い (最小化問題)。
    score: i64,
    /// 「目標色 d[k] と異なる色を体節 k に食べた」回数。最終スコアのペナルティに直結。
    error_count: usize,
    /// MoveTree 内の自分のノード ID。履歴復元時に使用。
    tree_node_id: u32,
}

impl State {
    /// 初期状態を生成します。ヘビは長さ 5 で左端列 (col=0) に縦に並んだ形で開始します。
    fn new(input: &Input, tree: &mut MoveTree, bfs_ctx: &mut BfsContext) -> Self {
        // 盤面をフラット配列にコピー。row*16+col 形式。
        let mut f = [0u8; 256];
        for i in 0..input.N {
            for j in 0..input.N {
                f[i * 16 + j] = input.f[i][j] as u8;
            }
        }

        // 初期ヘビ: (0,0), (1,0), (2,0), (3,0), (4,0) の縦列。
        // ij[0] が頭 (一番上=row 0)、ij[4] が尾。
        let mut ij = [0u8; 256];
        let mut body_bits = [0u64; 4];
        for i in 0..5 {
            let pos = ((4 - i) * 16) as u8;
            ij[i] = pos;
            set_bit(&mut body_bits, pos);
        }

        // 初期体節の色は全て 1 と仮定 (問題設定依存。要確認)。
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

    /// ヘビの idx 番目の体節 (0=頭) の盤面位置を取得します。
    /// リングバッファなので head_ptr + idx を u8 の wrap で計算。
    #[inline(always)]
    fn get_pos(&self, idx: usize) -> u8 {
        self.ij[self.head_ptr.wrapping_add(idx as u8) as usize]
    }

    /// 1 手 (dir 方向への移動) を適用します。
    /// 不正な手 (壁・Uターン) なら false を返し、状態は変更しません……と言いたい所ですが、
    /// 早期 return の前に状態変更が無いので safe です。
    fn apply(
        &mut self,
        dir: usize,
        input: &Input,
        new_tree_id: u32,
        bfs_ctx: &mut BfsContext,
        panic_mode: bool,
    ) -> bool {
        // 頭の位置から行き先を計算。
        let head_pos = self.get_pos(0);
        let hi = (head_pos / 16) as isize;
        let hj = (head_pos % 16) as isize;
        let (di, dj) = DIJ[dir];
        let ni = hi + di;
        let nj = hj + dj;

        // 盤外チェック。
        if ni < 0 || ni >= input.N as isize || nj < 0 || nj >= input.N as isize {
            return false;
        }

        let new_pos = (ni * 16 + nj) as u8;
        // 真後ろ (首の位置) への U ターンは禁止。
        if self.len > 1 && new_pos == self.get_pos(1) {
            return false;
        }

        let eaten_color = self.f[new_pos as usize];

        if eaten_color != 0 {
            // === 餌を食べた場合 ===
            // 盤面から餌を除去。
            self.f[new_pos as usize] = 0;
            // リングバッファの先頭を 1 つ前に進めて、新しい頭を置く。
            // 体長が 1 増えるので尾は引きずらない。
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            // 新しい体節に色を記録 (インデックスは食べる前の len = 新体節の位置)。
            self.c[self.len] = eaten_color;
            // 目標色と違っていればエラー数を加算。
            if input.d[self.len] != eaten_color as usize {
                self.error_count += 1;
            }
            self.len += 1;
            set_bit(&mut self.body_bits, new_pos);
        } else {
            // === 通常の移動 (餌なし) ===
            // 尾を 1 マス縮め、頭を 1 マス伸ばす。長さは不変。
            let tail_pos = self.get_pos(self.len - 1);
            clear_bit(&mut self.body_bits, tail_pos);
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            set_bit(&mut self.body_bits, new_pos);
        }

        // === 自分の体を噛んだ場合の処理 ===
        // 餌を食べた直後だけ起こり得ます (通常移動では尾を縮めた後に頭を進めるため衝突しない)。
        // 体節 bite_idx 以降を切り捨てて盤面に餌として戻します。
        if self.len >= 3 && get_bit(&self.body_bits, new_pos) {
            let mut bite_idx = 0;
            // 頭 (0) と新しく置いたばかりの体節 (len-1) を除いて検索。
            for h in 1..self.len - 1 {
                if self.get_pos(h) == new_pos {
                    bite_idx = h;
                    break;
                }
            }
            if bite_idx > 0 {
                // bite_idx より後ろの体節を盤面に餌として戻す。
                for p in bite_idx + 1..self.len {
                    let pos = self.get_pos(p);
                    self.f[pos as usize] = self.c[p];
                    clear_bit(&mut self.body_bits, pos);
                    // エラーカウントも巻き戻す。
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

    /// 頭から target_color の餌マスまでの最短コストを BFS で計算します。
    /// - 自分の体は壁。
    /// - target_color 以外の餌マスも壁扱い (誤って食べないため)。
    /// - 1 手のコストを 10 とスケールしているのは、評価関数で別の項と整数のまま
    ///   重み付け加算するための単位合わせです。
    fn cost_to_target(&self, input: &Input, target_color: u8, ctx: &mut BfsContext) -> i32 {
        // 世代カウンタを進めて、dist 配列を論理的にリセット。
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

            // ゴール判定: 「目標色のマス」に到達したらその距離を返す。
            // 特定座標ではなく色で判定するため、最寄りの目標色マスへの距離が自然に得られます。
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
                    // 自分の体は通れない。
                    if !get_bit(&self.body_bits, v) {
                        // 違う色の餌は障害物扱い (誤食回避)。
                        let is_wrong_food =
                            self.f[v as usize] != 0 && self.f[v as usize] != target_color;
                        if is_wrong_food {
                            continue;
                        }

                        // 未訪問 or より良い距離なら更新。
                        // ※ 通常 BFS では「未訪問のみ」で十分ですが、ここではコストが
                        //   定数 10 なので「dist[v] > d + 10」の比較は実質的に未訪問判定と等価。
                        //   gen 不一致を未訪問として扱うのが本質です。
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
        // 到達不能 (詰み)。十分大きな値を返してこの状態を選ばれにくくする。
        25000
    }

    /// 状態の評価値を計算します。小さいほど良い。
    /// 構成要素:
    ///   - turn: 既に消費した手数 (短い方が良い)
    ///   - error_count (e): 誤った色を食べた回数。重いペナルティ。
    ///   - 残り体節数 (M - len): 多いほど不利。
    ///   - cost: 次に食べたい色までの最短距離 (BFS)。
    fn evaluate(&self, input: &Input, bfs_ctx: &mut BfsContext, panic_mode: bool) -> i64 {
        // +1 しているのは「エラー 0 でも 0 にならないようにする」ためのオフセット。
        // (e*e のペナルティが 0 に潰れるのを防ぐ)
        let e = self.error_count + 1;

        // 完成状態 (全体節が揃った): turn の小ささとエラーの少なさだけで評価。
        if self.len == input.M {
            return self.turn as i64 + 10000 * e as i64;
        }

        // 基本コスト: 経過ターン + (エラーペナルティ + 残り長さペナルティ) × 10000。
        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));
        let target_color = input.d[self.len] as u8;

        // 次に食べたい色までの BFS コスト。
        let cost = self.cost_to_target(input, target_color, bfs_ctx) as i64;

        // 既存エラーへのヒューリスティックなペナルティ重み。
        // 通常時は重く (32500)、パニックモードや到達不能時は緩く (5000) して
        // 「妥協してでも先に進む」状態を許容します。
        let mut penalty_weight = 32500;
        if panic_mode || cost >= 20000 {
            penalty_weight = 5000;
        }

        // e^2 系のペナルティ。エラーが増えるほど急激に悪化させる。
        let heuristic_error_penalty = (penalty_weight * e * e * 2 / 3) as i64;

        base + heuristic_error_penalty + cost
    }
}

fn main() {
    let start_time = Instant::now();

    let mut sc = Scanner::new();
    let input = parse_input(&mut sc);

    let mut tree = MoveTree::new();
    let mut bfs_ctx = BfsContext::new();

    // 初期状態を作りビームに投入。
    let initial_state = State::new(&input, &mut tree, &mut bfs_ctx);
    let mut best_score: i64 = initial_state.score;
    let mut best_tree_id: u32 = initial_state.tree_node_id;
    let mut best_state = initial_state.clone();

    let mut beam = vec![initial_state];
    // 次世代ビーム (使い回しのために main 内で確保)。
    let mut next_beam: Vec<State> = Vec::with_capacity(16384);

    // ソート用 indices もループ外で確保し、毎ターンの確保コストを排除。
    let mut indices: Vec<usize> = Vec::with_capacity(16384);

    let mut current_beam_width: usize = 300;

    // 重複排除用のハッシュテーブル代わり。世代カウンタ方式で「クリア不要」。
    // キーは「頭の位置 << 8 | 体長」を 16bit に詰めています (64K 分の配列)。
    let mut seen_generation = vec![0u32; 65536];
    let mut current_generation = 0u32;

    // ===== ビームサーチ本体 =====
    while !beam.is_empty() {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        if elapsed_ms >= TIME_LIMIT_MS {
            break;
        }

        current_generation += 1;

        // 残り時間に応じてビーム幅を動的に調整。
        // 終盤ほど幅を狭めて 1 ターン当たりの計算量を減らします。
        let remaining_time = TIME_LIMIT_MS.saturating_sub(elapsed_ms);
        current_beam_width = if remaining_time < 50 {
            30
        } else if remaining_time < 100 {
            100
        } else if remaining_time < 200 {
            200
        } else if remaining_time < 500 {
            500
        } else if remaining_time < 1000 {
            2000
        } else {
            4000
        };

        // 残り 150ms を切ったら「妥協モード」: エラーペナルティを下げて
        // 完成優先に切り替え、未完成での失格を回避。
        let panic_mode = remaining_time < 150;

        next_beam.clear();

        // === 現在ビームの各状態から 4 方向に展開 ===
        for state in &beam {
            // 既に完成済み & スコア最良 (誤食 0) なら更新だけして展開しない。
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
                // dummy_id: apply 内では new_tree_id を後で上書きするので一旦親 ID を渡す。
                let dummy_id = state.tree_node_id;
                if next_state.apply(dir, &input, dummy_id, &mut bfs_ctx, panic_mode) {
                    // apply 成功時にだけ MoveTree にノードを追加。
                    // (失敗手で木を肥大化させないため)
                    let child_tree_id = tree.add_child(state.tree_node_id, dir as u8);
                    next_state.tree_node_id = child_tree_id;
                    next_beam.push(next_state);
                }
            }
        }

        if next_beam.is_empty() {
            break;
        }

        // === ビーム選別: スコア下位 current_beam_width 件を選ぶ ===
        // 全ソートはコストが高いので、まず select_nth_unstable で
        // 「上位 margin_width 件」だけを切り出してから厳密ソートします (k 選択)。
        // margin_width はビーム幅の 2 倍にしておき、後段の重複排除で件数が
        // 減ることを見込んだ余裕分です。
        indices.clear();
        indices.extend(0..next_beam.len());

        let margin_width = (current_beam_width * 2).min(next_beam.len());

        if margin_width < next_beam.len() {
            indices.select_nth_unstable_by_key(margin_width, |&i| next_beam[i].score);
            indices.truncate(margin_width);
        }

        // 切り出した範囲だけを厳密ソート。
        indices.sort_unstable_by_key(|&i| next_beam[i].score);

        beam.clear();

        // === 重複排除しつつビームに詰める ===
        // 「頭の位置 + 体長」が同じ状態は似通っているとみなして 1 つだけ採用。
        // 厳密な重複排除ではなく多様性確保のためのヒューリスティック。
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

        // ビームの先頭 (= 最良スコア) でベスト更新を試みる。
        if !beam.is_empty() && beam[0].score < best_score {
            best_score = beam[0].score;
            best_tree_id = beam[0].tree_node_id;
            best_state = beam[0].clone();
        }
    }

    // ===== 後処理: 体長が M に届いていない場合の救済 =====
    // ビームサーチで完成に至らなかった場合、最近傍の餌をとにかく食べに行って
    // 長さ M を強制的に達成させます。色は問わない (= 誤食ペナルティを受け入れる)。
    let mut final_state = best_state.clone();
    let mut final_tree_id = best_tree_id;

    while final_state.len < input.M {
        // ローカル BFS。BfsContext を流用してもよいですが、parent 配列が必要なので別に用意。
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

        // 最も近い「色問わずの」餌を探索。
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
                    // 自分の体は通れない (Uターンも含めて壁扱い)。
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

        // どこにも餌が無い / 到達できない: これ以上どうしようもないので諦める。
        if target_food == 255 {
            break;
        }

        // BFS の parent_dir 配列から経路を逆順復元。
        let mut path = Vec::new();
        let mut curr = target_food;
        while curr != start {
            path.push(parent_dir[curr as usize]);
            curr = parent_pos[curr as usize];
        }
        path.reverse();

        // 復元した経路を 1 手ずつ適用し、MoveTree にも追記。
        for dir in path {
            let next_tree_id = tree.add_child(final_tree_id, dir);
            final_state.apply(
                dir as usize,
                &input,
                next_tree_id,
                &mut bfs_ctx,
                true, // panic_mode
            );
            final_tree_id = next_tree_id;
        }
    }

    // ===== 最終出力 =====
    // MoveTree から最終ノードまでの経路を時系列順に取り出して 1 行ずつ出力。
    let history = tree.reconstruct(final_tree_id);
    for &dir in &history {
        println!("{}", DIR_CHARS[dir as usize]);
    }
}
