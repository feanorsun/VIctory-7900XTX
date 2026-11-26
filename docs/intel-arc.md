# Intel Arc B390 and an Intel mobile processor

Target: an integrated Intel Arc B390 (Xe3, Panther Lake class) on a mobile
Intel Core Ultra processor with hybrid cores (performance, efficiency and low-power
efficiency cores). The code was written and measured on one AMD machine (see
[findings.md](findings.md)). This note says what that means for Intel, what
this change does, and what still needs a run on the hardware. Nothing here has
been measured on Intel yet.

## What the code assumes today

| Area | Where | On Intel today |
|---|---|---|
| Indirect dispatch group cap (65,535 vs 2^20) | `indirect.rs` `widest()` | **65,535.** The 2^20 cap is only on for AMD (`VendorId == 0x1002`). See the first item below. |
| RDNA float rules (`VICTORY_RDNA_FLOAT`) | `host.rs` `set_host_rdna` | Off. Only on for waves of 32/64, which is AMD. Intel gets the generic float path, which is the default. |
| Widest wave | `host.rs` `set_host_max_wave` | Taken from the driver. Intel Xe2/Xe3 are expected to report 16 to 32 lanes; console wave64 programs are then translated as two 32-lane halves. Check the `host wave lanes` log line. |
| Fast/efficiency core pinning | `cores.rs` | Generic: uses the system's efficiency classes, so it applies to Intel hybrid cores as it does to Alder Lake. |
| Thread counts | `list/thin.rs` (8), `shadow.rs` `wide_threads` (cores/4, 2 to 6), `pipeline/make.rs` | Fixed or core-count based; not tuned for a mobile package's power limit. |
| Memory model | everywhere | Assumes a discrete card: uploads and copies into the card's memory. On an iGPU that memory is the system's own (see below). |

## What this change does

- **`crates/victory-gpu/src/hardware.rs`** logs the host adapter once at start:
  vendor, device id, name, UMA, tile-based. Look for the `host adapter` line in a
  run's log. Nothing branches on it.
- **`.cargo/config.toml`** builds Windows code for `x86-64-v3` (AVX2, FMA, BMI2,
  and the rest of that level). Intel Core Ultra and the AMD machine both have it.
  Float results do not change. It is not measured yet.
- `indirect::amd()` now compares against the shared `VENDOR_AMD` constant. Same
  behaviour.
- **Release profile** uses `lto = "thin"` and `codegen-units = 1`. Expected to
  be faster at run time, at the cost of longer release and diagnostics builds.
  Not measured.
- **In-place memory cap** (`VICTORY_INPLACE_MB`) defaults to 4 GiB, or an eighth
  of installed RAM where that is less. A 16 GB laptop now locks at most 2 GiB;
  machines with 32 GB or more are unchanged. Set the variable to override.
- **Shared memory, integrated graphics only** (the adapter reports UMA):
  - `VICTORY_INPLACE` defaults to 1. Windows of the title's private memory are
    placed for the GPU instead of copied, so the bytes are kept once instead of
    twice. A window the driver refuses still falls back to a copy. Upstream
    confirmed the approach with a probe only; it has not been run as a whole
    game on any machine. **This is the change most likely to show up as a
    visual or crash problem.** Revert with `VICTORY_INPLACE=0`.
  - `VICTORY_TEXTURE_ROOM_PERCENT` defaults to 70 instead of 90. Texture copies
    may fill 70 % of the card's budget, which here is the PC's memory, so about
    a fifth of 32 GB is left for the title and the system. Costs more uploads
    when textures are swapped. Set it to 90 to revert.

## What it does not do, and why

**1. Indirect dispatches above 65,535 groups on Intel (check this first).**
The cap was raised to 2^20 for AMD to fix patches that went blocky, because the
title dispatches more than 65,535 tile groups in one indirect call. On Intel the
cap stays at 65,535, so those dispatches are cut short. That may show as blocky
or missing patches on the Arc. Whether Intel's driver honours a larger count
is not known. Test it with the 2^20 cap on Intel, the same way as on AMD, and
compare a captured frame. Do not change the default until that is measured.

**2. Memory is shared on an iGPU (the largest gain available, not started).**
The adapter reports `UMA = true` on integrated graphics. Then the card and the
processor use the same physical memory, and the copies into card-only memory
(texture uploads, list buffers, descriptor mirrors) are work that is not needed.
Using host-visible heaps for that memory would remove copies and their waits. It
touches most of `victory-gpu`'s memory code, and it cannot be checked without the
machine, so it is left as the next step. The `hardware` log tells you whether
the adapter is UMA.

**3. Power.** On a mobile part the CPU and the Arc share one power budget. Busy
spin loops take power the GPU could use: `shadow.rs` (`spin_loop`), `watch.rs`
(`spin_loop`), and `indirect.rs` (test-only). Measure how often they run on
the laptop before changing them. `VICTORY_PROCESS_PRIORITY=normal` is worth a
test on battery as well as on mains.

**4. Thread counts for a hybrid mobile package.** The fixed defaults (eight list
threads, eight join parts) assume the desktop split of fast and slow cores. Try
`VICTORY_THIN_THREADS` and `VICTORY_JOIN_PARTS` at 4, 6 and 8 on the device.

## How to measure on the device

Build as the README says (`cargo build --locked --profile diag -p victory --features diagnostics`),
then run once and read the log:

```powershell
scripts\play.ps1 -Exe <your copy's executable>
```

From the log, check:

- `host adapter`: `intel = true`, `uma = true`, the device name.
- `host wave lanes`: the min and max the driver reports.
- `the thread is kept on the fast cores`: appears when the processor has two kinds of core.

Then compare the frame line (see [findings.md](findings.md)) at the same place in
the prologue: the default, `VICTORY_INDIRECT_WIDE=0`, and `VICTORY_THIN=1` (the
switch is off unless set to 1; `play.ps1` sets it unless `-Safe`). Report the
numbers with the adapter's name and driver version.

## Status

- Compiles for the Windows target (`cargo check` and clippy for `victory-gpu`).
  Clippy with `-D warnings` already fails on code this change does not touch, on
  the unmodified tree too; that is outside this change.
- Unit tests for `hardware.rs` are written; they have not been run, because they
  need Windows and the device.
- Not run on Intel hardware. None of the numbers above are Intel numbers.
