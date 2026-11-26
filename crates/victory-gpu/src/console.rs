//! Which console the title runs on. Titles pick settings (resolution,
//! memory budgets) by it, so the user chooses; without a choice, by the PC's
//! memory ([`Console::default_for`]).

use std::sync::atomic::{AtomicBool, Ordering};

const GIB: usize = 1 << 30;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Console {
    #[default]
    SeriesX,
    SeriesS,
}

impl Console {
    /// Title memory in bytes: (GPU-optimal, standard). Series X in the
    /// advanced configuration; Series S has one range, counted as standard
    /// (7,776 MiB, the published console-mode figure).
    // shortcut: later system updates gave Series S titles more (amount not
    // published here); the title sizes nothing from this figure
    // (it goes by the device type), so update it if a title does.
    pub const fn title_memory(self) -> (usize, usize) {
        match self {
            Console::SeriesX => (10 * GIB, 7 * GIB / 2),
            Console::SeriesS => (0, 7_776 << 20),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Console::SeriesX => "Xbox Series X",
            Console::SeriesS => "Xbox Series S",
        }
    }

    /// The console when the user names none: Series S on a PC with 32 GiB of
    /// RAM or less, Series X otherwise (and where the RAM is not known).
    ///
    /// The Series X profile does not fit in 32 GB: a run of it on a PC with
    /// 32 GB shared between an integrated card and the processor had 130 MB
    /// of RAM free at its third frame and drew two frames a second. Series S
    /// has less title memory and a 1080p output.
    pub const fn default_for(installed_ram: Option<u64>) -> Console {
        match installed_ram {
            Some(ram) if ram <= 32 * GIB as u64 => Console::SeriesS,
            _ => Console::SeriesX,
        }
    }
}

/// The PC's installed RAM in bytes, for [`Console::default_for`].
#[cfg(windows)]
pub fn installed_ram() -> Option<u64> {
    crate::host::total_physical_ram()
}

static SERIES_S: AtomicBool = AtomicBool::new(false);

/// Set before the title starts.
pub fn set(console: Console) {
    SERIES_S.store(console == Console::SeriesS, Ordering::Relaxed);
}

pub fn current() -> Console {
    if SERIES_S.load(Ordering::Relaxed) {
        Console::SeriesS
    } else {
        Console::SeriesX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pc_with_32_gib_or_less_gets_series_s_by_default() {
        let gib = 1u64 << 30;
        assert_eq!(Console::default_for(Some(16 * gib)), Console::SeriesS);
        // A "32 GB" PC reports a little less than 32 GiB.
        assert_eq!(Console::default_for(Some(32 * gib - (400 << 20))), Console::SeriesS);
        assert_eq!(Console::default_for(Some(32 * gib)), Console::SeriesS);
        assert_eq!(Console::default_for(Some(64 * gib)), Console::SeriesX);
        assert_eq!(Console::default_for(None), Console::SeriesX);
    }
}
