#![allow(nonstandard_style)]
#![allow(unused_assignments)]

use std::io::{BufRead, stdin};
use std::time::Instant;

// =============================================
// Scanner & Macros
// =============================================
// 競技プログラミング用の高速な入出力読み込みモジュール。
// std::io::StdinLock を用いてバッファリングし、文字列変換のオーバーヘッドを最小化する。
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

// read_value, inputマクロ: C++の cin のように直感的に変数を読み込むためのマクロ
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
/// 余裕を持って 1990ms に設定している。
const TIME_LIMIT_MS: u64 = 1990;

/// 4方向の (di, dj) ベクトル。インデックス順は U, D, L, R に対応。
const DIJ: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// DIJ と同じインデックス順の出力用文字。
const DIR_CHARS: [char; 4] = ['U', 'D', 'L', 'R'];

#[derive(Clone, Debug)]
pub struct Input {
    pub N: usize,           // 盤面サイズ (最大16)
    pub M: usize,           // 目標とするヘビの最終的な長さ (最大256)
    pub C: usize,           // 餌の種類(色数)
    pub d: Vec<usize>,      // d[i] = i番目の体節が持つべき目標の色
    pub f: Vec<Vec<usize>>, // 初期盤面の餌の配置

    // -------------------------------------------------------
    // 【事前計算済み隣接リスト】
    // 毎ターンの探索(BFSや移動判定)で、「盤外にはみ出さないか」のif文や、
    // 2次元座標(i, j)から1次元インデックスへの変換計算を省略するためのキャッシュ。
    // -------------------------------------------------------
    /// adj[pos][k] = 1次元座標 `pos` から k 番目の有効な移動先の1次元座標
    pub adj: [[u8; 4]; 256],
    /// adj_len[pos] = 座標 `pos` から移動できる有効なマスの数 (角なら2、端なら3、中央なら4)
    pub adj_len: [u8; 256],
    /// adj_dir[pos][k] = adj[pos][k] へ移動するための方向インデックス (0=U, 1=D, 2=L, 3=R)
    pub adj_dir: [[u8; 4]; 256],
}

pub fn parse_input<R: BufRead>(sc: &mut Scanner<R>) -> Input {
    input! { sc, N: usize, M: usize, C: usize, d: [usize; M], f: [[usize; N]; N] }

    let mut adj = [[0u8; 4]; 256];
    let mut adj_len = [0u8; 256];
    let mut adj_dir = [[0u8; 4]; 256];

    // 全マスの全方向について、盤内に収まる移動先だけを事前計算して登録する
    for row in 0..N {
        for col in 0..N {
            let pos = (row * 16 + col) as u8; // N<=16なので、16進法のように1バイトに収める
            let mut k = 0u8;
            for (dir, &(di, dj)) in DIJ.iter().enumerate() {
                let ni = row as isize + di;
                let nj = col as isize + dj;
                // ここで盤外チェックを済ませてしまうため、探索ループ内ではチェック不要になる
                if ni >= 0 && ni < N as isize && nj >= 0 && nj < N as isize {
                    adj[pos as usize][k as usize] = (ni * 16 + nj) as u8;
                    adj_dir[pos as usize][k as usize] = dir as u8;
                    k += 1;
                }
            }
            adj_len[pos as usize] = k;
        }
    }

    Input {
        N,
        M,
        C,
        d,
        f,
        adj,
        adj_len,
        adj_dir,
    }
}

// ---------------------------------------------------------
// Bitboard Utils
// ---------------------------------------------------------
// 16x16=256マスを、64bit整数4つ(256bit)で管理する。
// 配列で bool を持つよりメモリが少なく、コピーも高速。
// 主に「ヘビの胴体があるか」の衝突判定にO(1)で利用する。

/// pos の位置のビットを立てる (1にする)
#[inline(always)]
fn set_bit(bits: &mut [u64; 4], pos: u8) {
    bits[(pos >> 6) as usize] |= 1 << (pos & 63);
}

