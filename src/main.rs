//! ccbar: Claude Code の 5時間枠 / 7日枠の消費率をメニューバーに出す小さなツール。
//!
//! - `ccbar tap`  : status line の JSON を stdin から受け取り state.json に保存する（記録係）
//! - `ccbar show` : state.json と現在時刻から SwiftBar 用の出力を作り、必要なら通知する（解釈係）

use ccbar::store::{notify, now_secs, read_json, read_stdin_limited, write_json_atomic};
use ccbar::{
    NotifyDecision, Notified, State, StatusInput, bar_line, merge, needs_write, notify_decision,
    render,
};

fn tap() {
    let now = now_secs();
    let input = read_stdin_limited()
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<StatusInput>(&s).map_err(|e| e.to_string()));
    match input {
        Ok(input) => {
            let old: State = read_json("state.json");
            let new = merge(old.clone(), &input, now);
            if needs_write(&old, &new) && let Err(e) = write_json_atomic("state.json", &new) {
                eprintln!("ccbar: state.json を書けません: {e}");
            }
            println!("{}", bar_line(&new, now));
        }
        Err(e) => {
            eprintln!("ccbar: 入力を読めません: {e}");
            println!("ccbar: --");
        }
    }
}

fn show() {
    let now = now_secs();
    let state: State = read_json("state.json");
    print!("{}", render(&state, now));

    let mut notified: Notified = read_json("notified.json");
    let mut changed = false;
    for (label, w, rec) in [
        ("5時間枠", state.five_hour, &mut notified.five_hour),
        ("7日枠", state.seven_day, &mut notified.seven_day),
    ] {
        let decision = notify_decision(w, *rec, now);
        if decision == NotifyDecision::Nothing {
            continue;
        }
        if decision == NotifyDecision::Notify {
            notify("Claude Code", &format!("{label}が回復した"));
        }
        *rec = w.map(|w| w.resets_at);
        changed = true;
    }
    if changed && let Err(e) = write_json_atomic("notified.json", &notified) {
        eprintln!("ccbar: notified.json を書けません: {e}");
    }
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("tap") => tap(),
        Some("show") => show(),
        _ => eprintln!("使い方: ccbar tap | ccbar show"),
    }
    // どの場合も終了コード 0（status line を空欄にしないため）
}
