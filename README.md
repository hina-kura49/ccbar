# ccbar

Claude Code の **5時間枠 / 7日枠の消費率** を macOS のメニューバーに常時表示し、枠が回復したら通知する小さな Rust 製ツールです。

```
5h 23% · 7d 41%
```

- Claude Code の status line が渡す JSON だけを使います（API 直叩き・OAuth トークンの読み取りはしません）。
- 常駐しません。書く側（`tap`）と読む側（`show`）が状態ファイルでつながるだけです。
- 依存は `serde` / `serde_json` / `jiff` のみ。バイナリは約 0.5MB です。

## インストール

```sh
cargo install --git https://github.com/hina-kura49/ccbar
```

## 設定

1. `~/.claude/settings.json` に追加します。

   ```json
   { "statusLine": { "type": "command", "command": "ccbar tap" } }
   ```

2. [SwiftBar](https://swiftbar.app) のプラグインフォルダに `ccbar.30s.sh` を置きます（30秒ごとに `ccbar show` を呼びます）。

## 仕組み

| コマンド | 役割 |
| --- | --- |
| `ccbar tap` | stdin の JSON から `rate_limits` を取り出し `~/Library/Caches/ccbar/state.json` に保存 |
| `ccbar show` | 保存した値と現在時刻から表示を作り、回復を通知（`notified.json` に記録） |

回復の通知は、消費率 50% 以上・回復から 10 分以内・同じ回復で未通知のときだけ出します。

## 注意

- 値が出るのは Pro / Max 契約のときだけです。枠ごとに欠けることがあります（回復した枠は JSON から落ちます）。
- Claude Code を閉じている間は値が更新されません（「最終更新」で確認できます）。

## ライセンス

MIT
