//! What the host's graphics adapter is, logged once when the device is made:
//! its vendor and device ids, its name, and whether it shares memory with the
//! processor (integrated graphics, such as Intel Arc and Xe iGPUs, which use
//! the system's own memory). Nothing branches on it yet; the log says which
//! path a run took, so a measurement can be read against the hardware.

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
