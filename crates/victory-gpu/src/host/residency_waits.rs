//! Time spent making new memory resident and waiting for its mapping, by allocation kind.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// The allocation that requested the wait. Buffer helpers can serve several callers.
#[derive(Clone, Copy)]
pub(crate) enum AllocationKind {
    Texture,
    TexturePoolHeap,
    UploadBuffer,
    SharedBuffer,
    MappedCardBuffer,
    UavBuffer,
}

const KINDS: [(AllocationKind, &str); 6] = [
    (AllocationKind::Texture, "texture"),
    (AllocationKind::TexturePoolHeap, "texture pool heap"),
    (AllocationKind::UploadBuffer, "upload buffer"),
    (AllocationKind::SharedBuffer, "shared buffer"),
    (AllocationKind::MappedCardBuffer, "mapped card buffer"),
    (AllocationKind::UavBuffer, "device-local UAV buffer"),
];

#[derive(Clone, Copy)]
pub(super) enum Wait {
    Resident,
    Mapped,
}

struct Counter {
    count: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
}

impl Counter {
    const fn new() -> Self {
        Self { count: AtomicU64::new(0), total_us: AtomicU64::new(0), max_us: AtomicU64::new(0) }
    }

    fn add(&self, us: u64) -> u64 {
        self.total_us.fetch_add(us, Relaxed);
        self.max_us.fetch_max(us, Relaxed);
        self.count.fetch_add(1, Relaxed) + 1
    }

    fn values(&self) -> (u64, u64, u64) {
        (self.count.load(Relaxed), self.total_us.load(Relaxed), self.max_us.load(Relaxed))
    }
}

struct Counters {
    all: Counter,
    by_kind: [Counter; KINDS.len()],
}

impl Counters {
    const fn new() -> Self {
        Self { all: Counter::new(), by_kind: [const { Counter::new() }; KINDS.len()] }
    }
}

impl fmt::Display for Counters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (kind, name)) in KINDS.into_iter().enumerate() {
            if i != 0 {
                f.write_str(", ")?;
            }
            let (count, total_us, max_us) = self.by_kind[kind as usize].values();
            write!(f, "{name}(count={count} total_us={total_us} max_us={max_us})")?;
        }
        Ok(())
    }
}

impl Wait {
    fn add(self, groups: &[Counters; 2], kind: AllocationKind, us: u64) -> (&Counters, u64) {
        let group = &groups[self as usize];
        group.by_kind[kind as usize].add(us);
        (group, group.all.add(us))
    }

    fn should_report(self, n: u64) -> bool {
        match self {
            Self::Resident => n.is_multiple_of(1000),
            Self::Mapped => n == 1 || n.is_multiple_of(200),
        }
    }
}

pub(super) fn record(wait: Wait, kind: AllocationKind, us: u64) {
    static GROUPS: [Counters; 2] = [const { Counters::new() }; 2];
    let (group, n) = wait.add(&GROUPS, kind, us);
    if !wait.should_report(n) {
        return;
    }
    let (_, total_us, max_us) = group.all.values();
    let total_ms = total_us as f64 / 1000.0;
    let max_ms = max_us as f64 / 1000.0;
    match wait {
        Wait::Resident => tracing::info!(
            target: "victory_gpu::host",
            allocation_kinds = %group,
            "new memory waited for (VICTORY_WAIT_RESIDENT=0: not), since the start: {n} times, {total_ms:.1} ms, the longest {max_ms:.1} ms"
        ),
        Wait::Mapped => tracing::info!(
            target: "victory_gpu::host",
            allocation_kinds = %group,
            "new card memory waited for until it is mapped (VICTORY_WAIT_RESIDENT=0: not), since the start: {n} times, {total_ms:.1} ms, the longest {max_ms:.1} ms"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_waits_and_allocation_kinds() {
        use AllocationKind::{SharedBuffer, Texture, UploadBuffer};
        let groups = [Counters::new(), Counters::new()];
        Wait::Resident.add(&groups, Texture, 10);
        Wait::Resident.add(&groups, Texture, 30);
        let (resident, n) = Wait::Resident.add(&groups, SharedBuffer, 7);
        assert_eq!(n, 3);
        let (mapped, n) = Wait::Mapped.add(&groups, SharedBuffer, 7);
        assert_eq!(n, 1);
        assert_eq!(resident.all.values(), (3, 47, 30));
        assert_eq!(resident.by_kind[Texture as usize].values(), (2, 40, 30));
        assert_eq!(resident.by_kind[SharedBuffer as usize].values(), (1, 7, 7));
        assert_eq!(resident.by_kind[UploadBuffer as usize].values(), (0, 0, 0));
        assert_eq!(mapped.all.values(), (1, 7, 7));
        assert_eq!(mapped.by_kind[Texture as usize].values(), (0, 0, 0));
        assert_eq!(mapped.by_kind[SharedBuffer as usize].values(), (1, 7, 7));
        let report = resident.to_string();
        assert!(report.contains("texture(count=2 total_us=40 max_us=30)"));
        assert!(report.contains("shared buffer(count=1 total_us=7 max_us=7)"));
    }

    #[test]
    fn keeps_the_existing_report_intervals() {
        let resident: Vec<_> = (1..=2001).filter(|&n| Wait::Resident.should_report(n)).collect();
        let mapped: Vec<_> = (1..=401).filter(|&n| Wait::Mapped.should_report(n)).collect();
        assert_eq!(resident, [1000, 2000]);
        assert_eq!(mapped, [1, 200, 400]);
    }
}
