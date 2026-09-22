/**
 * Below this a track is a jingle, an interlude or a skit, and counting it
 * distorts everything built on play counts. The threshold scrobblers settled on.
 */
const MIN_TRACK_SECS = 30;

/** Listening this far in counts even for a track far longer than twice it. */
const ALWAYS_COUNT_AFTER_SECS = 240;

/**
 * Whether the current position means this track has actually been listened to.
 *
 * Half the track, or four minutes, whichever comes first - so an album cut and
 * a forty-minute live set both count at a point that means the same thing. Note
 * this says nothing about *whether it has already been counted*; the caller
 * owns that, since crossing the line repeatedly is one listen, not many.
 */
export function shouldCountPlay(positionSecs: number, durationSecs: number): boolean {
  if (!(durationSecs > MIN_TRACK_SECS)) return false;
  return positionSecs >= Math.min(durationSecs / 2, ALWAYS_COUNT_AFTER_SECS);
}
