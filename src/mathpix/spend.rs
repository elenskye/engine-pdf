//! 花钱的规矩（站主 2026-11-28 定，`decisions/0023`），所有用 engine-pdf 的 App 一样：
//!
//! - 一次超过 `ASK_ABOVE_USD`（$0.5）或算不出这一次要花多少，先问用户；否则直接用。
//! - 所有 App 合起来最近 24 小时花的，加上这一次超过 `DAY_LIMIT_USD`（$5），也先问。
//!
//! 账本是本机一个 JSON Lines 文件（`Ledger::Shared`，所有 App 共用），每次真正上传成功记一行：时间、App、
//! 页数、金额。模拟的传输不记。App 要看花了多少就读 `Ledger::entries`。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 一次超过这么多先问。
pub const ASK_ABOVE_USD: f64 = 0.5;
/// 最近 24 小时（所有 App 合计）加上这一次超过这么多先问。
pub const DAY_LIMIT_USD: f64 = 5.0;
/// 合计的时间窗。
pub const WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
/// 改共用账本位置的环境变量（命令行、验收用）。
pub const LEDGER_ENV: &str = "ENGINE_PDF_LEDGER";

/// 账本记在哪。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Ledger {
    /// 所有 App 共用：`ENGINE_PDF_LEDGER`，否则 `~/Library/Application Support/engine-pdf/mathpix-ledger.jsonl`。
    #[default]
    Shared,
    /// 指定的文件（App 的开发版、测试）。
    At(PathBuf),
    /// 不记、不读：只按单次的规矩问。
    Off,
}

/// 账本的一行：一次上传。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix 秒。
    pub at: u64,
    pub app: String,
    pub service: String,
    pub pages: u32,
    pub usd: f64,
}

/// 为什么要先问。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    /// 算不出这一次要花多少。
    UnknownCost,
    /// 这一次超过 `ASK_ABOVE_USD`。
    Over { usd: f64 },
    /// 最近 24 小时已花 `spent`，加上这一次超过 `DAY_LIMIT_USD`。
    Day { spent: f64, usd: f64 },
}

impl std::fmt::Display for Why {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCost => f.write_str("算不出这一次要花多少。"),
            Self::Over { usd } => write!(f, "这一次预计 ${usd:.3}，超过 ${ASK_ABOVE_USD}。"),
            Self::Day { spent, usd } => write!(
                f,
                "所有 App 最近 24 小时已花 ${spent:.3}，加上这一次 ${usd:.3} 超过 ${DAY_LIMIT_USD}。"
            ),
        }
    }
}

fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Ledger {
    /// 账本文件；`Off` 没有。
    pub fn path(&self) -> Option<PathBuf> {
        match self {
            Self::Shared => std::env::var_os(LEDGER_ENV)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| {
                        PathBuf::from(home)
                            .join("Library/Application Support/engine-pdf/mathpix-ledger.jsonl")
                    })
                }),
            Self::At(path) => Some(path.clone()),
            Self::Off => None,
        }
    }

    /// 记一次上传（追加一行；几个 App 同时写也不会写坏一行）。
    pub fn record(&self, entry: &Entry) -> std::io::Result<()> {
        let Some(path) = self.path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut line = serde_json::to_vec(entry).map_err(std::io::Error::other)?;
        line.push(b'\n');
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(&line)
    }

    /// `since` 以来的各行，按时间先后；读不了、写坏的行跳过。
    pub fn entries(&self, since: SystemTime) -> Vec<Entry> {
        let Some(text) = self.path().and_then(|p| std::fs::read_to_string(p).ok()) else {
            return vec![];
        };
        let since = unix(since);
        let mut out: Vec<Entry> = text
            .lines()
            .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
            .filter(|e| e.at >= since && e.usd.is_finite())
            .collect();
        out.sort_by_key(|e| e.at);
        out
    }

    /// `now` 之前 24 小时所有 App 花的。
    pub fn spent(&self, now: SystemTime) -> f64 {
        self.entries(now.checked_sub(WINDOW).unwrap_or(UNIX_EPOCH))
            .iter()
            .map(|e| e.usd.max(0.0))
            .sum()
    }

    /// 这一次（`usd`，算不出时 None）要不要先问用户、为什么。
    pub fn approval(&self, usd: Option<f64>) -> Option<Why> {
        self.approval_at(usd, SystemTime::now())
    }

    pub fn approval_at(&self, usd: Option<f64>, now: SystemTime) -> Option<Why> {
        let usd = match usd {
            Some(usd) if usd.is_finite() && usd >= 0.0 => usd,
            _ => return Some(Why::UnknownCost),
        };
        if usd > ASK_ABOVE_USD {
            return Some(Why::Over { usd });
        }
        let spent = self.spent(now);
        (spent + usd > DAY_LIMIT_USD).then_some(Why::Day { spent, usd })
    }
}

