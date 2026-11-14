//! Deterministic synthetic test data: a 5000-track library, a deterministic
//! shuffle permutation and placeholder lyrics.
//!
//! Everything is generated from fixed tables and index arithmetic — no RNG
//! state, no file I/O, nothing read from the real machine. `muzon-queue`
//! deliberately contains no randomness (a permutation is always passed in as
//! data), so the prototype generates one here: a seeded LCG driving a
//! Fisher–Yates shuffle, reproducible across runs and tests.

/// A track in the synthetic library.
#[derive(Debug, Clone)]
pub struct Track {
    /// Numeric id, 1-based and unique within the generated library.
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: u32,
}

/// How many tracks the prototype's library holds.
pub const TRACK_COUNT: usize = 5000;

const TITLE_A: &[&str] = &[
    "Midnight",
    "Golden",
    "Silent",
    "Electric",
    "Broken",
    "Velvet",
    "Neon",
    "Distant",
    "Fading",
    "Crimson",
    "Hollow",
    "Winter",
    "Paper",
    "Glass",
    "Wandering",
    "Burnt",
    "Lonely",
    "Sacred",
    "Rusty",
    "Frozen",
    "Analog",
    "Plastic",
    "Sleepless",
    "Gravity",
];

const TITLE_B: &[&str] = &[
    "Heart",
    "City",
    "Sky",
    "River",
    "Fire",
    "Signal",
    "Dream",
    "Rain",
    "Echo",
    "Garden",
    "Machine",
    "Star",
    "Train",
    "Coast",
    "Mirror",
    "Wolf",
    "Cathedral",
    "Harbor",
    "Static",
    "Window",
    "Season",
    "Orbit",
    "Highway",
    "Letter",
];

const ARTISTS: &[&str] = &[
    "Astra Vale",
    "Blue Harbor",
    "Cassette Club",
    "Dune Atlas",
    "Echo Field",
    "Fable & Fawn",
    "Golden Antenna",
    "Halogen Sea",
    "Idle Nation",
    "Jet Coastline",
    "Kite Theory",
    "Lantern Vale",
    "Mercury Motel",
    "North Arcade",
    "Oxide Choir",
    "Paper Planets",
    "Quiet Motors",
    "Radio Meadow",
    "Static Bloom",
    "Tape Parade",
    "Umbra Sound",
    "Violet Engine",
    "Wax Cathedral",
    "Xerox Youth",
    "Yellow Signal",
    "Zephyr Lane",
    "Ashgrove",
    "Cinder Lane",
    "Driftwood Choir",
    "Ember Atlas",
    "Fog Lantern",
    "Glass Harbor",
];

const ALBUMS: &[&str] = &[
    "Long Way Around",
    "Signal Fires",
    "Paper Rooms",
    "Night Swim",
    "Slow Machines",
    "Analog Heart",
    "Field Notes",
    "Winter Harbor",
    "Neon Countryside",
    "Glass Hours",
    "Midnight Radio",
    "Velvet Seasons",
    "Echo Garden",
    "Golden Static",
    "Rust & Rain",
    "Sleepless City",
    "Blue Cathedral",
    "Orbit Diaries",
    "Train Windows",
    "Coast to Coast",
    "Hollow Season",
    "Letters Home",
    "Fading Maps",
    "Quiet Machines",
];

/// The full synthetic library: [`TRACK_COUNT`] tracks with numeric ids,
/// titles, artists, albums and durations.
#[must_use]
pub fn generate_library() -> Vec<Track> {
    generate_tracks(TRACK_COUNT)
}

/// A synthetic library of any size, same deterministic scheme — handy for
/// tests, which do not need 5000 rows to exercise the logic.
#[must_use]
pub fn generate_tracks(count: usize) -> Vec<Track> {
    (0..count)
        .map(|i| Track {
            id: i as u64 + 1,
            title: format!(
                "{} {}",
                TITLE_A[(i * 7 + 3) % TITLE_A.len()],
                TITLE_B[(i * 11 + 5) % TITLE_B.len()],
            ),
            artist: ARTISTS[i % ARTISTS.len()].to_string(),
            album: ALBUMS[(i / 5) % ALBUMS.len()].to_string(),
            duration_secs: 95 + (i as u32 * 73) % 285,
        })
        .collect()
}

/// A deterministic Fisher–Yates permutation of `0..n`.
///
/// This is the higher layer that owns the randomness `muzon-queue` refuses to
/// have; fixed-seeded so every run (and test) sees the same shuffle.
#[must_use]
pub fn shuffle_permutation(n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut rng = Lcg::new(SEED);
    for i in (1..n).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

const SEED: u64 = 0x5EED_CAFE_F00D;

/// The minimal linear congruential generator (Knuth's MMIX constants).
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
}

/// A handful of placeholder lyric lines for the Now Playing screen.
///
/// Deliberately unsynced: there is no time base here, just a few stable lines
/// derived from the track itself. No LRC parsing, no online fetching.
#[must_use]
pub fn lyrics_for(track: &Track) -> Vec<String> {
    let title = &track.title;
    vec![
        "[Verse 1]".to_string(),
        format!("{title} on a borrowed tape,"),
        format!("{} hums the prototype awake,", track.artist),
        "no signal here, no sound to play —".to_string(),
        "[Chorus]".to_string(),
        format!("{title}, {title}, quietly repeated,"),
        "synthetic lines for a synthetic beat.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use muzon_queue::is_permutation;

    #[test]
    fn generated_library_has_the_requested_size_distinct_ids_and_sane_durations() {
        let tracks = generate_tracks(50);
        assert_eq!(tracks.len(), 50);
        for (i, track) in tracks.iter().enumerate() {
            assert_eq!(track.id, i as u64 + 1);
            assert!((95..=379).contains(&track.duration_secs), "{}", track.id);
            assert!(!track.title.is_empty());
            assert!(!track.artist.is_empty());
            assert!(!track.album.is_empty());
        }
    }

    #[test]
    fn the_shuffle_permutation_is_a_permutation_and_deterministic() {
        for n in [0, 1, 2, 64, 5000] {
            assert!(is_permutation(&shuffle_permutation(n), n), "n = {n}");
        }
        assert_eq!(shuffle_permutation(500), shuffle_permutation(500));
    }

    #[test]
    fn lyrics_are_stable_placeholder_lines() {
        let track = &generate_tracks(3)[2];
        assert_eq!(lyrics_for(track), lyrics_for(track));
        assert!(lyrics_for(track).len() >= 4);
    }
}
