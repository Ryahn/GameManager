/**
 * Tests for the first-post parser that runs inside F95 pages
 * (src-tauri/src/f95/op_parser.js). The fixtures follow XenForo's markup for
 * F95 threads; add real saved first posts here when a layout breaks.
 */
import { describe, expect, it } from "vitest";
import src from "../../src-tauri/src/f95/op_parser.js?raw";

type Link = { host: string; url: string; label: string; masked: boolean };
type Group = { label: string; os: string[]; links: Link[] };
type Result = { kind: string; logged_in?: boolean; username?: string | null; groups?: Group[] };

const parse = new Function(`${src}; return f95gmParse;`)() as (d: Document) => Result;

function page(post: string, { loggedIn = true, title = "Game [v0.9] [Dev] | F95zone" } = {}) {
  const html = `<!doctype html><html data-logged-in="${loggedIn}"><head><title>${title}</title></head><body>
    <a class="p-navgroup-link p-navgroup-link--user"><span class="p-navgroup-linkText">Tester</span></a>
    <article class="message message--post"><div class="message-body"><div class="bbWrapper">${post}</div></div></article>
    <article class="message message--post"><div class="message-body"><div class="bbWrapper">
      Reply with <a href="https://mega.nz/file/reply">a link</a>
    </div></div></article>
  </body></html>`;
  return new DOMParser().parseFromString(html, "text/html");
}

const masked = (host: string, n: number) => `https://f95zone.to/masked/${host}/${n}/abc${n}`;

describe("op_parser", () => {
  it("groups links by the label before the colon", () => {
    const r = parse(
      page(`Overview text <a href="https://www.patreon.com/dev">Patreon</a><br><br>
        <b><span style="font-size:22px">DOWNLOAD</span></b><br>
        <b>Win/Linux</b>: <a href="${masked("mega.nz", 1)}">MEGA</a> - <a href="${masked("pixeldrain.com", 2)}">PIXELDRAIN</a> - <a href="https://gofile.io/d/xyz">GOFILE</a><br>
        <b>Mac</b>: <a href="${masked("mega.nz", 3)}">MEGA</a><br>
        <b>Android</b>: <a href="https://pixeldrain.com/u/and123">PIXELDRAIN</a><br>
        <div class="bbImageWrapper"><a href="https://attachments.f95zone.to/x.png"><img src="x.png"></a></div>`),
    );
    expect(r.kind).toBe("ok");
    expect(r.logged_in).toBe(true);
    expect(r.username).toBe("Tester");
    expect(r.groups!.map((g) => [g.label, g.os, g.links.map((l) => l.label)])).toEqual([
      ["Win/Linux", ["windows", "linux"], ["MEGA", "PIXELDRAIN", "GOFILE"]],
      ["Mac", ["mac"], ["MEGA"]],
      ["Android", ["android"], ["PIXELDRAIN"]],
    ]);
    const win = r.groups![0].links;
    expect(win[0]).toMatchObject({ host: "mega.nz", masked: true });
    expect(win[2]).toMatchObject({ host: "gofile.io", masked: false, url: "https://gofile.io/d/xyz" });
  });

  it("ignores links before the DOWNLOAD heading and in replies", () => {
    const r = parse(page(`<a href="https://pixeldrain.com/u/early">Old build</a><br><b>DOWNLOAD</b><br>Win: <a href="https://pixeldrain.com/u/win">PIXELDRAIN</a>`));
    const urls = r.groups!.flatMap((g) => g.links.map((l) => l.url));
    expect(urls).toEqual(["https://pixeldrain.com/u/win"]);
  });

  it("uses spoiler titles and label-only lines", () => {
    const r = parse(
      page(`<b>DOWNLOAD</b><br>
        Windows:<br><a href="https://pixeldrain.com/u/w1">PIXELDRAIN</a> - <a href="https://mega.nz/file/w2">MEGA</a><br>
        <div class="bbCodeSpoiler"><button class="bbCodeSpoiler-button"><span class="bbCodeSpoiler-button-title">Extras</span></button>
          <div class="bbCodeSpoiler-content"><a href="https://gofile.io/d/patch">Walkthrough mod</a></div></div>`),
    );
    expect(r.groups!.map((g) => [g.label, g.links.length])).toEqual([
      ["Windows", 2],
      ["Extras", 1],
    ]);
  });

  it("without a heading, only takes known hosts and masked links", () => {
    const r = parse(page(`Get it here: <a href="https://example.com/blog">blog</a> <a href="https://pixeldrain.com/u/a">PD</a> <a href="${masked("mega.nz", 9)}">MEGA</a>`));
    expect(r.groups!.flatMap((g) => g.links.map((l) => l.host))).toEqual(["pixeldrain.com", "mega.nz"]);
  });

  it("recognises Cloudflare checks, rate limits and guests", () => {
    expect(parse(page("", { title: "Just a moment..." })).kind).toBe("challenge");
    expect(parse(page("", { title: "Error 1015 | Rate limited" })).kind).toBe("rate_limited");
    const guest = new DOMParser().parseFromString(
      `<html data-logged-in="false"><head><title>Log in</title></head><body><form action="/login/login"></form></body></html>`,
      "text/html",
    );
    expect(parse(guest).kind).toBe("login_required");
    const r = parse(page(`<b>DOWNLOAD</b><br>Win: <a href="${masked("mega.nz", 1)}">MEGA</a>`, { loggedIn: false }));
    expect(r.logged_in).toBe(false);
    expect(r.username).toBeNull();
  });

  it("drops non-web and internal links", () => {
    const r = parse(
      page(`<b>DOWNLOAD</b><br>Win: <a href="javascript:alert(1)">X</a> <a href="https://f95zone.to/threads/123/">thread</a> <a href="https://pixeldrain.com/u/ok">PD</a>`),
    );
    expect(r.groups!.flatMap((g) => g.links.map((l) => l.url))).toEqual(["https://pixeldrain.com/u/ok"]);
  });
});
