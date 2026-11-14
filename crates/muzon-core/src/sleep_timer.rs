//! Sleep policy is owned by the audio worker, independent of window visibility.
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SleepChoice {
    #[default]
    Off,
    Minutes(u16),
    EndOfTrack,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SleepState {
    pub remaining_secs: Option<u64>,
    pub end_of_track: bool,
}
#[derive(Default)]
pub(crate) struct SleepTimer {
    deadline: Option<Instant>,
    end_of_track: bool,
}
impl SleepTimer {
    pub fn set(&mut self, choice: SleepChoice, now: Instant) {
        self.deadline = match choice {
            SleepChoice::Minutes(minutes) if minutes > 0 => {
                now.checked_add(Duration::from_secs(u64::from(minutes.min(1440)) * 60))
            }
            _ => None,
        };
        self.end_of_track = choice == SleepChoice::EndOfTrack;
    }
    pub fn state(&self, now: Instant) -> SleepState {
        SleepState {
            remaining_secs: self.deadline.map(|deadline| {
                let remaining = deadline.saturating_duration_since(now);
                remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)
            }),
            end_of_track: self.end_of_track,
        }
    }
    pub fn fires(&mut self, now: Instant, ended: bool) -> bool {
        let fired =
            self.deadline.is_some_and(|deadline| now >= deadline) || (self.end_of_track && ended);
        if fired {
            self.set(SleepChoice::Off, now);
        }
        fired
    }
    pub fn waits_for_end(&self) -> bool {
        self.end_of_track
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_uses_elapsed_time_and_fires_once() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set(SleepChoice::Minutes(1), now);
        assert_eq!(
            timer.state(now + Duration::from_millis(500)).remaining_secs,
            Some(60)
        );
        assert!(!timer.fires(now + Duration::from_secs(59), true));
        assert!(timer.fires(now + Duration::from_secs(60), false));
        assert!(!timer.fires(now + Duration::from_secs(61), false));
    }
    #[test]
    fn modes_replace_each_other_and_cancel_clears_both() {
        let now = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set(SleepChoice::Minutes(15), now);
        timer.set(SleepChoice::EndOfTrack, now);
        assert_eq!(timer.state(now).remaining_secs, None);
        assert!(!timer.fires(now + Duration::from_secs(9999), false));
        assert!(timer.fires(now, true));
        assert!(!timer.fires(now, true));
        timer.set(SleepChoice::EndOfTrack, now);
        timer.set(SleepChoice::Off, now);
        assert_eq!(timer.state(now), SleepState::default());
        timer.set(SleepChoice::Minutes(0), now);
        assert_eq!(timer.state(now), SleepState::default());
    }
}
