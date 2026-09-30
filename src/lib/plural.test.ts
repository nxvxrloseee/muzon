import { describe, expect, it } from "vitest";
import { tracksLabel } from "./plural";

describe("tracksLabel", () => {
  it.each([
    [0, "0 треков"],
    [1, "1 трек"],
    [3, "3 трека"],
    [5, "5 треков"],
    [11, "11 треков"],
    [12, "12 треков"],
    [21, "21 трек"],
    [22, "22 трека"],
    [111, "111 треков"],
    [104, "104 трека"],
  ])("%i", (count, label) => {
    expect(tracksLabel(count)).toBe(label);
  });
});
