import argparse
import json
import os
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor

# ==========================================
# 設定
# ==========================================
# ※ご自身の環境に合わせてバイナリ名を調整してください
SOLVER_CMD = ["./target/release/ahc063"]
VIS_CMD = ["./tools/target/release/vis"]

IN_DIR = "tools/in"
OUT_DIR = "tools/out"
BASELINE_FILE = "baseline.json"
# ==========================================

# ターミナル出力用のカラーコード
GREEN = "\033[92m"
RED = "\033[91m"
RESET = "\033[0m"


def get_params(filename):
    """入力ファイルからN, M, Cを読み取る"""
    filepath = os.path.join(IN_DIR, filename)
    try:
        with open(filepath, "r") as f:
            n, m, c = map(int, f.readline().split())
            return n, m, c
    except Exception:
        return 0, 0, 0


def process_case(args_tuple):
    filename, loop_count = args_tuple
    in_path = os.path.join(IN_DIR, filename)
    out_path = os.path.join(OUT_DIR, filename)

    scores = []
    for _ in range(loop_count):
        # 1. ソルバーの実行
        with open(in_path, "r") as fin, open(out_path, "w") as fout:
            subprocess.run(SOLVER_CMD, stdin=fin, stdout=fout)

        # 2. ビジュアライザ（vis）を実行してスコアを取得
        res = subprocess.run(
            [VIS_CMD[0], in_path, out_path], capture_output=True, text=True
        )

        score = 0
        for line in res.stdout.split("\n"):
            if line.startswith("Score ="):
                try:
                    score = int(line.split("=")[1].strip())
                except ValueError:
                    pass
                break
        scores.append(score)

    n, m, c = get_params(filename)
    return filename, scores, n, m, c


def format_cell(score, baseline, col_width):
    """スコアとベースラインのDiffを計算して色付きの文字列を生成する"""
    if baseline is None:
        return f"{score:<{col_width}}"

    diff = score - baseline
    pct = (diff / baseline * 100) if baseline > 0 else 0.0

    diff_str = f"({diff:+d}, {pct:+.1f}%)"
    full_str = f"{score} {diff_str}"
    pad = max(0, col_width - len(full_str))

    if diff < 0:
        return f"{GREEN}{full_str}{RESET}" + " " * pad
    elif diff > 0:
        return f"{RED}{full_str}{RESET}" + " " * pad
    else:
        return f"{full_str}" + " " * pad


def main():
    parser = argparse.ArgumentParser(description="AHC Runner & Evaluator")
    parser.add_argument(
        "-s",
        "--save",
        action="store_true",
        help="実行結果(Best)をベースラインとして保存する",
    )
    parser.add_argument(
        "-c",
        "--compare",
        action="store_true",
        help="保存されたベースラインとスコアを比較する",
    )
    parser.add_argument(
        "-q", "--sequential", action="store_true", help="逐次実行（時間が正確）"
    )
    parser.add_argument("-n", type=int, default=None, help="実行するファイル数の上限")
    parser.add_argument("-l", "--loop", type=int, default=1, help="各ケースの実行回数")
    args = parser.parse_args()

    os.makedirs(OUT_DIR, exist_ok=True)

    print("Building solver and visualizer...")
    subprocess.run(["cargo", "build", "--release"], check=True)
    subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "--manifest-path",
            "tools/Cargo.toml",
            "--bin",
            "vis",
        ],
        check=True,
    )

    input_files = [f for f in os.listdir(IN_DIR) if f.endswith(".txt")]
    input_files.sort()
    if args.n is not None:
        input_files = input_files[: args.n]

    baseline_scores = {}
    if args.compare:
        if os.path.exists(BASELINE_FILE):
            with open(BASELINE_FILE, "r") as f:
                baseline_scores = json.load(f)
            print(f"Loaded baseline from {BASELINE_FILE}")
        else:
            print("Warning: Baseline file not found. Running without comparison.")

    # 集計用変数
    totals = [0] * args.loop
    total_best = 0
    total_baseline = 0
    results_dict = {}

    start_time = time.time()

    print("\nRunning test cases...")

    # 動的レイアウトの計算
    col_width = 28 if args.compare else 10

    header_parts = [f"{'File':<10}"]
    if args.loop > 1:
        for i in range(args.loop):
            header_parts.append(f"{f'Case {i + 1}':<{col_width}}")
        header_parts.append(f"{'Best':<{col_width}}")
    else:
        header_parts.append(f"{'Score':<{col_width}}")

    header_parts.extend([f"{'N':<2}", f"{'M':<2}", f"{'C':<2}"])
    header_line = " | ".join(header_parts)

    print(header_line)
    print("-" * len(header_line))

    task_args = [(f, args.loop) for f in input_files]

    if args.sequential:
        results = map(process_case, task_args)
    else:
        with ThreadPoolExecutor() as executor:
            results = executor.map(process_case, task_args)

    # 各ケースの結果出力
    for filename, scores, n, m, c in results:
        best_score = min(scores)
        results_dict[filename] = best_score
        total_best += best_score

        for i in range(args.loop):
            totals[i] += scores[i]

        base = baseline_scores.get(filename) if args.compare else None
        if base is not None:
            total_baseline += base

        row_parts = [f"{filename:<10}"]

        if args.loop > 1:
            for i in range(args.loop):
                row_parts.append(format_cell(scores[i], base, col_width))
            row_parts.append(format_cell(best_score, base, col_width))
        else:
            row_parts.append(format_cell(scores[0], base, col_width))

        row_parts.extend([f"{n:<2}", f"{m:<2}", f"{c:<2}"])
        print(" | ".join(row_parts))

    print("-" * len(header_line))

    # Total行の出力
    base_for_total = total_baseline if (args.compare and total_baseline > 0) else None
    total_parts = [f"{'Total':<10}"]

    if args.loop > 1:
        for i in range(args.loop):
            total_parts.append(format_cell(totals[i], base_for_total, col_width))
        total_parts.append(format_cell(total_best, base_for_total, col_width))
    else:
        total_parts.append(format_cell(totals[0], base_for_total, col_width))

    total_parts.extend([f"{'-':<2}", f"{'-':<2}", f"{'-':<2}"])
    print(" | ".join(total_parts))

    elapsed = time.time() - start_time
    print(f"\nTime        : {elapsed:.2f} sec")

    if args.save:
        with open(BASELINE_FILE, "w") as f:
            json.dump(results_dict, f, indent=2)
        print(f"Saved best scores to {BASELINE_FILE} as new baseline.")


if __name__ == "__main__":
    main()
