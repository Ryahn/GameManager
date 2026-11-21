import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { type BBElement, decodeEntities, parseBBCode, safeColor, safeUrl, sizeEm } from "./bbcode";
import { BBCode } from "@/components/BBCode";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

describe("parseBBCode", () => {
  it("nests tags case-insensitively", () => {
    expect(parseBBCode("a [B]bold [i]both[/I][/b] c")).toEqual([
      "a ",
      { tag: "b", arg: undefined, children: ["bold ", { tag: "i", arg: undefined, children: ["both"] }] },
      " c",
    ]);
  });

  it("keeps unknown tags and stray brackets as text", () => {
    expect(parseBBCode("[v0.2] [Ren'Py] [/b] [x]")).toEqual(["[v0.2] [Ren'Py] [/b] [x]"]);
  });

  it("closes tags left open, and inner tags when an outer one closes", () => {
    expect(parseBBCode("[b]never closed")).toEqual([{ tag: "b", arg: undefined, children: ["never closed"] }]);
    const [b] = parseBBCode("[b][i]x[/b]y") as [{ children: unknown[] }, string];
    expect(b.children).toEqual([{ tag: "i", arg: undefined, children: ["x"] }]);
    expect(parseBBCode("[b][i]x[/b]y")[1]).toBe("y");
  });

  it("splits list items at each [*]", () => {
    const [list] = parseBBCode("[list]\n[*]one\n[*]two\n[/list]") as BBElement[];
    expect(list.tag).toBe("list");
    expect(list.children.filter((c): c is BBElement => typeof c !== "string").map((c) => c.tag)).toEqual(["*", "*"]);
  });

  it("takes code literally and reads quoted arguments", () => {
    expect(parseBBCode("[code][b]not bold[/b][/code]")).toEqual([{ tag: "code", arg: undefined, children: ["[b]not bold[/b]"] }]);
    expect(parseBBCode('[url="https://x.y/a"]go[/url]')[0]).toMatchObject({ tag: "url", arg: "https://x.y/a" });
  });

  it("caps nesting depth", () => {
    const deep = "[b]".repeat(100) + "x";
    const count = (n: unknown): number =>
      typeof n === "object" && n && "children" in n ? 1 + Math.max(0, ...(n as { children: unknown[] }).children.map(count)) : 0;
    expect(count(parseBBCode(deep)[0])).toBeLessThanOrEqual(25);
  });
});

describe("sanitising", () => {
  it("only allows web links", () => {
    expect(safeUrl("https://f95zone.to/x")).toBe("https://f95zone.to/x");
    expect(safeUrl("www.example.com")).toBe("https://www.example.com/");
    expect(safeUrl("javascript:alert(1)")).toBeNull();
    expect(safeUrl("file:///C:/Windows")).toBeNull();
  });

  it("accepts readable colours and drops black/white and junk", () => {
    expect(safeColor("#F5B544")).toBe("#f5b544");
    expect(safeColor("red")).toBe("red");
    expect(safeColor("rgb(255, 0, 0)")).toBe("#ff0000");
    expect(safeColor("#000")).toBeNull();
    expect(safeColor("white")).toBeNull();
    expect(safeColor("red; background:url(x)")).toBeNull();
  });

  it("maps sizes into a sane range", () => {
    expect(sizeEm("5")).toBe(1.25);
    expect(sizeEm("200px")).toBe(1.75);
    expect(sizeEm("huge")).toBeNull();
  });

  it("decodes common entities", () => {
    expect(decodeEntities("Tom &amp; Jerry &#39;s &lt;3 &#x2764;")).toBe("Tom & Jerry 's <3 ❤");
  });
});

describe("<BBCode>", () => {
  it("renders formatting as elements, not markup", () => {
    const { container } = render(<BBCode text={"[b]v0.9[/b] is [i]out[/i] <script>alert(1)</script>"} />);
    expect(container.querySelector("strong")?.textContent).toBe("v0.9");
    expect(container.querySelector("em")?.textContent).toBe("out");
    expect(container.querySelector("script")).toBeNull();
    expect(container.textContent).toContain("<script>alert(1)</script>");
  });

  it("renders lists, spoilers and links", () => {
    const { container } = render(
      <BBCode text={"[LIST]\n[*]one\n[*]two\n[/LIST]\n[SPOILER=Old]hidden[/SPOILER]\n[URL=https://a.b/c]site[/URL] and https://d.e/f"} />,
    );
    expect([...container.querySelectorAll("li")].map((l) => l.textContent)).toEqual(["one", "two"]);
    expect(container.querySelector("details summary")?.textContent).toContain("Spoiler: Old");
    expect(screen.getByText("site").closest("a")?.getAttribute("href")).toBe("https://a.b/c");
    expect(screen.getByText("https://d.e/f").closest("a")).not.toBeNull();
  });

  it("never links unsafe urls", () => {
    const { container } = render(<BBCode text={"[url=javascript:alert(1)]click[/url]"} />);
    expect(container.querySelector("a")).toBeNull();
    expect(container.textContent).toBe("click");
  });
});
