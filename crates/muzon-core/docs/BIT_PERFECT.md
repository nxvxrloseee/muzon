# Direct ALSA output validation

`OutputMode::BitPerfect` requests a direct path; it is never verification.
`OutputStatus.bit_perfect_verified` is deliberately always false. Sink caps
report negotiated PCM format/rate/channels only. They do not establish physical
DAC configuration, USB behaviour, source identity, or absence of sample changes.

## Implementation and API

- `domain::playback_settings::OutputMode::{Normal, BitPerfect}`;
  `PlaybackSettings.output_mode`, `.output_device` (serde defaults keep old
  profiles normal); `PlaybackEdit::{OutputMode, OutputDevice}`.
- `data::audio::output::{OutputDevice, OutputStatus, SinkFormat,
  list_output_devices}`. Device fields: `id`, `name`. Status fields: `mode`,
  `device`, `requested_mode`, `requested_device`, `blocked`, `negotiated_format`,
  `bit_perfect_verified`, `error`. Format fields:
  `format` (PCM string including meaningful depth/container, e.g. S24_32LE),
  `rate` (Hz), `channels`. No negotiated format is reported while stopped/failed.
- `AudioPlayer::{configure_output, output_status}`. Reconfiguration stops both
  decks, replaces sink/filter/flags in NULL, prerolls and restores position and
  playing/paused state, with bounded waits. Failed configuration stops playback
  and blocks further play until reconfigured successfully; no normal fallback.
- `Snapshot::{output_status, output_devices}`;
  `ControllerClient::refresh_output_devices()` refreshes on the worker. Call
  on settings open and after hotplug (or periodically while the picker is open).
  No suitable provider/device is not permission to substitute default/plughw.
- `data::tags::read_audio_format(&Path) -> anyhow::Result<String>`: source badge
  using Lofty with tags disabled. Rate/depth omitted if unavailable; not output
  evidence. No Track/database schema change.

Normal retains automatic system output, DSP, cubic volume and normal gapless /
crossfade. Device preference is used for direct mode. BitPerfect detaches the
entire filter, sets playbin flags to `audio+native-audio` (no `soft-volume`),
volume to 1.0, `alsasink device=hw:... provide-clock=true slave-method=none`.
Native-audio omits playsink's converters/resampler, so conversion dithering and
noise shaping have no active element in this path. EQ/ReplayGain/tempo/spectrum
preferences remain stored but are inaudible; tempo reports 1.0. Next-track URI
handoff/pre-roll/crossfade are not armed in direct mode; EOS causes the normal
controller queue advance and a NULL/reopen between every pair, conservatively
including same-rate tracks. Direct errors stop rather than skip through a queue
of tracks on an unavailable device. Explicit Next/Play may retry.

## Automated coverage (no physical hardware)

`cargo test -p muzon-core --locked` tests settings compatibility, direct device
validation, stable ID extraction, inactive sink/flags/filter configuration,
unity volume, disabled transition arming, return to Normal, configuration failure
blocking, paused state/position restoration with normal fakesinks, and recovery
from a missing source file. Existing normal DSP/transition tests remain in place.
The direct configuration test uses a nonexistent card identifier and never takes
that sink out of NULL. Fakesinks prove application policy, not bit-perfect audio.

## Opt-in snd-aloop probe

Nothing below is run by the regular test suite. The machine must **already**
have a Loopback card; this project does not load modules or alter real devices.
Use a disposable profile and a known short PCM WAV/FLAC with all effects stored
non-neutral to demonstrate their suppression. Run:

```sh
cargo run -p muzon-core --example aloop_probe -- /absolute/path/known.wav
```

The example refuses any device except `hw:CARD=Loopback,DEV=0` or `DEV=1`, uses
this engine's actual AudioPlayer, and prints negotiated caps plus read-only
`/proc/asound/Loopback/pcm*p/sub0/hw_params` while playing. Ensure sub0 is free;
occupied subdevices make the printed evidence ambiguous. It never loads modules.
Do not treat matching caps/hw_params as a sample-identity test.

