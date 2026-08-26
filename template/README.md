# AtCoder-Heuristic

短期AHC向けRust探索テンプレート集です。標準ライブラリだけで、問題側では主に `State`・遷移・差分評価・`apply` を書けば探索を始められるようにしています。

使い始める手順は、(1) このcrate一式を作業用にコピー、(2) `templates/*.rs` の一つを `src/main.rs` へコピー、(3) TODOの問題固有部分を書く、です。AtCoder提出時は使う `src/` moduleを同じ提出ファイルへ `mod` としてinlineし、冒頭の `use atcoder_heuristic::...` をそのlocal moduleへの`use`に置き換えてください。

スコア・評価値はすべて「大きいほど良い」です。通常は余裕を見て `1900ms` を使います。

## Quick Start

### Greedy

[`templates/greedy.rs`](templates/greedy.rs) をコピーし、初期解と改善だけを問題に合わせます。`Timer` と `Random`、current/bestは既に骨格にあります。

### Simulated Annealing

```rust
let result = anneal(
    initial,
    AnnealConfig::new(1900, start_temp, end_temp),
    |state| state.score,
    |state, random| {
        let mv = propose_move(state, random);
        let diff = exact_diff(state, &mv); // apply後 score - 現score
        (mv, diff)
    },
    |state, mv| apply_move(state, mv),
);
```

SAは候補ごとにStateをcloneしません。`propose`で変更箇所だけを見てdiffを計算し、受理されたときだけ`apply`します。best更新時だけStateがcloneされます。[`templates/annealing.rs`](templates/annealing.rs) と [`examples/tsp.rs`](examples/tsp.rs) を出発点にしてください。

### Beam Search

```rust
let result = beam_search(initial, turns, BeamConfig::new(1900), |state, turn, out| {
    for action in actions(state, turn) {
        let (next, absolute_eval) = transition(state, action);
        out.push((next, absolute_eval, action));
    }
});
```

`out`へは所有する `(next_state, absolute_eval, action)` をpushします。Node、親ポインタ、上位K選択、復元、幅調整はライブラリ側です。rootのevalはAPI上`0`です。時間切れまたは行き止まりでは最後に完成した深さを返すため、`actions.len()`は`turns`未満になり得ます。

Beamは候補すべての回答履歴を保持しません。`select_nth_unstable_by`で上位Kを選んでから残存候補だけNode化し、親ポインタを逆順に辿ります。幅は各turnの `turn_start.elapsed() / expanded_states` をEMA（既定は過去90%、今回10%）で平滑化し、残時間・残turnから安全率90%、変更幅±20%で更新します。

### Beam → Annealing

Beamの`result.state`をSAの`initial`へ渡すだけです。完成済みの [`templates/hybrid.rs`](templates/hybrid.rs) はGreedy → Annealingの最小骨格ですが、同じ位置をBeamの初期構造に置き換えられます。

## Components

- `Random`: 速いxorshift。`usize(a..b)`、`f64()`、`bool()`。
- `Timer`: `progress()`は常に`0.0..=1.0`（0msは`1.0`）。
- `anneal`: maximize SA、候補State cloneなし。
- `beam_search`: Stateに`Clone`不要の親ポインタBeam。
- `BitSet<const WORDS: usize>`: `u64`/`u128`を超える小さな固定盤面用。小さい場合は生の整数を優先します。
- `Zobrist`: `update(hash, position, old, new)`でO(1)差分hash。hash衝突は可能なので、完全な重複除去が必要ならhash一致後にStateも比較してください。

## Candidate list

毎回1000候補を重く評価しません。前計算・安い指標・問題構造で候補を絞ってから、少数だけ正確な差分評価をします。TSP exampleは都市ごとの近傍都市上位K、Light Gridは静的照射価値 → 新規coverage数 → 局所diffの順です。candidate list自体は問題依存なので共通抽象化していません。

## Examples

- [`examples/tsp.rs`](examples/tsp.rs): 近傍candidate listを使う2-opt。差分は張り替わる2辺（old/new計4辺）だけを見て、採用時だけ区間反転します。
- [`examples/light_grid.rs`](examples/light_grid.rs): `N <= 16`の照明配置。照射ray前計算、`BitSet<4>`、coverage count、隣接ペナルティ、候補絞り込み、局所差分を示します。

## Validation

通常の検証は `cargo test --all-targets` です。Random/Timer/SA/Beam/BitSet/Zobristに加え、TSPとLight Gridの差分評価・apply後の集計整合性も確認します。

性能sanityは環境依存のためignoredです。`cargo test --release --lib -- --ignored --nocapture` で、top-Kの全sort/選択比較と動的Beam幅のdiagnosticsを表示します。速度倍率や厳密な実行時間にはassertせず、選抜集合・幅制約・計測値の健全性だけを検証します。

標準ライブラリの[`Instant`](https://doc.rust-lang.org/std/time/struct.Instant.html)と[`select_nth_unstable_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.select_nth_unstable_by)だけを使用しています。
