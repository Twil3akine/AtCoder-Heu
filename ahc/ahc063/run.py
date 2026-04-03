import os
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor

# ==========================================
# 設定
# ==========================================
# ※ バイナリ名は自分の環境に合わせて書き換えてください
SOLVER_CMD = ["./target/release/ahc063"]
VIS_CMD = ["./tools/target/release/vis"]

IN_DIR = "tools/in"
OUT_DIR = "tools/out"
# ==========================================

def process_case(filename):
    in_path = os.path.join(IN_DIR, filename)
    out_path = os.path.join(OUT_DIR, filename)

    # 1. ソルバーの実行
    with open(in_path, "r") as fin, open(out_path, "w") as fout:
        subprocess.run(SOLVER_CMD, stdin=fin, stdout=fout)

    # 2. ビジュアライザ（vis）を実行してスコアを取得
    res = subprocess.run([VIS_CMD[0], in_path, out_path], capture_output=True, text=True)
    
    score = 0
    # vis の出力から Score = XXX の行を探す
    for line in res.stdout.split('\n'):
        if line.startswith("Score ="):
            try:
                score = int(line.split("=")[1].strip())
            except ValueError:
                pass
            break
            
    return filename, score

def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    print("Building solver and visualizer...")
    subprocess.run(["cargo", "build", "--release"], check=True)
    # toolsディレクトリ内の vis もビルドする
    subprocess.run(["cargo", "build", "--release", "--manifest-path", "tools/Cargo.toml", "--bin", "vis"], check=True)

    input_files = [f for f in os.listdir(IN_DIR) if f.endswith(".txt")]
    input_files.sort()

    total_score = 0
    start_time = time.time()

    # スレッドプールで並列実行（手元のPCのコア数を活かして高速化）
    print("Running test cases...")
    with ThreadPoolExecutor() as executor:
        results = executor.map(process_case, input_files)

    for filename, score in results:
        print(f"{filename}: {score}")
        total_score += score

    elapsed = time.time() - start_time
    print("-" * 30)
    print(f"Total Score : {total_score}")
    print(f"Time        : {elapsed:.2f} sec")

if __name__ == "__main__":
    main()