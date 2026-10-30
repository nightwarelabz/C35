//! A readback is mapped only if THIS submission encoded its copy.
//! A callback completing between encode and submit must not cause a
//! skipped copy to be mapped a second time as if it were a fresh sample.
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Default)]
pub(crate) struct ReadbackGate(AtomicU8);

impl ReadbackGate {
    pub fn arm(&self) -> bool {
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn begin_mapping(&self) -> bool {
        self.0
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn complete(&self) {
        self.0.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_an_encoded_copy_can_start_a_mapping() {
        let gate = ReadbackGate::default();
        assert!(!gate.begin_mapping());
        assert!(gate.arm());
        assert!(!gate.arm());
        assert!(gate.begin_mapping());
        assert!(!gate.begin_mapping());
        assert!(!gate.arm());
        gate.complete();
        // Previous frame's callback completed AFTER this frame skipped
        // its copy: still no fresh mapping is allowed.
        assert!(!gate.begin_mapping());
        assert!(gate.arm());
        assert!(gate.begin_mapping());
    }
}