/// pos の位置のビットを落とす (0にする)
#[inline(always)]
fn clear_bit(bits: &mut [u64; 4], pos: u8) {
    bits[(pos >> 6) as usize] &= !(1 << (pos & 63));
}

/// pos の位置のビットが立っているか確認する
#[inline(always)]
fn get_bit(bits: &[u64; 4], pos: u8) -> bool {
    (bits[(pos >> 6) as usize] >> (pos & 63)) & 1 != 0
}

// ---------------------------------------------------------
// Move Tree
// ---------------------------------------------------------
// ビームサーチの各状態(State)が「自分がどういう経路で来たか」のVecを持つと、
// 状態をcloneするたびにVecのヒープ確保とコピーが走り激遅になる。
// そこで、履歴はすべてグローバルな木構造(MoveTree)に集約し、
// Stateは「自分が木のどのノード(ID)にいるか」だけを保持する。
struct MoveTree {
    parent: Vec<u32>, // 親ノードのID
    dir: Vec<u8>,     // 親からこのノードに来るために選んだ方向
}

impl MoveTree {
    fn new() -> Self {
        Self {
            parent: Vec::with_capacity(1 << 20), // 約100万ノード分を事前確保
            dir: Vec::with_capacity(1 << 20),
        }
    }

    // 初期状態用の根ノードを追加
    #[inline]
    fn add_root(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(u32::MAX); // u32::MAX を「親なし(ルート)」の目印とする
        self.dir.push(0);
        id
    }

    // 子ノードを追加し、そのIDを返す
    #[inline]
    fn add_child(&mut self, parent_id: u32, d: u8) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(parent_id);
        self.dir.push(d);
        id
    }

    // 最終ノードのIDから親をたどり、最初から最後までの移動方向のリストを復元する
    fn reconstruct(&self, mut node_id: u32) -> Vec<u8> {
        let mut dirs = Vec::new();
        while node_id != u32::MAX {
            let p = self.parent[node_id as usize];
            if p != u32::MAX {
                dirs.push(self.dir[node_id as usize]);
            }
            node_id = p;
        }
        dirs.reverse(); // 葉から根へたどったので逆順にする
        dirs
    }
}

// ---------------------------------------------------------
// BFS Context
// ---------------------------------------------------------
// 毎ターン呼ばれるBFS(幅優先探索)のたびに dist 配列を `[0; 256]` で初期化すると遅い。
// 代わりに `current_gen` (現在の世代番号) を1増やし、
// 「gen[i] が current_gen と同じなら dist[i] は今回の探索で書き込まれた有効な値」
// と見なすことで、配列全体の初期化処理をO(1)でスキップする。
struct BfsContext {
    dist: [i32; 256],  // 距離を記録
    r#gen: [u32; 256], // 最後に書き込まれた世代。(`gen` はRustの予約語になり得るため `r#` をつける)
    current_gen: u32,  // 現在の探索の世代番号
    q: [u8; 256],      // BFS用のキュー。固定長配列を使うことでヒープ確保を避ける。
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
// ビームサーチの探索ノードとなる状態構造体。
// clone() の負荷を下げるため、Vecを持たずすべて固定長配列で構成。
#[derive(Clone)]
struct State {
    f: [u8; 256],        // 盤面の状態 (0=空, 1~C=餌の色)
    ij: [u8; 256],       // ヘビの体節の座標を保持する【リングバッファ】。
    c: [u8; 256],        // ヘビの体節の色。
    body_bits: [u64; 4], // ヘビの胴体が存在するマスのビットボード (衝突判定用)
    head_ptr: u8,        // リングバッファ `ij` の先頭（頭）を指すインデックス
    len: usize,          // ヘビの現在の長さ
    turn: usize,         // 経過ターン数（操作回数）
    score: i64,          // この状態の評価スコア（小さいほど良い）
    error_count: usize,  // 目標の色 `d` と違う色を食べてしまった回数（ペナルティ）
    tree_node_id: u32,   // MoveTree に記録されているこの状態のノードID
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
        // 初期状態のヘビは長さ5で (4,0), (3,0), (2,0), (1,0), (0,0) にいる
        for i in 0..5 {
            let pos = ((4 - i) * 16) as u8;
            ij[i] = pos;
            set_bit(&mut body_bits, pos);
        }

