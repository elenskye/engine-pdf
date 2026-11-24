use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 可在线程间共享的取消令牌。取消后，所有克隆均可读到取消状态。
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub(crate) fn check(&self) -> Result<(), crate::Error> {
        if self.is_cancelled() {
            return Err(crate::Error::Cancelled);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Cancel;

    #[test]
    fn cancellation_is_shared_across_threads() {
        let cancel = Cancel::new();
        assert!(!cancel.is_cancelled());
        let other = cancel.clone();
        std::thread::spawn(move || other.cancel()).join().unwrap();
        assert!(cancel.is_cancelled());
        assert!(!Cancel::new().is_cancelled());
    }
}