For byte-level validation, capture the paired Loopback PCM with an independently
configured ALSA recorder (playback DEV=0 pairs with capture DEV=1, same subdevice)
at exactly the fixture's rate, channels and sample container. Pre-arm capture,
play a deterministic PCM fixture with leading/trailing silence and a unique
nonzero marker, and compare the aligned non-silent PCM bytes to the decoded
source. Trim recorder start/end silence, never normalize, convert or resample.
Fail on any differing payload byte, not just rate/depth. Repeat S16LE / S24LE /
S24_32LE when supported; record rejected containers rather than converting them.
Capture timing and frame alignment need independent validation; no byte-identity
result has been obtained on this machine.

## Physical DAC checklist (manual, explicit device selection)

1. Use an output-only DAC profile; a simultaneous microphone may constrain rate.
   Select its stable `hw:CARD=<id>,DEV=<n>` ID. Other apps cannot share this PCM.
   Set safe physical amplifier/DAC volume first: the player sends unity gain.
2. Try known lossless fixtures at 44.1 / 48 / 96 / 192 kHz, 16 and 24 meaningful
   bits (including 24-in-32 containers as appropriate). For each record source
   metadata, negotiated sink caps and the actual `/proc/asound/.../hw_params`.
   A lossy decoder can output PCM, but that is not source-lossless verification.
3. Alternate rates and containers at EOS, Next/Previous, repeat-one, repeat-all,
   and manual seeks. Confirm device reopens, tails play to EOS, and no crossfade
   or about-to-finish handoff occurs. A gap/click is possible on rate changes.
4. Store non-neutral EQ, ReplayGain/preamp, tempo, crossfade, volume and enable
   spectrum. Direct playback must bypass these; switching back to Normal must
   restore preferences, track speed, and spectrum publication.
5. Switch modes/device while playing and paused. Check path, playhead and pause
   state. Unsupported rate/format, occupied PCM, and disconnected DAC must stop
   with visible errors, no stale negotiated badge and no fallback to system audio.
6. Reconnect, refresh devices, explicitly retry/reconfigure, then return to Normal.
   Also test a source removed before playback and an unavailable media disk.

No physical DAC or loopback playback has been run as part of this change.

## Primary references

- https://gstreamer.freedesktop.org/documentation/playback/playbin.html
- https://gstreamer.freedesktop.org/documentation/alsa/alsasink.html
- https://gstreamer.freedesktop.org/documentation/audio/gstaudiobasesink.html
- https://gstreamer.freedesktop.org/documentation/gstreamer/gstdevicemonitor.html
- https://github.com/GStreamer/gstreamer/blob/main/subprojects/gst-plugins-base/gst/playback/gstplaysink.c
  (`GST_PLAY_FLAG_NATIVE_AUDIO` / `GST_PLAY_FLAG_SOFT_VOLUME` converter gating)
- https://github.com/GStreamer/gstreamer/blob/main/subprojects/gst-plugins-base/ext/alsa/gstalsadeviceprovider.c
  (`alsa.id` and created sink's hw device string)

Context7 `/gstreamer/gstreamer` playback docs were consulted, followed by these
primary sources and local `gst-inspect-1.0` 1.28.9 registry inspection for actual
flag nicknames, sink properties and provider support.

## Automated PCM comparison

```sh
python3 -m unittest discover -s crates/muzon-core/tests -p 'test_*.py'
python3 crates/muzon-core/tests/loopback_pcm.py
```

The comparison runner accepts only the already-present `Loopback` card, refuses
occupied subdevice 0 and does not load kernel modules. It pre-arms raw stereo
capture with `arecord`, generates deterministic 44.1/48/96/192 kHz fixtures at
16/24 bits, plays through the real `aloop_probe` engine and compares the complete
nonzero payload and known tail byte-for-byte after one unique frame-aligned
marker. A matching header/marker alone does not pass. Corruption, truncation,
unaligned or duplicate markers fail. Unsupported packed 24-bit formats are
reported as failures, never silently converted or counted as successes.

Per-case logs and results.json stay in an owned temporary directory. The probe
uses non-neutral EQ and low stored software volume to exercise direct bypass;
no physical output is selected. Exact payload equality on Loopback still does
not prove DAC electronics, USB transport timing or leading/trailing-silence timing.
On the current machine the hardware runner returned **77 (SKIP: no Loopback)**;
the comparison algorithm's four deterministic tests passed.
