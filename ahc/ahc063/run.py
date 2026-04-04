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


def process_case(filename):
    in_path = os.path.join(IN_DIR, filename)
    out_path = os.path.join(OUT_DIR, filename)

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

    n, m, c = get_params(filename)
    return filename, score, n, m, c


def main():
    parser = argparse.ArgumentParser(description="AHC Runner & Evaluator")
    parser.add_argument(
        "-s",
        "--save",
        action="store_true",
        help="今回の実行結果をベースラインとして保存する",
    )
    parser.add_argument(
        "-c",
        "--compare",
        action="store_true",
        help="保存されたベースラインとスコアを比較する",
    )
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

    baseline_scores = {}
    if args.compare:
        if os.path.exists(BASELINE_FILE):
            with open(BASELINE_FILE, "r") as f:
                baseline_scores = json.load(f)
            print(f"Loaded baseline from {BASELINE_FILE}")
        else:
            print("Warning: Baseline file not found. Running without comparison.")

    total_score = 0
    total_baseline_score = 0
    start_time = time.time()
    results_dict = {}

    print("\nRunning test cases...")
    print(f"{'File':<10} | {'Score':<10} | {'Diff':<12} | N  | M  | C")
    print("-" * 60)

    with ThreadPoolExecutor() as executor:
        results = executor.map(process_case, input_files)

    for filename, score, n, m, c in results:
        results_dict[filename] = score
        total_score += score

        diff_str = ""
        if args.compare and filename in baseline_scores:
            baseline = baseline_scores[filename]
            total_baseline_score += baseline
            diff = score - baseline
            if diff < 0:
                diff_str = f"{GREEN}{diff:<12}{RESET}"  # スコア減少（改善）
            elif diff > 0:
                diff_str = f"{RED}+{diff:<11}{RESET}"  # スコア増加（悪化）
            else:
                diff_str = f"{'0':<12}"
        else:
            diff_str = f"{'-':<12}"

        print(f"{filename:<10} | {score:<10} | {diff_str} | {n:<2} | {m:<2} | {c:<2}")

    elapsed = time.time() - start_time
    print("-" * 60)

    if args.compare and baseline_scores:
        total_diff = total_score - total_baseline_score
        color = GREEN if total_diff < 0 else RED if total_diff > 0 else RESET
        sign = "+" if total_diff > 0 else ""
        print(f"Total Score : {total_score} (Diff: {color}{sign}{total_diff}{RESET})")
    else:
        print(f"Total Score : {total_score}")

    print(f"Time        : {elapsed:.2f} sec")

    if args.save:
        with open(BASELINE_FILE, "w") as f:
            json.dump(results_dict, f, indent=2)
        print(f"\nSaved current scores to {BASELINE_FILE} as new baseline.")


if __name__ == "__main__":
    main()
