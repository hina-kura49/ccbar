//! 副作用のある部分（ファイルの読み書きと osascript）。薄く保つ。

use serde::{Deserialize, Serialize};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 読み込む最大バイト数。異常に大きい入力・ファイルでメモリを使わないための上限。
const MAX_BYTES: u64 = 256 * 1024;

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("CCBAR_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Library/Caches/ccbar")
}

/// 保存先を作る（自分だけが読み書きできる 0700）。シンボリックリンクなら使わない。
fn ensure_dir(dir: &Path) -> io::Result<()> {
    if let Ok(meta) = fs::symlink_metadata(dir) {
        if meta.file_type().is_symlink() {
            return Err(io::Error::other("保存先がシンボリックリンクです"));
        }
        return Ok(());
    }
    DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

/// stdin を上限つきで読む。
pub fn read_stdin_limited() -> io::Result<String> {
    let mut buf = String::new();
    io::stdin().take(MAX_BYTES).read_to_string(&mut buf)?;
    Ok(buf)
}

/// JSON ファイルを読む。無い・壊れている・大きすぎるときは既定値。
pub fn read_json<T: for<'de> Deserialize<'de> + Default>(name: &str) -> T {
    let Ok(f) = fs::File::open(data_dir().join(name)) else {
        return T::default();
    };
    let mut buf = Vec::new();
    if f.take(MAX_BYTES).read_to_end(&mut buf).is_err() {
        return T::default();
    }
    serde_json::from_slice(&buf).unwrap_or_default()
}

/// 一時ファイル（0600・既存なら失敗）に書いてから rename で差し替える。
pub fn write_json_atomic<T: Serialize>(name: &str, value: &T) -> io::Result<()> {
    let dir = data_dir();
    ensure_dir(&dir)?;
    let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(&serde_json::to_vec(value)?)?;
        fs::rename(&tmp, dir.join(name))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// 通知を出す。文字列は呼び出し側の固定文言だけを渡すこと（AppleScript に埋め込むため）。
pub fn notify(title: &str, message: &str) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        esc(message),
        esc(title)
    );
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
