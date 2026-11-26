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
  run's log. On integrated graphics it also sets the memory defaults below.
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
  - Caches that keep memory empty for reuse are sized from installed RAM (32 GB
    as the example). Each keeps the smaller of its old limit and its share:
    - empty texture blocks: 1/32 of RAM, 1 GB (was 4 GB);
    - scratch chunks for command lists: 1/128 of RAM, 256 MB (was 1 GB);
    - idle command lists: one per 128 MB of RAM, 256 (was 512).

    The cost is re-creating those objects more often, which shows up as
    stutter when a scene changes quickly. Each limit is a guess and is not
    measured. Revert by setting the limits back in the code, or measure first.
  - Still large and not touched here: upload memory (about 4 GB in the notes)
    and the title's own memory (40 to 46 GB in total). Run with
    `VICTORY_MEMORY_MAP=1` to see the breakdown on the device.
  - The rest of the shared-memory path (one copy instead of two) is staged and
    gated in [shared-memory-plan.md](shared-memory-plan.md). Only stage 1 is done.

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

## The first run on such a PC (2026-11-26)

One run on a PC with 32 GB shared between an integrated Intel card and the
processor, of another fork's build (codec-d/cool_fox), with the Series X
profile: about 2 frames a second. Its log says why:

- `host memory pressure` at the third frame with `physical_free_mib=130`: the
  PC was out of RAM before the world loaded, so Windows was paging.
- `textures picked by index: the card has no room for more; they wait`: the
  card's budget, which is the same RAM, was full too.
- `no wave32 lowering for this wave64 program`: one compute program (hash
  `02384a60fda6f2d0`) did not translate for a card whose widest wave is 32, and
  its draws were dropped. That is the other fork's translator. This one tries
  a wave64 program in several more forms on such a card (`emit/mod.rs`: without
  early-outs, without skips, each host wave as its own), so the program may
  translate here. A run of this repository says: look for `shader does not
  translate` in its log, and run once with `VICTORY_WAVE_WHY=1` if it is there.
- `host view heap capped at 1000000`: the title asks for 1,245,184 views and
  the driver gives a million. Not a fault by itself: cards that cap there
  (NVIDIA) run the same way, the title's highest slot in use is about 720,000,
  and 737,000 title slots are left after the dynamic ones
  (`descriptor/host.rs`, `DYNAMIC`).

One more thing the smaller picture avoids: the title's tile lists are indirect
dispatches of one group per 8x4 tile, and on a card that is not AMD they are cut
at 65,535 groups (`indirect.rs`, `widest`). At 2560x1440 that is 115,200 tiles
and the lists are cut; at 1280x720 it is 28,800 and they are not.

What this repository does about the first two, by default on a PC with 32 GiB
of RAM or less:

- The console is Series S unless `--console` says otherwise (less title
  memory, 1080p output).
- The memory budgets of a smaller PC are on (`residency.rs`;
  `VICTORY_RESIDENCY=0` turns them off).

And on request: `play.cmd -Output 1280x720` (or `scripts\play.ps1 -Output
1280x720`) has the title render at 853x480 and show 1280x720. That is about a
ninth of the Series X profile's pixels.

`-Ports` turns on the five renderer patches of the README together (57-75 ms to
about 45 ms a frame where they were measured, an AMD card). They are off by
default while they are tested, and none has run on an Intel card. Try the
smaller picture first, then add `-Ports`, so a fault can be told apart.

The README's own figures say the Series S profile still held about 30 GB of
private memory and 10.5 GB of card memory on a 64 GB PC with a card of its
own. On 32 GB shared that is still more than there is. Expect paging until the
figures of a run of this repository say otherwise.

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
