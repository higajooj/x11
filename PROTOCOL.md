# Attack Shark X11 HID protocol

Recovered from `AttackShark.exe` v1.1 (Ghidra decompile of the `MS_4` page class,
cross-checked against the shared `MS_10` class) and confirmed against a live
X11 on its 2.4 GHz receiver (`1d57:fa60`, firmware version `0x0110`).

## Transport

The receiver exposes four HID interfaces. Configuration uses the one whose
report descriptor has a top-level collection on usage page `0x0B`, usage `0`
(on Linux it's a single hidraw node that also carries the status input report).

| Report | Dir | Size (incl. ID) | Purpose |
|---|---|---|---|
| `0x03` | input | 5 | status / heartbeat / write ack |
| `0x04` | feature | 56 | DPI stages, stage colours, ripple, angle snap |
| `0x05` | feature | 15 | lighting + power timers + debounce |
| `0x06` | feature | 9 | polling rate |
| `0x08` | feature | 59 | button mapping (18 slots) |
| `0x09` | feature | 64 | macro upload (one macro = 3 chunks) |
| `0x0a` | feature | 128 | bulk read of current settings |
| `0x0b` | feature | 8 | firmware version |
| `0x0c` | feature | 10 | sent before a full-profile write (`0c 0a 01 fe 01 fe ...`) |
| `0xa0` | feature | 8 | read request / ready flag |

Multi-byte checksums are a 16-bit sum of the payload bytes, stored
big-endian (`hi, lo`) right after the summed range.

## Status report `0x03`

`03 MM TT LL HH` where `MM` is the model byte (`0x55` = X11 / app page MS_4)
and `TT` the event type:

| `TT` | Meaning |
|---|---|
| `0x10` | active DPI stage changed, `LL` = stage (1-based) |
| `0x20` | polling rate changed, low nibble of `LL` = index+1 |
| `0x40` | battery: `LL` = state (`2` full, `3` charging, else discharging), `HH` = percent |
| `0x50` | write ack: `LL == 0` success, `HH` = report ID acknowledged |
| `0x70` | light mode changed, high nibble of `LL` |

The receiver sends a `0x40` heartbeat every ~2 s; the app treats 6 s of
silence as disconnected.

## Reading

1. `SET_FEATURE a0 [id, len, 0, flag, 0, 0, 0]`
2. poll `GET_FEATURE a0` until byte 1 is `0x01`
3. `GET_FEATURE id` with `len` bytes

The app first reads `0x0b` (len 8, flag 0): version = `(b[2] << 8) | b[3]`.
If the high byte equals the model byte it reads `0x06`, `0x05`, `0x04`
individually; otherwise it reads the 128-byte bulk report `0x0a` (flag 1).

Bulk `0x0a` layout: `b[3]` polling byte, `b[5..=51]` = report-4 payload
(bytes 3.. of the write packet), `b[55..=62]` = report-5 payload,
`b[65 + 3*i ..]` = button slot `i` (3 bytes each, 18 slots).

## Writing

Send the feature report, then wait for a `0x50` ack for that report ID.
The app retries every 500 ms, up to 10 times.

### `0x04` DPI (56 bytes)

```
[0]=04 [1]=38 [2]=01
[3]  angle snap (0/1)
[4]  ripple control (0/1)
[5]  stage-enabled bitmask
[6]  "high range" bitmask (stage slider >= 221)   [7] = same as [6]
[8..15]  per-stage DPI code (low byte)
[16..23] per-stage "x2" flag
[24] active stage (1-based)
[25..48] 8 x RGB stage colours
[49] 02
[50..51] checksum of [3..49]
```

DPI is edited as a slider value `v` (1–320 on the X11):
`dpi = v*50` for `v <= 200`, else `(v-100)*100`. Encoding of `v` uses three
lookup tables from the binary; see `src/protocol.rs`.

### `0x05` lighting / power (15 bytes)

```
[3]  mode << 4
[4]  (deep_sleep & 0xf0) | (9 - speed)
[5]  (deep_sleep << 4) | (brightness if mode in {1,5} else 8)
[6..8] RGB
[9]  sleep (light-off) timer, units of 30 s
[10] key debounce, units of 2 ms
[11..12] checksum of [3..10]
```

Modes: 0 off, 1 static, 2 breathing, 3 neon, 4 colour breathing,
5 static DPI, 6 breathing DPI.

### `0x06` polling rate (9 bytes)

`06 09 01 RR ~RR` with `RR` = `8`/`4`/`2`/`1` for 125/250/500/1000 Hz.

### `0x08` buttons (59 bytes)

`08 3b 01` then 18 slots × 3 bytes, checksum of `[3..56]` at `[57..58]`.
Each slot is `(kind, a, b)`; `kind 0x11` is a keyboard key with `a` = HID
modifier mask and `b` = HID usage, `kind 0x12` is a macro with `b` =
macro number (slot index + 1). Other kinds are fixed functions (see the
table in `src/protocol.rs`). X11 physical buttons are slots 1, 2, 3, 7, 8
(left, right, middle, forward, back); slot 6 is the DPI button.

### `0x09` macro (131 bytes, sent as 3 × 64-byte chunks)

Logical packet:
```
[0]=09 [1]=83 [2]=macro number
[3] repeat mode (0 = N times, 1 = until any key, 2 = while held)
[4..6] 0
[7] repeat count (min 1)
[28] event count
[29..] events, 2 bytes each: (round(delay/10ms) | (0x01 down / 0x81 up), key)
       delays >= 1271 ms emit an extra (delay/200, 0x03) pair first
[129..130] checksum of [3..128]
```
On the wire: chunk `k` = `09 LL NN k` + 60 bytes of `[3..]`, where `LL` is
`0x40` for chunks 0 and 1 and `0x0c` for the last chunk.

Macro key codes are HID usages; mouse buttons are `F1` left, `F2` right,
`F3` middle, `F4` back, `F5` forward.
