import { describe, expect, it } from "vitest";
import { clock, videoName } from "@/components/game/mappers";
import { startIndex } from "./PlayerPage";

const items = ["S1/E1.mp4", "S1/E2.mp4", "S1/E3.mp4"].map((file) => ({ file }));
const at = (file: string, position: number, duration = 100) => ({ file, position, duration, updated_at: 1 });

describe("player", () => {
  it("starts where you left off, or at the next video once one is finished", () => {
    expect(startIndex(items, [])).toBe(0);
    expect(startIndex(items, [at("S1/E2.mp4", 40)])).toBe(1);
    expect(startIndex(items, [at("S1/E2.mp4", 99)])).toBe(2);
    expect(startIndex(items, [at("S1/E3.mp4", 99)])).toBe(2);
    expect(startIndex(items, [at("gone.mp4", 10)])).toBe(0);
  });

  it("formats times and names", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(249)).toBe("4:09");
    expect(clock(3725)).toBe("1:02:05");
    expect(videoName("Season 1/Episode 2.mp4")).toBe("Episode 2");
  });
});
