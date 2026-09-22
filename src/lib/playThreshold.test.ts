import { describe, expect, it } from "vitest";
import { shouldCountPlay } from "./playThreshold";

describe("shouldCountPlay", () => {
  it("counts an ordinary track at its halfway point", () => {
    expect(shouldCountPlay(99, 200)).toBe(false);
    expect(shouldCountPlay(100, 200)).toBe(true);
  });

  it("counts a long track after four minutes rather than waiting for half", () => {
    // A 20-minute set shouldn't need 10 minutes to register.
    expect(shouldCountPlay(239, 1200)).toBe(false);
    expect(shouldCountPlay(240, 1200)).toBe(true);
  });

  it("never counts something too short to be a song", () => {
    expect(shouldCountPlay(29, 29)).toBe(false);
    expect(shouldCountPlay(15, 20)).toBe(false);
  });

  it("ignores a duration that isn't known yet", () => {
    // Duration reads as 0 for the first ticks after a track starts.
    expect(shouldCountPlay(0, 0)).toBe(false);
    expect(shouldCountPlay(120, 0)).toBe(false);
  });
});
