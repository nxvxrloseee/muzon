#!/usr/bin/env python3
"""Opt-in byte comparison through the real engine and an existing snd-aloop card.
Never loads modules or opens a physical output. Exit 77 means no Loopback card.
Run: python3 crates/muzon-core/tests/loopback_pcm.py [--probe /path/to/aloop_probe]
"""
import argparse
import json
import random
import shutil
import signal
import subprocess as sp
import tempfile
import time
import wave
from pathlib import Path


def fixture(path, rate, bits):
    width = bits // 8
    rng = random.Random(0x4D555A4F4E + rate + bits)
    # Different stereo channels exercise channel order as well as sample values.
    payload = b''.join(rng.randrange(-(1 << (bits-3)), 1 << (bits-3)).to_bytes(width, 'little', signed=True)
                       for _ in range(rate * 2))
    silence = bytes((rate // 10) * 2 * width)
    with wave.open(str(path), 'wb') as audio:
        audio.setnchannels(2)
        audio.setsampwidth(width)
        audio.setframerate(rate)
        audio.writeframes(silence + payload + silence)
    return payload + silence


def compare_pcm(expected, captured, frame_bytes):
    """Find one unique nonzero marker, then compare *every* expected payload byte.
    Recorder start latency may add silence; it may not reorder/convert samples.
    """
    marker = expected[:32 * frame_bytes]
    if len(marker) < 32 * frame_bytes or not any(marker):
        raise ValueError('fixture requires a nonzero 32-frame marker')
    offset = captured.find(marker)
    if offset < 0:
        raise ValueError('PCM marker absent: missing, converted or corrupted samples')
    if offset % frame_bytes:
        raise ValueError('PCM marker is not aligned to an interleaved frame')
    if captured.find(marker, offset + 1) >= 0:
        raise ValueError('ambiguous marker: possible duplicated playback')
    if len(captured) < offset + len(expected):
        raise ValueError('capture is truncated before the expected tail')
    actual = captured[offset:offset+len(expected)]
    if actual != expected:
        mismatch = next(i for i, (a, b) in enumerate(zip(actual, expected)) if a != b)
        raise ValueError(f'PCM differs at payload byte {mismatch}, frame {mismatch // frame_bytes}')
    return offset // frame_bytes


def closed(path):
    return path.read_text().strip() == 'closed'


def run_case(probe, root, rate, bits):
    proc = Path('/proc/asound/Loopback')
    playback = proc / 'pcm0p/sub0/hw_params'
    capture = proc / 'pcm1c/sub0/hw_params'
    if not closed(playback) or not closed(capture):
        raise RuntimeError('Loopback subdevice 0 is occupied; refusing to change another stream')
    source = root / f'{rate}-{bits}.wav'
    output = root / f'{rate}-{bits}.raw'
    expected = fixture(source, rate, bits)
    fmt = {16: 'S16_LE', 24: 'S24_3LE'}[bits]
    recorder_log = root / f'{rate}-{bits}-capture.log'
    with recorder_log.open('w') as log:
        recorder = sp.Popen(['arecord', '-q', '-D', 'hw:CARD=Loopback,DEV=1,SUBDEV=0',
                             '-t', 'raw', '-f', fmt, '-r', str(rate), '-c', '2',
                             str(output)], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 3
            while not capture.exists() or 'rate:' not in capture.read_text():
                if recorder.poll() is not None:
                    raise RuntimeError('capture negotiation failed; see ' + str(recorder_log))
                if time.monotonic() >= deadline:
                    raise RuntimeError('capture did not pre-arm within deadline')
                time.sleep(.02)
            params = capture.read_text()
            if f'rate: {rate} ' not in params or 'channels: 2' not in params:
                raise RuntimeError('capture hardware parameters differ from fixture: ' + params)
            result = sp.run([str(probe), str(source), 'hw:CARD=Loopback,DEV=0'],
                            capture_output=True, text=True, timeout=20)
            (root / f'{rate}-{bits}-engine.log').write_text(result.stdout + result.stderr)
            if result.returncode:
                raise RuntimeError('engine rejected format/output; this is not a passing bit-identity test')
            time.sleep(.25)
        finally:
            if recorder.poll() is None:
                recorder.send_signal(signal.SIGINT)
            try:
                recorder.wait(timeout=3)
            except sp.TimeoutExpired:
                recorder.kill()
                recorder.wait()
    offset = compare_pcm(expected, output.read_bytes(), 2 * (bits // 8))
    return {'rate': rate, 'bits': bits, 'channels': 2, 'matched_bytes': len(expected),
            'capture_offset_frames': offset, 'result': 'byte-identical-loopback-payload'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe', type=Path)
    args = parser.parse_args()
    card = Path('/proc/asound/Loopback/id')
    if not card.exists() or card.read_text().strip() != 'Loopback':
        print('SKIP: no existing snd-aloop Loopback card; no module or physical output was touched')
        return 77
    if not shutil.which('arecord'):
        parser.error('arecord is required')
    repo = Path(__file__).resolve().parents[3]
    probe = args.probe
    if probe is None:
        sp.run(['cargo', 'build', '--locked', '-p', 'muzon-core', '--example', 'aloop_probe'], cwd=repo, check=True)
        metadata = json.loads(sp.check_output(['cargo','metadata','--no-deps','--format-version','1'],cwd=repo,text=True))
        probe = Path(metadata['target_directory']) / 'debug/examples/aloop_probe'
    probe = probe.resolve(strict=True)
    root = Path(tempfile.mkdtemp(prefix='muzon-loopback-pcm-'))
    results = []
    print('Scratch:', root, flush=True)
    for rate in (44100, 48000, 96000, 192000):
        for bits in (16, 24):
            try:
                result = run_case(probe, root, rate, bits)
                print(f'PASS: {rate} Hz / {bits} bit: exact PCM payload', flush=True)
            except (OSError, ValueError, RuntimeError, sp.SubprocessError) as error:
                result = {'rate':rate,'bits':bits,'result':'failed','reason':str(error)}
                print(f'FAIL: {rate} Hz / {bits} bit: {error}', flush=True)
            results.append(result)
    (root/'results.json').write_text(json.dumps(results,indent=2))
    return int(any(row['result']=='failed' for row in results))


if __name__ == '__main__':
    raise SystemExit(main())