/// 一次上传的账本行（现在）。
pub(super) fn entry(app: &str, pages: u32, usd: f64) -> Entry {
    Entry {
        at: unix(SystemTime::now()),
        app: app.into(),
        service: super::SERVICE.into(),
        pages,
        usd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> Ledger {
        let dir =
            std::env::temp_dir().join(format!("engine-pdf-spend-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Ledger::At(dir.join("nested/ledger.jsonl"))
    }

    #[test]
    fn asks_above_half_a_dollar_or_when_the_cost_is_unknown() {
        let ledger = temp("once");
        assert_eq!(ledger.approval(Some(0.5)), None);
        assert_eq!(ledger.approval(Some(0.505)), Some(Why::Over { usd: 0.505 }));
        assert_eq!(ledger.approval(None), Some(Why::UnknownCost));
        assert_eq!(ledger.approval(Some(f64::NAN)), Some(Why::UnknownCost));
        assert_eq!(Ledger::Off.approval(Some(0.2)), None);
        assert_eq!(
            Ledger::Off.approval(Some(0.6)),
            Some(Why::Over { usd: 0.6 })
        );
    }

    #[test]
    fn the_last_day_of_every_app_counts_toward_five_dollars() {
        let ledger = temp("day");
        let now = SystemTime::now();
        let at = |ago: u64, app: &str, usd: f64| Entry {
            at: unix(now) - ago,
            app: app.into(),
            service: "Mathpix".into(),
            pages: 1,
            usd,
        };
        ledger.record(&at(25 * 3600, "Peck", 3.0)).unwrap(); // older than a day: not counted
        ledger.record(&at(3600, "Pomelo", 2.4)).unwrap();
        ledger.record(&at(60, "Pith", 2.4)).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(ledger.path().unwrap())
            .unwrap()
            .write_all(b"not json\n")
            .unwrap();
        assert!((ledger.spent(now) - 4.8).abs() < 1e-9);
        assert_eq!(ledger.approval_at(Some(0.2), now), None);
        let Some(Why::Day { spent, usd }) = ledger.approval_at(Some(0.25), now) else {
            panic!("要问")
        };
        assert!((spent - 4.8).abs() < 1e-9 && usd == 0.25);
        // a day later only what's newer counts
        assert_eq!(
            ledger.approval_at(
                Some(0.25),
                now + Duration::from_secs(3601 + 24 * 3600 - 60 - 1)
            ),
            None
        );
        let apps: Vec<String> = ledger
            .entries(UNIX_EPOCH)
            .into_iter()
            .map(|e| e.app)
            .collect();
        assert_eq!(apps, ["Peck", "Pomelo", "Pith"]);
        assert!(
            Ledger::At(PathBuf::from("/nonexistent/x.jsonl"))
                .entries(UNIX_EPOCH)
                .is_empty()
        );
    }

    #[test]
    fn the_shared_ledger_follows_the_environment() {
        assert!(Ledger::Shared.path().is_some());
        assert_eq!(Ledger::Off.path(), None);
        assert!(
            Why::Day {
                spent: 4.8,
                usd: 0.25
            }
            .to_string()
            .contains("最近 24 小时已花 $4.800")
        );
    }
}
