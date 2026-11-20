import { describe, expect, it } from "vitest";
import { friendlyError } from "./errors";
import { cleanVersion, imageSrc, statusTone, timeAgo } from "./utils";

describe("friendlyError", () => {
  it("explains rate limits in minutes or seconds", () => {
    expect(friendlyError({ kind: "rate_limited", retry_in_secs: 45 }).detail).toContain("45 seconds");
    expect(friendlyError({ kind: "rate_limited", retry_in_secs: 300 }).detail).toContain("5 minutes");
    expect(friendlyError({ kind: "rate_limited", retry_in_secs: 45 }).action).toBe("wait");
  });

  it("sends auth problems to sign-in", () => {
    expect(friendlyError({ kind: "unauthorized", message: "" }).action).toBe("sign_in");
    expect(friendlyError({ kind: "not_signed_in" }).action).toBe("sign_in");
  });

  it("keeps the server's detail message", () => {
    expect(friendlyError({ kind: "forbidden", message: "Pending approval" }).detail).toBe("Pending approval");
  });

  it("handles unknown throwables", () => {
    expect(friendlyError("boom").detail).toBe("boom");
    expect(friendlyError(undefined).title).toBe("Something went wrong");
  });
});

describe("utils", () => {
  it("cleans version prefixes", () => {
    expect(cleanVersion("v0.9.1")).toBe("0.9.1");
    expect(cleanVersion(" V1.0 ")).toBe("1.0");
    expect(cleanVersion("Final")).toBe("Final");
    expect(cleanVersion(null)).toBe("");
  });

  it("maps status prefixes", () => {
    expect(statusTone(["Ren'Py", "Completed"])?.tone).toBe("success");
    expect(statusTone(["Abandoned"])?.label).toBe("Abandoned");
    expect(statusTone(["Onhold"])?.tone).toBe("warning");
    expect(statusTone(["Unity"])).toBeNull();
  });

  it("formats relative time", () => {
    const now = 1_700_000_000_000;
    expect(timeAgo(now / 1000 - 30, now)).toBe("just now");
    expect(timeAgo(now / 1000 - 3 * 3600, now)).toBe("3h ago");
    expect(timeAgo(now / 1000 - 2 * 86400, now)).toBe("2d ago");
    expect(timeAgo(null, now)).toBe("");
  });

  it("routes images through the local cache scheme", () => {
    const src = imageSrc("https://attachments.f95zone.to/a b.png")!;
    expect(src).toMatch(/gmimg/);
    expect(src).toContain(encodeURIComponent("https://attachments.f95zone.to/a b.png"));
    expect(imageSrc(null)).toBeUndefined();
  });
});
