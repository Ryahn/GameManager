import { describe, expect, it } from "vitest";
import { formatPlaytime, sameVersion, statusMeta } from "./play";

describe("formatPlaytime", () => {
  it("formats durations people can read", () => {
    expect(formatPlaytime(0)).toBe("Not played");
    expect(formatPlaytime(42)).toBe("under a minute");
    expect(formatPlaytime(25 * 60)).toBe("25 min");
    expect(formatPlaytime(3600)).toBe("1h");
    expect(formatPlaytime(3725)).toBe("1h 2m");
  });
});

describe("sameVersion", () => {
  it("ignores a leading v and case", () => {
    expect(sameVersion("v0.9", "0.9")).toBe(true);
    expect(sameVersion("0.9a", "V0.9A")).toBe(true);
    expect(sameVersion("0.9", "0.10")).toBe(false);
    expect(sameVersion("", "")).toBe(false);
  });
});

describe("statusMeta", () => {
  it("falls back to not started", () => {
    expect(statusMeta("finished").label).toBe("Finished");
    expect(statusMeta("bogus" as never).label).toBe("Not started");
  });
});
