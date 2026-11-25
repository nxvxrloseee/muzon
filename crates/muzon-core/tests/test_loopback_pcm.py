import tempfile
import unittest
import wave
from pathlib import Path
from loopback_pcm import fixture, compare_pcm


class ComparisonTests(unittest.TestCase):
    def test_marker_alignment_compares_entire_payload_for_each_depth(self):
        with tempfile.TemporaryDirectory() as root:
            for bits in (16,24):
                path=Path(root)/f'{bits}.wav'
                expected=fixture(path,44100,bits)
                frame=2*(bits//8)
                captured=bytes(frame*137)+expected+bytes(frame*33)
                self.assertEqual(compare_pcm(expected,captured,frame),137)
                with wave.open(str(path),'rb') as wav:
                    self.assertEqual(wav.getsampwidth(),bits//8)
                    self.assertEqual(wav.getnchannels(),2)
                    self.assertEqual(wav.getframerate(),44100)

    def test_late_corruption_is_not_hidden_by_a_matching_marker(self):
        with tempfile.TemporaryDirectory() as root:
            expected=fixture(Path(root)/'late.wav',8000,16)
        captured=bytearray(bytes(40)+expected)
        captured[-100]^=1
        with self.assertRaisesRegex(ValueError,'differs'):
            compare_pcm(expected,bytes(captured),4)

    def test_truncated_duplicate_unaligned_and_missing_captures_fail(self):
        with tempfile.TemporaryDirectory() as root:
            expected=fixture(Path(root)/'x.wav',8000,16)
        for captured,reason in [(expected[:-4],'truncated'),(expected+expected,'ambiguous'),
                                (b'\0'+expected,'aligned'),(bytes(len(expected)),'absent')]:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError,reason):
                compare_pcm(expected,captured,4)

    def test_silent_marker_is_not_accepted_as_identity_evidence(self):
        with self.assertRaisesRegex(ValueError,'nonzero'):
            compare_pcm(bytes(1000),bytes(1000),4)


if __name__=='__main__':
    unittest.main()
