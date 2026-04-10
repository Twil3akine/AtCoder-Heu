use std::collections::{HashSet, VecDeque};
use std::io::{self, Read};

#[derive(Clone)]
struct State {
    n: usize,
    board: Vec<Vec<u8>>, // 0: 空, 1~C: 餌の色
    snake_pos: VecDeque<(usize, usize)>,
    snake_colors: Vec<u8>,
}

impl State {
    fn new(n: usize, board: Vec<Vec<u8>>) -> Self {
        let mut snake_pos = VecDeque::new();
        for i in 0..5 {
            snake_pos.push_back((4 - i, 0));
        }
        let snake_colors = vec![1; 5];
        Self {
            n,
            board,
            snake_pos,
            snake_colors,
        }
    }

    // 行動を適用し、成功すれば true を返す
    fn apply_action(&mut self, action: char) -> bool {
        let (r, c) = self.snake_pos[0];
        let (nr, nc) = match action {
            'U' => {
                if r == 0 {
                    return false;
                }
                (r - 1, c)
            }
            'D' => {
                if r + 1 == self.n {
                    return false;
                }
                (r + 1, c)
            }
            'L' => {
                if c == 0 {
                    return false;
                }
                (r, c - 1)
            }
            'R' => {
                if c + 1 == self.n {
                    return false;
                }
                (r, c + 1)
            }
            _ => return false,
        };

        // Uターン禁止 (長さは必ず2以上ある)
        if (nr, nc) == self.snake_pos[1] {
            return false;
        }

        // 1. 移動先が餌かどうか
        let food_color = self.board[nr][nc];

        // ヘビの新しい座標列をシミュレート
        let mut next_pos = self.snake_pos.clone();
        next_pos.push_front((nr, nc));

        // 修正後
        if food_color > 0 {
            self.board[nr][nc] = 0;
            self.snake_colors.push(food_color);
        } else {
            // 食べない：長さを保つため末尾を消す
            next_pos.pop_back();
        }

        // 2. 噛みちぎり判定
        // 移動後の頭 (nr, nc) が、移動後の胴体 (index: 1 から len - 2) と一致するか
        let len = next_pos.len();
        let mut bite_idx = None;
        for h in 1..=len.saturating_sub(2) {
            if next_pos[h] == (nr, nc) {
                bite_idx = Some(h);
                break;
            }
        }

        if let Some(h) = bite_idx {
            // 噛みちぎり発生
            // 残りのしっぽ側を盤面に餌として配置
            for p in (h + 1)..len {
                let (pr, pc) = next_pos[p];
                self.board[pr][pc] = self.snake_colors[p];
            }
            // ヘビの長さを h + 1 に縮める
            next_pos.truncate(h + 1);
            self.snake_colors.truncate(h + 1);
        }

        self.snake_pos = next_pos;
        true
    }
}

// 障害物（自分自身や他の餌）を避けて最短経路を探すBFS
fn bfs(state: &State, start: (usize, usize), goal: (usize, usize)) -> Option<Vec<char>> {
    let mut q = VecDeque::new();
    let mut visited = vec![vec![false; state.n]; state.n];
    let mut parent = vec![vec![None; state.n]; state.n];

    // 障害物として現在のヘビの胴体（尻尾以外）を登録
    let mut obstacles = HashSet::new();
    for i in 0..state.snake_pos.len() - 1 {
        obstacles.insert(state.snake_pos[i]);
    }

    q.push_back(start);
    visited[start.0][start.1] = true;

    let dirs = [('U', (!0, 0)), ('D', (1, 0)), ('L', (0, !0)), ('R', (0, 1))];

    while let Some((r, c)) = q.pop_front() {
        if (r, c) == goal {
            let mut path = Vec::new();
            let mut curr = goal;
            while curr != start {
                let (prev_r, prev_c, dir) = parent[curr.0][curr.1].unwrap();
                path.push(dir);
                curr = (prev_r, prev_c);
            }
            path.reverse();
            return Some(path);
        }

        for &(dir, (dr, dc)) in &dirs {
            let nr = r.wrapping_add(dr);
            let nc = c.wrapping_add(dc);

            if nr < state.n && nc < state.n && !visited[nr][nc] {
                // ゴール以外の餌は障害物とみなす（貪欲食い時のノイズ回避）
                if (nr, nc) != goal && state.board[nr][nc] > 0 {
                    continue;
                }
                if obstacles.contains(&(nr, nc)) {
                    continue;
                }

                visited[nr][nc] = true;
                parent[nr][nc] = Some((r, c, dir));
                q.push_back((nr, nc));
            }
        }
    }
    None
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut tokens = input.split_whitespace();

    let n: usize = tokens.next().unwrap().parse().unwrap();
    let m: usize = tokens.next().unwrap().parse().unwrap();
    let _c: usize = tokens.next().unwrap().parse().unwrap();

    let mut d = vec![0; m];
    for i in 0..m {
        d[i] = tokens.next().unwrap().parse().unwrap();
    }

    let mut board = vec![vec![0; n]; n];
    for i in 0..n {
        for j in 0..n {
            board[i][j] = tokens.next().unwrap().parse().unwrap();
        }
    }

    let mut state = State::new(n, board);
    let mut actions = Vec::new();

    // 1. ノイズを1つ食べて長さを6にする (適当に一番最初に見つけた餌を食べる)
    let head = state.snake_pos[0];
    let mut target_food = None;
    for i in 0..n {
        for j in 0..n {
            if state.board[i][j] > 0 {
                target_food = Some((i, j));
                break;
            }
        }
        if target_food.is_some() {
            break;
        }
    }

    if let Some(goal) = target_food {
        if let Some(path) = bfs(&state, head, goal) {
            for &a in &path {
                state.apply_action(a);
                actions.push(a);
            }
        }
    }

    // 2. 目標マス (2, 5) に餌を落とす
    // まず、下準備として目標マスの1つ下 (3, 5) へ向かう
    let head = state.snake_pos[0];
    if let Some(path) = bfs(&state, head, (3, 5)) {
        for &a in &path {
            state.apply_action(a);
            actions.push(a);
        }

        // 定石の発動: U で (2, 5) に入り、 L, U, L, D, R で噛みちぎる
        let combo = ['U', 'L', 'U', 'L', 'D', 'R'];
        for &a in &combo {
            state.apply_action(a);
            actions.push(a);
        }
    } else {
        eprintln!("(3, 5) への経路が見つかりませんでした");
    }

    // 3. 結果の出力
    for a in actions {
        println!("{}", a);
    }
}
