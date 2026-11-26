//! What the host's graphics adapter is, logged once when the device is made:
//! its vendor and device ids, its name, and whether it shares memory with the
//! processor (integrated graphics, such as Intel Arc and Xe iGPUs, which use
//! the system's own memory). On integrated graphics that sets the memory
//! defaults (`integrated`, `ram_share`): the caches keep less of the PC's RAM
//! empty there. The log says which path a run took.

use windows::Win32::Graphics::Direct3D12::{
    D3D12_FEATURE_ARCHITECTURE1, D3D12_FEATURE_DATA_ARCHITECTURE1, ID3D12Device,
};

static INTEGRATED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Whether the host adapter shares the system's memory with the processor
/// (integrated graphics). Set by [`log`], which runs as the device is made;
/// false until then.
pub(crate) fn integrated() -> bool {
    INTEGRATED.get().copied().unwrap_or(false)
}

/// The installed RAM divided by `unit`, on integrated graphics: what a cache
/// may keep empty there, since the card's memory is the PC's own. `None` on a
/// card with memory of its own, whose empty blocks cost the PC nothing.
pub(crate) fn ram_share(unit: u64) -> Option<u64> {
    share_of(integrated(), total_ram(), unit)
}

fn total_ram() -> Option<u64> {
    static TOTAL: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *TOTAL.get_or_init(crate::host::total_physical_ram)
}

fn share_of(integrated: bool, total: Option<u64>, unit: u64) -> Option<u64> {
    if !integrated {
        return None;
    }
    total.map(|total| total / unit.max(1))
}

#[cfg(test)]
mod share_tests {
    use super::share_of;

    #[test]
    fn only_integrated_graphics_are_limited_by_the_pc_ram() {
        assert_eq!(share_of(false, Some(32 << 30), 32), None);
        assert_eq!(share_of(true, Some(32 << 30), 32), Some(1 << 30));
        assert_eq!(share_of(true, None, 32), None);
    }
}

/// PCI vendor ids.
pub(crate) const VENDOR_AMD: u32 = 0x1002;
pub(crate) const VENDOR_INTEL: u32 = 0x8086;

/// The adapter a device is on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Identity {
    pub(crate) vendor: u32,
    pub(crate) device: u32,
    pub(crate) name: String,
    /// The card's memory is the system's (integrated graphics).
    pub(crate) uma: bool,
    /// A tile-based renderer (mobile-style GPUs; none of the desktop cards).
    pub(crate) tile_based: bool,
}

impl Identity {
    pub(crate) fn is_intel(&self) -> bool {
        self.vendor == VENDOR_INTEL
    }
}

/// The adapter `device` is on, or `None` if DXGI or the driver does not say.
pub(crate) fn identity(device: &ID3D12Device) -> Option<Identity> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIAdapter1, IDXGIFactory4,
    };
    // SAFETY: plain queries on a device we hold; out pointers are ours.
    unsafe {
        let desc = CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0))
            .and_then(|f| f.EnumAdapterByLuid::<IDXGIAdapter1>(device.GetAdapterLuid()))
            .and_then(|a| a.GetDesc1())
            .ok()?;
        let mut arch = D3D12_FEATURE_DATA_ARCHITECTURE1::default();
        let (uma, tile_based) = if device
            .CheckFeatureSupport(
                D3D12_FEATURE_ARCHITECTURE1,
                (&raw mut arch).cast(),
                size_of_val(&arch) as u32,
            )
            .is_ok()
        {
            (arch.UMA.as_bool(), arch.TileBasedRenderer.as_bool())
        } else {
            (false, false)
        };
        let end = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        Some(Identity {
            vendor: desc.VendorId,
            device: desc.DeviceId,
            name: String::from_utf16_lossy(&desc.Description[..end]),
            uma,
            tile_based,
        })
    }
}

/// Logs [`identity`] once for the run.
pub(crate) fn log(device: &ID3D12Device) {
    match identity(device) {
        Some(id) => {
            let _ = INTEGRATED.set(id.uma);
            tracing::info!(
            name = %id.name,
            vendor = format_args!("{:#06x}", id.vendor),
            device_id = format_args!("{:#06x}", id.device),
            uma = id.uma,
            tile_based = id.tile_based,
            intel = id.is_intel(),
            "host adapter"
            );
        }
        None => tracing::warn!("host adapter not identified (DXGI or the driver did not say)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intel_is_recognised_by_its_pci_vendor_id() {
        let mut id = Identity {
            vendor: VENDOR_INTEL,
            device: 0,
            name: String::new(),
            uma: true,
            tile_based: false,
        };
        assert!(id.is_intel());
        id.vendor = VENDOR_AMD;
        assert!(!id.is_intel());
    }
}
