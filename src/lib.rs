//! ccbar の中核ロジック。副作用（ファイル・プロセス）は `store` に分けてある。

use jiff::{Timestamp, tz::TimeZone};
use serde::{Deserialize, Serialize};

pub mod store;

// ---- 仮置きの定数（一か所にまとめる） ----
/// 通知する最低の消費率（%）。これ未満の枠の回復では鳴らさない。
pub const NOTIFY_THRESHOLD_PERCENT: f64 = 50.0;
/// 回復からこの秒数を過ぎたら通知しない（スリープ明けの古い通知を防ぐ）。
pub const NOTIFY_MAX_DELAY_SECS: u64 = 10 * 60;
/// 枠の値が同じなら、この秒数以内は state.json を書き直さない（ディスク書き込みを減らす）。
pub const WRITE_SKIP_SECS: u64 = 60;

// ---- 型 ----

/// 入力: Claude Code が渡す JSON のうち、使う部分だけ。
#[derive(Deserialize)]
pub struct StatusInput {
    pub rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
pub struct RateLimits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
}

/// 入力と保存で共用。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub used_percentage: f64,
    pub resets_at: u64, // Unix 秒
}

/// 保存: state.json
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct State {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub updated_at: u64,
}

/// 保存: notified.json
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Notified {
    pub five_hour: Option<u64>,
    pub seven_day: Option<u64>,
}

// ---- 純粋関数 ----

/// 新しい入力にある枠だけを上書きし、無い枠は前回の値を残す。
pub fn merge(mut state: State, input: &StatusInput, now: u64) -> State {
    if let Some(rl) = &input.rate_limits {
        if let Some(w) = rl.five_hour {
            state.five_hour = Some(w);
        }
        if let Some(w) = rl.seven_day {
            state.seven_day = Some(w);
        }
    }
    state.updated_at = now;
    state
}

#[derive(Debug, PartialEq)]
pub enum Display {
    Unknown,
    Recovered,
    Used(u32),
}

/// 枠と現在時刻から表示値を決める。
pub fn window_display(w: Option<Window>, now: u64) -> Display {
    match w {
        None => Display::Unknown,
        Some(w) if now >= w.resets_at => Display::Recovered,
        Some(w) => Display::Used(w.used_percentage.round() as u32),
    }
}

pub fn percent_text(d: &Display) -> String {
    match d {
        Display::Unknown => "--".to_string(),
        Display::Recovered => "0%".to_string(),
        Display::Used(p) => format!("{p}%"),
    }
}

#[derive(Debug, PartialEq)]
pub enum NotifyDecision {
    /// 条件を満たさない（まだ回復していない、または処理済み）。何も書かない。
    Nothing,
    /// 通知は見送るが、この resets_at を処理済みとして記録する。
    RecordOnly,
    /// 通知して記録する。
    Notify,
}

/// この枠の回復を通知すべきか決める。
pub fn notify_decision(w: Option<Window>, notified: Option<u64>, now: u64) -> NotifyDecision {
    let Some(w) = w else {
        return NotifyDecision::Nothing;
    };
    if now < w.resets_at || notified == Some(w.resets_at) {
        return NotifyDecision::Nothing;
    }
    let high_enough = w.used_percentage >= NOTIFY_THRESHOLD_PERCENT;
    let fresh = now - w.resets_at <= NOTIFY_MAX_DELAY_SECS;
    if high_enough && fresh {
        NotifyDecision::Notify
    } else {
        NotifyDecision::RecordOnly
    }
}

/// 残り時間を「2時間13分」「45分」の形にする。
pub fn remaining_text(secs: u64) -> String {
    let mins = secs.div_ceil(60);
    let (h, m) = (mins / 60, mins % 60);
    if h > 0 {
        format!("{h}時間{m}分")
    } else {
        format!("{m}分")
    }
}

/// 回復時刻の表示。同じ日なら「14:00」、別の日なら「11/21 09:00」。
pub fn reset_time_text(resets_at: u64, now: u64) -> String {
    let tz = TimeZone::system();
    let to_zoned = |s: u64| Timestamp::from_second(s as i64).map(|t| t.to_zoned_datetime(tz.clone()));
    match (to_zoned(resets_at), to_zoned(now)) {
        (Ok(r), Ok(n)) if r.date() == n.date() => r.strftime("%H:%M").to_string(),
        (Ok(r), _) => r.strftime("%-m/%-d %H:%M").to_string(),
        _ => "--".to_string(),
    }
}

/// メニューバー用の一行。
pub fn bar_line(state: &State, now: u64) -> String {
    format!(
        "5h {} · 7d {}",
        percent_text(&window_display(state.five_hour, now)),
        percent_text(&window_display(state.seven_day, now))
    )
}

fn window_row(label: &str, w: Option<Window>, now: u64, with_remaining: bool) -> String {
    let d = window_display(w, now);
    let pct = percent_text(&d);
    match (d, w) {
        (Display::Used(_), Some(w)) => {
            let mut s = format!("{label}  {pct}  {}に回復", reset_time_text(w.resets_at, now));
            if with_remaining {
                s += &format!("（あと{}）", remaining_text(w.resets_at - now));
            }
            s
        }
        (Display::Recovered, _) => format!("{label}  {pct}  回復済み"),
        _ => format!("{label}  --"),
    }
}

