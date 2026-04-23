# Ouroboros Solver Design Notes

## 問題の見方

この問題は、snake の移動問題であると同時に、色列の編集問題でもある。

- 食事: 末尾に色を追加
- 噛みちぎり: prefix を残して suffix を盤面上の餌へ変換

したがって、snake を完成品として扱うのではなく、

- 正しい prefix を本体に保持する
- 不要または再編集したい suffix を bite で外に出す
- 必要なら盤面上の餌として再利用する

という「列編集器」として扱う。

---

## スコアの捉え方

最終スコア:

`T + 10000 * (E + 2 * (M - k))`

- `T`: 軽い
- `E`: 重い
- `M-k`: さらに重い

設計上の含意:

- 移動距離最適化より、色列の品質が重要
- 長さ不足は非常に重いので、食べること自体の価値も高い
- ただし、間違った色を抱え続けるのも危険

---

## 全体方針

毎ターン、次の二択を行う。

1. 局所的に bite が有利なら bite を使う
2. そうでなければ通常モードで餌を取りに行く

固定作業場は使わない。
作業場は「場所」ではなく「局所形状」と考える。

---

## B1: bite 候補生成

### 候補生成方法

図形テンプレをベタ書きするのではなく、頭から深さ 1〜4 の合法手列を列挙し、最終手で self-collision が発生するものだけ bite 候補とする。

利点:

- 実装が素直
- 2x2, 3x2 的な局所ループを自然に含められる
- テンプレ実装ミスを減らせる

### 足切り条件

以下を満たさない候補は除外する。

- `lcp_after < lcp_now`
- `new_len < 5`
- `move_len > 4`
- `bad_removed + ordered_match + color_match == 0`

ここで:

- `lcp_now`: 現在の色列と target の最長共通 prefix 長
- `lcp_after`: bite 後の色列と target の最長共通 prefix 長

---

## B1: bite 候補評価

bite 候補に対して以下を計算する。

### 1. `bad_removed`

切られる suffix のうち、現在の index で target と不一致な色の個数。

`bad_removed = sum(cur[i] != target[i]) for i in cut_from..len`

初版では重み付きにしない。

### 2. `ordered_match`

切られた suffix をそのまま順に食べ直したとき、target の続きと何個一致するか。

bite 後の長さを `k2` として、

- `suffix[0]` vs `target[k2]`
- `suffix[1]` vs `target[k2+1]`
- ...

の一致数を数える。

### 3. `color_match`

切られた suffix に含まれる色が、target の次の `W=8` 個にどれだけ必要かを見る。

- target の次 `W` 個
- suffix の先頭 `W` 個程度

の multiset の共通数を取る。

### 4. `removed_len`

切られる長さ。補助的にのみ使用。

### 5. `move_len`

bite 実行までの手数。深さ 4 以下に絞っているので重くしすぎない。

### 確定評価式

`score =`

- `+ 1000000 * (lcp_after - lcp_now)`
- `+ 12000   * bad_removed`
- `+ 4000    * ordered_match`
- `+ 1500    * color_match`
- `+ 100     * removed_len`
- `- 100     * move_len`
- `- 3000    * max(0, 6 - new_len)`

意味:

- prefix を壊さないことが最優先
- 今邪魔な suffix を消す価値を強く評価
- そのまま順に再利用できる suffix を高く評価
- 色素材として有用な suffix も少し評価
- 短すぎる蛇は避ける

---

## B2: bite モード採用条件

毎ターン bite 候補は検討するが、採用は限定する。

### 採用条件

- best bite が存在する
- `best.score > BITE_THRESHOLD`
- `lcp_after >= lcp_now`
- `new_len >= 5`
- さらに以下のどれかを満たす
  - `bad_removed >= 1`
  - `ordered_match >= 2`
  - `color_match >= 3`

### 初期値

- `BITE_THRESHOLD = 5000`
- 長さが長い場合は閾値を下げてもよい
  - 例: `len >= 18` なら半減

---

## 通常モード

通常モードでは BFS で経路を作り、候補選択は Greedy にする。

### 理由

- 盤面は最大 16x16 で BFS 1 回は軽い
- 全餌に対して個別 BFS は不要
- 頭から 1 回 BFS して、全餌の距離を見るだけでよい

### 通常モード手順

1. 頭から BFS 1 回
2. 到達可能な餌を列挙
3. `target[len]` 色の餌があるなら、その中で最大スコアを選ぶ
4. なければ全餌から最大スコアを選ぶ
5. その最短路の最初の 1 手を打つ

### 確定餌評価式

`food_score =`

- `+ 20000`
- `+ 10000 * [c == target[k]]`
- `+ 3000  * [k+1 < M and c == target[k+1]]`
- `+ 1000  * future_count(c)`
- `- 120   * dist`

ここで:

- `k`: 現在長
- `c`: 餌の色
- `future_count(c)`: `target[k..min(M, k+8))` に色 `c` が何個あるか

補足:

- 食べること自体に大きな価値がある (`M-k` を 1 減らす)
- 次に欲しい色は最優先
- 次の次や近未来に必要な色も少し評価
- 距離罰は軽め

---

## データ構造

### 基本方針

高速化も考慮して、snake は `Vec` ベースで持つ。
`VecDeque` は今回の「prefix を残して truncate する」操作と相性が悪い。

### 推奨型

- `type Pos = u8`
- `type Color = u8`

盤面最大 16x16 = 256 マスなので、1 次元 index `0..255` に圧縮できる。

### State

- `body: Vec<Pos>`      // head -> tail
- `colors: Vec<Color>`  // head -> tail
- `food: [Color; 256]`

補助配列:

- `occupied: [bool; 256]`
- `dist: [i16; 256]`
- `first_move: [u8; 256]`

### なぜ `u8` を使うか

`usize` より命令レベルで速いからではなく、キャッシュ効率のため。

- `Vec<(usize, usize)>` は重い
- `Vec<u8>` の 1 次元座標はかなり軽い

ただし、ループ添字や長さ管理は `usize` のままでよい。

---

## 状態更新

### 1ターンの順序

問題文通りに以下の順で処理する。

1. 移動
2. 食事
3. 噛みちぎり

### bite 後に注意すること

切られた suffix `(h+1..old_len-1)` の各マスに、対応する色の餌を盤面へ戻す。
これを忘れると解法が壊れる。

---

## BFS

通常モードでは bite を考えず、occupied cell は通れないものとして扱う。

- 頭から 1 回 BFS
- 到達可能な各餌に対して `dist` を利用
- `first_move` を持って最初の一手を復元

盤面サイズ的に BFS は十分軽い。

---

## 実装優先順位

1. 状態構造
2. 1ターンシミュレータ
3. BFS
4. 通常モード餌選択
5. 局所 bite 候補生成
6. bite 評価と採用条件
7. 時間制限 (`~1990ms`) 打ち切り

---

## 初版でやらないこと

- 固定作業場
- 狙った body index への精密衝突制御
- 盤面全域の大域的 bite 探索
- 吐いた餌の長期的な厳密再取得計画
- 尻尾 1 個だけを遠回りして切る専用操作

初版は「近くで安全に bite できるときだけ使う」を徹底する。
