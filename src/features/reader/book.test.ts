import { describe, expect, it } from "vitest";
import { flatten, spreadOf, spreads } from "./book";

const book = {
  root: "C:/Comics/X",
  other_files: [],
  chapters: [
    { name: "Ch1", pages: ["1", "2", "3", "4", "5"].map((n) => ({ src: `/f/${n}`, name: n })) },
    { name: "Ch2", pages: ["1", "2", "3"].map((n) => ({ src: `/f/b${n}`, name: n })) },
  ],
};

describe("reader book", () => {
  it("flattens chapters in order", () => {
    const { pages, starts } = flatten(book);
    expect(pages).toHaveLength(8);
    expect(starts).toEqual([0, 5]);
    expect(pages[5]).toMatchObject({ chapter: 1, name: "1" });
  });

  it("pairs pages but keeps covers and wide pages alone", () => {
    const { pages } = flatten(book);
    expect(spreads(pages, new Set())).toEqual([[0], [1, 2], [3, 4], [5], [6, 7]]);
    // Page 2 is a double-page scan: it stands alone and pairing resumes after.
    expect(spreads(pages, new Set([2]))).toEqual([[0], [1], [2], [3, 4], [5], [6, 7]]);
  });

  it("finds the spread for a page", () => {
    const list = spreads(flatten(book).pages, new Set());
    expect(spreadOf(list, 4)).toBe(2);
    expect(spreadOf(list, 99)).toBe(0);
  });
});