/// SwiftBar 用の全出力。
pub fn render(state: &State, now: u64) -> String {
    let updated = if state.updated_at == 0 {
        "--".to_string()
    } else {
        Timestamp::from_second(state.updated_at as i64)
            .map(|t| t.to_zoned_datetime(TimeZone::system()).strftime("%H:%M").to_string())
            .unwrap_or_else(|_| "--".to_string())
    };
    format!(
        "{}\n---\n{}\n{}\n最終更新  {}\n",
        bar_line(state, now),
        window_row("5時間枠", state.five_hour, now, true),
        window_row("7日枠  ", state.seven_day, now, false),
        updated
    )
}


/// state.json を書き直す必要があるか。枠の値が変わらず、直近に書いたばかりなら不要。
pub fn needs_write(old: &State, new: &State) -> bool {
    old.five_hour != new.five_hour
        || old.seven_day != new.seven_day
        || new.updated_at.saturating_sub(old.updated_at) >= WRITE_SKIP_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(p: f64, r: u64) -> Window {
        Window { used_percentage: p, resets_at: r }
    }

    #[test]
    fn display_boundaries() {
        let win = Some(w(23.4, 1000));
        assert_eq!(window_display(None, 0), Display::Unknown);
        assert_eq!(window_display(win, 999), Display::Used(23));
        assert_eq!(window_display(win, 1000), Display::Recovered);
        assert_eq!(window_display(win, 1001), Display::Recovered);
        assert_eq!(window_display(Some(w(22.5, 1000)), 0), Display::Used(23));
    }

    #[test]
    fn merge_keeps_missing_windows() {
        let old = State { five_hour: Some(w(10.0, 5)), seven_day: Some(w(40.0, 9)), updated_at: 1 };
        let input: StatusInput =
            serde_json::from_str(r#"{"rate_limits":{"five_hour":{"used_percentage":20.0,"resets_at":7}},"x":1}"#)
                .unwrap();
        let s = merge(old, &input, 100);
        assert_eq!(s.five_hour, Some(w(20.0, 7)));
        assert_eq!(s.seven_day, Some(w(40.0, 9)));
        assert_eq!(s.updated_at, 100);
    }

    #[test]
    fn merge_without_rate_limits_only_bumps_time() {
        let old = State { five_hour: Some(w(10.0, 5)), seven_day: None, updated_at: 1 };
        let input: StatusInput = serde_json::from_str("{}").unwrap();
        let s = merge(old, &input, 50);
        assert_eq!(s.five_hour, Some(w(10.0, 5)));
        assert_eq!(s.updated_at, 50);
    }

    #[test]
    fn notify_rules() {
        let win = Some(w(60.0, 1000));
        // まだ回復していない
        assert_eq!(notify_decision(win, None, 999), NotifyDecision::Nothing);
        // 回復直後
        assert_eq!(notify_decision(win, None, 1000), NotifyDecision::Notify);
        // 10分ちょうどまでは通知、超えたら記録のみ
        assert_eq!(notify_decision(win, None, 1600), NotifyDecision::Notify);
        assert_eq!(notify_decision(win, None, 1601), NotifyDecision::RecordOnly);
        // 処理済み
        assert_eq!(notify_decision(win, Some(1000), 1000), NotifyDecision::Nothing);
        // 別の回復なら再び通知
        assert_eq!(notify_decision(win, Some(500), 1000), NotifyDecision::Notify);
        // しきい値未満は記録のみ
        assert_eq!(notify_decision(Some(w(49.9, 1000)), None, 1000), NotifyDecision::RecordOnly);
        assert_eq!(notify_decision(Some(w(50.0, 1000)), None, 1000), NotifyDecision::Notify);
        // 枠なし
        assert_eq!(notify_decision(None, None, 1000), NotifyDecision::Nothing);
    }

    #[test]
    fn bar_line_cases() {
        assert_eq!(bar_line(&State::default(), 0), "5h -- · 7d --");
        let s = State { five_hour: Some(w(23.0, 2000)), seven_day: Some(w(41.0, 100)), updated_at: 1 };
        assert_eq!(bar_line(&s, 1000), "5h 23% · 7d 0%");
    }

    #[test]
    fn write_skip() {
        let a = State { five_hour: Some(w(10.0, 5)), seven_day: None, updated_at: 100 };
        let same = State { updated_at: 130, ..State { five_hour: a.five_hour, seven_day: None, updated_at: 0 } };
        assert!(!needs_write(&a, &same));
        let later = State { updated_at: 160, ..State { five_hour: a.five_hour, seven_day: None, updated_at: 0 } };
        assert!(needs_write(&a, &later));
        let changed = State { five_hour: Some(w(11.0, 5)), seven_day: None, updated_at: 101 };
        assert!(needs_write(&a, &changed));
    }

    #[test]
    fn remaining() {
        assert_eq!(remaining_text(2 * 3600 + 13 * 60), "2時間13分");
        assert_eq!(remaining_text(45 * 60), "45分");
        assert_eq!(remaining_text(1), "1分");
    }
}