        let c = [1u8; 256]; // 初期の体節はすべて色1
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
        // 初期状態のスコアを計算
        state.score = state.evaluate(input, bfs_ctx, false);
        state
    }

    /// リングバッファ `ij` から、頭から `idx` 番目の体節の座標を取得する。
    /// u8 のオーバーフロー挙動 (wrapping_add) を利用して、自動的に 0~255 にループさせる。
    #[inline(always)]
    fn get_pos(&self, idx: usize) -> u8 {
        self.ij[self.head_ptr.wrapping_add(idx as u8) as usize]
    }

    /// 指定した方向 `dir` にヘビを1歩進める。
    /// 移動不可（壁、直後の体節へのUターン）なら `false` を返す。
    fn apply(
        &mut self,
        dir: usize,
        input: &Input,
        new_tree_id: u32,
        bfs_ctx: &mut BfsContext,
        panic_mode: bool,
    ) -> bool {
        let head_pos = self.get_pos(0);

        // --- 移動先の座標を決定 ---
        let adj_len = input.adj_len[head_pos as usize] as usize;
        let new_pos = {
            let mut found = None;
            // 事前計算リストから、指定された方向 `dir` が有効か探す
            for k in 0..adj_len {
                if input.adj_dir[head_pos as usize][k] as usize == dir {
                    found = Some(input.adj[head_pos as usize][k]);
                    break;
                }
            }
            match found {
                Some(p) => p,
                None => return false, // 盤外への移動なので失敗
            }
        };

        // 首（1つ後ろの体節）の位置と一致するならUターンなので失敗
        if self.len > 1 && new_pos == self.get_pos(1) {
            return false;
        }

        let eaten_color = self.f[new_pos as usize];

        // --- 移動と食事の処理 ---
        if eaten_color != 0 {
            // 【餌を食べた場合】
            self.f[new_pos as usize] = 0; // 盤面から餌を消す

            // リングバッファのポインタを1つ前にずらし、新しい頭を配置
            // （しっぽの位置はそのままなので、結果的に長さが1伸びる）
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            self.c[self.len] = eaten_color; // 食べた色を記録

            // 目標の色と違っていればエラーを加算
            if input.d[self.len] != eaten_color as usize {
                self.error_count += 1;
            }
            self.len += 1;
            set_bit(&mut self.body_bits, new_pos);
        } else {
            // 【通常の移動（餌なし）】
            // しっぽを消す
            let tail_pos = self.get_pos(self.len - 1);
            clear_bit(&mut self.body_bits, tail_pos);

            // 頭を進める
            self.head_ptr = self.head_ptr.wrapping_sub(1);
            self.ij[self.head_ptr as usize] = new_pos;
            set_bit(&mut self.body_bits, new_pos);
        }

        // --- 噛みちぎり判定 ---
        // 頭の移動先に、自分の胴体（ビットボードで判定）がある場合
        if self.len >= 3 && get_bit(&self.body_bits, new_pos) {
            let mut bite_idx = 0;
            // 頭(0)としっぽ(len-1)以外で衝突した体節のインデックスを探す
            for h in 1..self.len - 1 {
                if self.get_pos(h) == new_pos {
                    bite_idx = h;
                    break;
                }
            }
            if bite_idx > 0 {
                // 噛みちぎられた部分（bite_idx+1 以降）を盤面に餌として撒き直す
                for p in bite_idx + 1..self.len {
                    let pos = self.get_pos(p);
                    self.f[pos as usize] = self.c[p]; // 元々の色で餌を置く
                    clear_bit(&mut self.body_bits, pos);

                    // ばら撒いた部分がエラー色だったなら、エラーカウントを減らして相殺
                    if input.d[p] != self.c[p] as usize {
                        self.error_count -= 1;
                    }
                }
                // 長さを切り詰める
                self.len = bite_idx + 1;
            }
        }

        self.turn += 1;
        self.tree_node_id = new_tree_id;
        self.score = self.evaluate(input, bfs_ctx, panic_mode);
        true
    }

