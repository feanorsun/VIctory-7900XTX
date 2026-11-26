# Shared-memory GPU path: state and plan

Target: an integrated Intel Arc (UMA adapter, `uma = true` in the `host adapter`
log line). On such a machine the card's memory is the PC's memory, so every copy
the emulator makes between "title memory" and "card memory" is a second copy of
bytes already in RAM. This note says what is already done, what blocks the rest,
and the steps in order. Nothing past stage 1 is implemented.

## Done (integrated graphics only; discrete cards keep their defaults)

| Change | Where | Off switch |
|---|---|---|
| In-place stage 1 by default (title memory windows placed for the GPU, not copied) | `inplace.rs` `default_level` | `VICTORY_INPLACE=0` |
| Texture copies may fill 70 % of the card budget, not 90 % | `resource.rs` `default_percent` | `VICTORY_TEXTURE_ROOM_PERCENT=90` |
| In-place cap at 4 GiB or RAM/8 | `inplace.rs` `default_most` | `VICTORY_INPLACE_MB` |
| Empty texture blocks kept: RAM/32 (1 GB at 32 GB; was 4 GB) | `pool.rs` `kept_blocks` | none (code) |
| Scratch chunks kept: RAM/128 (256 MB; was 1 GB) | `host.rs` `kept_scratch` | none (code) |
| Idle command lists: one per 128 MB of RAM (256 at 32 GB; was 512) | `list.rs` `idle_most` | none (code) |

All of these are estimates. None has been measured on a device.

## Blocked: in-place stage 2 (title buffers, not only windows)

`inplace.rs` states that stage 2 must not be combined with late wait, because a
submission that is not yet run may still write title memory through such a
buffer. But late wait is effectively on in every normal run: `shadow::relay()`
defaults to on (`VICTORY_RELAY` unset; off only when `keeps()` says so), and
`host.rs` `late_stage()` then returns stage 3. So turning stage 2 on by default would break a stated assumption.
It is not done.

To unblock it, one of these has to be shown to hold, on the device:

1. Relay off on integrated graphics, with its measured cost, or
2. stage 2 buffers excluded from the relay's landing ranges, which needs the
   relay's bookkeeping read through first (`shadow.rs`, the relay sections).

## Not started: one copy, not two

The card side and the CPU side of a title buffer are separate allocations, and
`shadow.rs` moves bytes between them on refresh, compare and relay (see the
"copy in the card's memory" sections). On shared memory those two could be one
allocation. That needs the refresh, compare and relay logic rewritten around a
single side, and it touches most of `shadow.rs`. It should start with a design
note and a test harness that compares a frame with the copy on and off.

Textures have a further limit: an in-place texture must be row-major with one
mip level (`inplace.rs` header). Tiled textures, which the title uses, cannot be
placed this way.

## Hard rules found while reading

- Tile mappings into a heap opened from title memory remove the device. Never
  call them (`inplace.rs` header).
- While an opened window stands, the title cannot decommit, free or protect
  those pages (error 87). Freeing hooks exist (`title_memory_freeing`); they
  need the title's own frees routed through them before stage 2.
- A buffer in such a heap needs `ALLOW_CROSS_ADAPTER`.

## Steps, in order

0. **Measure on the Arc.** Run with the defaults and with `VICTORY_INPLACE=0`.
   Record the `host adapter` line, `VICTORY_MEMORY_MAP=1` output, frame times
   at the same place in the prologue, and a captured frame of each.
1. **Stage 1 check.** If the captured frames match and memory drops, keep it.
   If not, `VICTORY_INPLACE=0` becomes the default for integrated graphics.
2. **Relay decision.** Measure relay on and off on the device. Only then decide
   whether stage 2 can coexist with it.
3. **Stage 2 behind a switch** (`VICTORY_INPLACE=2`), default off, validated
   the same way as stage 1.
4. **Single-copy design** for `shadow.rs`, written up before any code.

## Gates

A step is kept only if: the captured frames match the copy path, the run has no
device loss, and memory or frame time moves in the expected direction on the
device. Numbers from the development container are not evidence for any step.
