//! 查找用的内存缓存（站主 2026-11-27 定，`decisions/0022`）：按文件内容记住每页的文字、
//! PDFium 字框、是否像扫描页、Vision 结果。同一份文件再查不用重新取字和识别；只在本进程里，
//! 总量有上限，满了先丢最久没用的文件。

use std::any::Any;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::sync::{Arc, Mutex};

use crate::Error;

/// 大约 64 MiB：千页左右的识别结果，或几十本书的文字。
const BUDGET: usize = 64 << 20;

/// 文件内容的指纹：长度加 SipHash，进程内比较用。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Fingerprint(usize, u64);

impl Fingerprint {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(bytes);
        Self(bytes.len(), hasher.finish())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Key {
    /// 一页的文字（PDFKit 原文、补过的连字、引擎存的识别原文、PDFium 参照）。
    Page(u32),
    /// PDFium 的字和字形框。
    Layer(u32),
    /// 页面是不是近乎全白以外的扫描页、整页图。
    Scanned(u32),
    /// 查找用的 Vision 结果，按识别语言分开。
    Vision(u32, Vec<String>),
}

type Value = Arc<dyn Any + Send + Sync>;
/// 一份文件存下的结果：键 → （结果、大约多少字节）。
type Entries = HashMap<Key, (Value, usize)>;

#[derive(Default)]
struct Cache {
    /// 最近用过的在后面。
    files: Vec<(Fingerprint, Entries)>,
    cost: usize,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    files: Vec::new(),
    cost: 0,
});

/// 有就交回缓存的；没有就算一个存起来。`cost` 是大约占多少字节。算的时候不占锁。
pub(crate) fn get_or_insert<T: Any + Send + Sync>(
    file: Fingerprint,
    key: Key,
    make: impl FnOnce() -> Result<T, Error>,
    cost: impl FnOnce(&T) -> usize,
) -> Result<Arc<T>, Error> {
    if let Some(value) = lock().get(file, &key) {
        if let Ok(value) = value.downcast::<T>() {
            return Ok(value);
        }
    }
    let value = Arc::new(make()?);
    let size = cost(&value).max(64);
    lock().insert(file, key, value.clone(), size);
    Ok(value)
}

fn lock() -> std::sync::MutexGuard<'static, Cache> {
    // 缓存里只有算好的结果，别的线程中途出错也不会留下半截数据。
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Cache {
    fn get(&mut self, file: Fingerprint, key: &Key) -> Option<Value> {
        let at = self.files.iter().position(|(seen, _)| *seen == file)?;
        let entry = self.files.remove(at);
        let value = entry.1.get(key).map(|(value, _)| value.clone());
        self.files.push(entry);
        value
    }

    fn insert(&mut self, file: Fingerprint, key: Key, value: Value, size: usize) {
        if size > BUDGET / 4 {
            return;
        }
        let at = match self.files.iter().position(|(seen, _)| *seen == file) {
            Some(at) => at,
            None => {
                self.files.push((file, HashMap::new()));
                self.files.len() - 1
            }
        };
        let mut entry = self.files.remove(at);
        if let Some((_, old)) = entry.1.insert(key, (value, size)) {
            self.cost -= old;
        }
        self.cost += size;
        self.files.push(entry);
        // 超了就丢最久没用的文件；只剩这一份还超，就把它也清掉重来。
        while self.cost > BUDGET && self.files.len() > 1 {
            let (_, dropped) = self.files.remove(0);
            self.cost -= dropped.values().map(|(_, size)| size).sum::<usize>();
        }
        if self.cost > BUDGET {
            let (_, pages) = self.files.last_mut().expect("刚放进去的文件");
            pages.clear();
            self.cost = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BUDGET, Cache, Fingerprint, Key};
    use std::sync::Arc;

    #[test]
    fn keeps_recent_files_within_budget() {
        let mut cache = Cache::default();
        let (a, b) = (Fingerprint::of(b"a"), Fingerprint::of(b"b"));
        assert_ne!(a, b);
        cache.insert(a, Key::Page(1), Arc::new(1u8), BUDGET / 8);
        cache.insert(b, Key::Page(1), Arc::new(2u8), BUDGET / 8);
        assert!(cache.get(a, &Key::Page(1)).is_some());
        for page in 2..9 {
            cache.insert(b, Key::Page(page), Arc::new(0u8), BUDGET / 8);
        }
        // a 最久没用，先被丢；b 留着。
        assert!(cache.get(a, &Key::Page(1)).is_none());
        assert!(cache.get(b, &Key::Page(8)).is_some());
        assert!(cache.cost <= BUDGET);
        // 单个结果太大不存。
        cache.insert(a, Key::Scanned(1), Arc::new(true), BUDGET);
        assert!(cache.get(a, &Key::Scanned(1)).is_none());
    }
}