    /// 頭の現在位置から、次に食べるべき色 `target_color` の餌までの最短距離(手数)を BFS で計算する。
    fn cost_to_target(&self, input: &Input, target_color: u8, ctx: &mut BfsContext) -> i32 {
        ctx.current_gen += 1; // 世代を進めて配列初期化をスキップ
        let mut head = 0usize;
        let mut tail = 0usize;

        let start = self.get_pos(0);
        ctx.dist[start as usize] = 0;
        ctx.r#gen[start as usize] = ctx.current_gen;
        ctx.q[tail] = start;
        tail += 1;

        while head < tail {
            let u = ctx.q[head];
            head += 1;

            // 目標の色を見つけたら即座に距離を返す
            if self.f[u as usize] == target_color {
                return ctx.dist[u as usize];
            }

            let d = ctx.dist[u as usize];
            let adj_len = input.adj_len[u as usize] as usize;

            // 事前計算リストを使って、盤内の隣接マスのみをループ
            for k in 0..adj_len {
                let v = input.adj[u as usize][k];

                // 自分の体は通れない (壁扱い)
                if get_bit(&self.body_bits, v) {
                    continue;
                }

                // 違う色の餌は障害物として扱い、踏まないようにする
                let fv = self.f[v as usize];
                if fv != 0 && fv != target_color {
                    continue;
                }

                // 未訪問（世代が違う）、またはより短い距離で到達できるなら更新
                if ctx.r#gen[v as usize] != ctx.current_gen || ctx.dist[v as usize] > d + 10 {
                    ctx.r#gen[v as usize] = ctx.current_gen;
                    ctx.dist[v as usize] = d + 10; // コストを10単位で管理
                    ctx.q[tail] = v;
                    tail += 1;
                }
            }
        }
        // 到達不能(完全に囲まれている、または目標の餌が無い)場合は大きなペナルティ値を返す
        25000
    }

    /// この状態の評価値(スコア)を計算する。小さいほど優秀。
    fn evaluate(&self, input: &Input, bfs_ctx: &mut BfsContext, panic_mode: bool) -> i64 {
        // e = 0 のときにペナルティ項が0に潰れないように +1 しておく
        let e = self.error_count + 1;

        // すべての目標の長さを満たしているなら、問題文の絶対スコアをそのまま返す
        if self.len == input.M {
            return self.turn as i64 + 10000 * e as i64;
        }

        // 基本スコア： 現在のターン数 + 10000 * (エラー数 + 2 * 不足している長さ)
        let base = self.turn as i64 + 10000 * (e as i64 + 2 * (input.M as i64 - self.len as i64));

        let target_color = input.d[self.len] as u8;
        // 次のターゲットまでの距離(コスト)
        let cost = self.cost_to_target(input, target_color, bfs_ctx) as i64;

        // エラーを犯すことへのヒューリスティックな追加ペナルティ。
        // パニックモード（時間切れ間近）や、到達不能(cost >= 20000)に陥った場合は
        // ペナルティの重みを下げて、「違う色を食べてでも無理やり進む（妥協する）」ことを許容する。
        let mut penalty_weight = 32500;
        if panic_mode || cost >= 20000 {
            penalty_weight = 5000;
        }

        // エラー数(e)の2乗に比例してペナルティを重くする
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

    let initial_state = State::new(&input, &mut tree, &mut bfs_ctx);
    let mut best_score: i64 = initial_state.score;
    let mut best_tree_id: u32 = initial_state.tree_node_id;
    let mut best_state = initial_state.clone();

    // beam: 現在の世代の有望な状態のリスト
    let mut beam = vec![initial_state];
    // next_beam: 次のターンの状態を一時的に溜め込むリスト（容量確保で再利用）
    let mut next_beam: Vec<State> = Vec::with_capacity(16384);
    // ソート用の中間配列
    let mut indices: Vec<usize> = Vec::with_capacity(16384);

    let mut current_beam_width: usize = 300;

    // 似たような状態を排除（多様性確保）するためのハッシュテーブル代わり
    let mut seen_generation = vec![0u32; 65536];
    let mut current_generation = 0u32;

    // --- ビームサーチ本体 ---
    while !beam.is_empty() {
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        if elapsed_ms >= TIME_LIMIT_MS {
            break; // 制限時間を超えたら探索打ち切り
        }

        current_generation += 1;

        // --- 動的ビーム幅調整 ---
        // 残り時間が少なくなるにつれてビーム幅を狭くし、時間内に処理を間に合わせる。
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

        // 残り時間が極端に少ない場合は、ペナルティ評価を落とすパニックモードに入る
        let panic_mode = remaining_time < 150;

        next_beam.clear();

        // 現在のビーム内のすべての状態から、4方向に展開する
        for state in &beam {
            // もしすでに目標長さMに到達し、かつエラーが無い（すべて目的の色を食べた）なら、
            // これ以上探索してもスコアは改善しないため、ベストを更新して展開を打ち切る。
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
                // applyが成功（壁にぶつからない等）した場合のみ次世代に追加
                if next_state.apply(dir, &input, dummy_id, &mut bfs_ctx, panic_mode) {
                    let child_tree_id = tree.add_child(state.tree_node_id, dir as u8);
                    next_state.tree_node_id = child_tree_id;
                    next_beam.push(next_state);
                }
            }
        }

        if next_beam.is_empty() {
            break; // 動ける状態が一つも無くなったら終了
        }

        indices.clear();
        indices.extend(0..next_beam.len());

        let margin_width = (current_beam_width * 2).min(next_beam.len());

        // 全てをソートすると遅いため、上位 `margin_width` 個だけを部分ソートして抽出する
        if margin_width < next_beam.len() {
            indices.select_nth_unstable_by_key(margin_width, |&i| next_beam[i].score);
            indices.truncate(margin_width);
        }

        // 切り出した上位陣の中で厳密にソート
        indices.sort_unstable_by_key(|&i| next_beam[i].score);

        beam.clear();

        // 重複排除処理
        for &i in &indices {
            let state = &next_beam[i];
            // 状態の同一視キー: 「頭の位置 (8bit) + ヘビの長さ (8bit)」の計16bit。
            // これが一致する状態は「似ている」とみなし、スコアが良い最初の1つだけをビームに残す。
            let key = (state.get_pos(0) as usize) << 8 | state.len;
            if seen_generation[key] != current_generation {
                seen_generation[key] = current_generation;
                beam.push(state.clone());
                // 指定したビーム幅に達したら補充終了
                if beam.len() == current_beam_width {
                    break;
                }
            }
        }

        // 毎ターン、ビームの先頭(一番良い状態)でグローバルベストを更新する
        if !beam.is_empty() && beam[0].score < best_score {
            best_score = beam[0].score;
            best_tree_id = beam[0].tree_node_id;
            best_state = beam[0].clone();
        }
    }

    // ===== 後処理: 体長が M に届いていない場合の救済 =====
    // タイムアップ等でビームサーチが完了しなかった場合、
    // 未完成のままではシステムテストで大きく減点されるか不正解になる。
    // そのため、手近な餌を「色を無視して」ひたすら食べて長さMに到達させる。
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

        // ローカルでの BFS 探索。今回は色は問わず、何らかの餌がある最も近いマスを探す。
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

                // 自分の体は通れない (Uターンも含めて壁扱い)。
                if !get_bit(&final_state.body_bits, v) && dist[v as usize] > d + 1 {
                    dist[v as usize] = d + 1;
                    parent_pos[v as usize] = u;
                    parent_dir[v as usize] = dir;
                    q[tail] = v;
                    tail += 1;
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
                true, // panic_mode を true にして評価を妥協
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
