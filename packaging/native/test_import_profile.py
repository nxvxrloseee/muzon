#!/usr/bin/env python3
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
from import_profile import import_profile


class ImportTest(unittest.TestCase):
    def test_wal_snapshot_keeps_library_and_skips_accounts(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            data, config, target = root / 'data', root / 'config', root / 'native'
            data.mkdir(); config.mkdir()
            source = sqlite3.connect(data / 'muzon.sqlite3')
            source.execute('PRAGMA journal_mode=WAL')
            source.execute('CREATE TABLE tracks(id INTEGER PRIMARY KEY,title TEXT)')
            source.execute('CREATE TABLE native_listen_outbox(id INTEGER)')
            source.execute("INSERT INTO tracks VALUES(7,'WAL track')")
            source.commit()
            before = (data / 'muzon.sqlite3').read_bytes()
            (config / 'session.json').write_text(json.dumps({'cursor': 0, 'queuePaths': ['/music/a.flac']}))
            (config / 'scrobble.json').write_text('{"listenbrainzUser":"private"}')
            (config / 'scrobble-queue.json').write_text('{"private":"history"}')
            (config / 's3.json').write_text('{"bucket":"private"}')
            (config / 'theme.json').write_text('{"background":"#112233"}')
            import_profile(data, config, target)
            self.assertEqual((data / 'muzon.sqlite3').read_bytes(), before)
            with sqlite3.connect(target / 'muzon.sqlite3') as copied:
                self.assertEqual(copied.execute('SELECT * FROM tracks').fetchall(), [(7, 'WAL track')])
                self.assertEqual(copied.execute("SELECT name FROM sqlite_master WHERE name='native_listen_outbox'").fetchall(), [])
            self.assertEqual(sorted(p.name for p in (target / 'config').iterdir()), ['appearance.json', 'imported-theme.json', 'session.json'])
            self.assertEqual(json.loads((target / 'config/appearance.json').read_text())['themeFile'], str(target / 'config/imported-theme.json'))
            source.close()
            with self.assertRaises(FileExistsError):
                import_profile(data, config, target)
            self.assertTrue((target / 'muzon.sqlite3').is_file())

    def test_invalid_config_rolls_back_only_new_destination(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch); data = root / 'data'; data.mkdir()
            with sqlite3.connect(data / 'muzon.sqlite3') as db:
                db.execute('CREATE TABLE tracks(id INTEGER)')
            (data / 'session.json').write_text('broken')
            with self.assertRaises(ValueError):
                import_profile(data, data, root / 'native')
            self.assertFalse((root / 'native').exists())
            self.assertEqual((data / 'session.json').read_text(), 'broken')
            with self.assertRaises(ValueError):
                import_profile(data, data, data / 'nested')


if __name__ == '__main__':
    unittest.main()
