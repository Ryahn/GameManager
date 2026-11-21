/**
 * BBCode parser for F95 thread text (overviews, changelogs).
 *
 * Produces a small tree that the <BBCode> component turns into React
 * elements, so nothing is ever injected as HTML. It is forgiving the way
 * forum posts need it to be:
 * - unknown tags and stray brackets ("[v0.2]", "[Ren'Py]") stay as text
 * - unclosed tags close at the end; a closing tag closes anything left open
 *   inside it
 * - [code]/[icode]/[plain] contents are taken literally
 */

export type BBNode = string | BBElement;

export interface BBElement {
  tag: string;
  arg?: string;
  children: BBNode[];
}

/** Tags we understand. Anything else is shown as typed. */
const KNOWN = new Set([
  "b",
  "i",
  "u",
  "s",
  "url",
  "img",
  "color",
  "size",
  "font",
  "center",
  "left",
  "right",
  "justify",
  "indent",
  "quote",
  "spoiler",
  "ispoiler",
  "list",
  "*",
  "code",
  "icode",
  "php",
  "html",
  "plain",
  "heading",
  "hr",
  "user",
  "attach",
  "media",
  "sup",
  "sub",
  "table",
  "tr",
  "td",
  "th",
]);

/** Contents are not parsed further. */
const LITERAL = new Set(["code", "icode", "php", "html", "plain", "attach", "media"]);
/** Tags with no closing pair. */
const VOID = new Set(["hr"]);

const MAX_INPUT = 200_000;
const MAX_DEPTH = 24;

const TAG_RE = /\[(\/?)([a-z0-9*]+)(?:=("[^"\]]*"|'[^'\]]*'|[^\]]*))?\]/gi;

function cleanArg(a: string | undefined): string | undefined {
  if (a === undefined) return undefined;
  const t = a.trim();
  return (t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'")) ? t.slice(1, -1) : t;
}

export function parseBBCode(input: string): BBNode[] {
  const src = input.length > MAX_INPUT ? input.slice(0, MAX_INPUT) : input;
  const root: BBElement = { tag: "root", children: [] };
  const stack: BBElement[] = [root];
  const top = () => stack[stack.length - 1];

  const pushText = (text: string) => {
    if (!text) return;
    const kids = top().children;
    const last = kids[kids.length - 1];
    if (typeof last === "string") kids[kids.length - 1] = last + text;
    else kids.push(text);
  };

  let pos = 0;
  TAG_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TAG_RE.exec(src))) {
    const [raw, slash, name] = m;
    const tag = name.toLowerCase();
    pushText(src.slice(pos, m.index));
    pos = m.index + raw.length;

    if (!KNOWN.has(tag)) {
      pushText(raw);
      continue;
    }

    if (slash) {
      // Close the nearest matching tag and anything opened inside it.
      // [/list] also ends an open [*].
      const idx = stack.map((n) => n.tag).lastIndexOf(tag);
      if (idx > 0) stack.length = idx;
      else pushText(raw);
      continue;
    }

    if (VOID.has(tag)) {
      top().children.push({ tag, children: [] });
      continue;
    }

    if (tag === "*") {
      // A list item ends at the next [*].
      if (top().tag === "*") stack.pop();
      if (!stack.some((n) => n.tag === "list")) {
        pushText(raw);
        continue;
      }
    }

    const el: BBElement = { tag, arg: cleanArg(m[3]), children: [] };

    if (LITERAL.has(tag)) {
      const close = new RegExp(`\\[/${tag}\\]`, "i");
      const rest = src.slice(pos);
      const end = rest.search(close);
      const body = end === -1 ? rest : rest.slice(0, end);
      if (body) el.children.push(body);
      top().children.push(el);
      pos += end === -1 ? rest.length : end + `[/${tag}]`.length;
      TAG_RE.lastIndex = pos;
      continue;
    }

    if (stack.length > MAX_DEPTH) {
      pushText(raw);
      continue;
    }
    top().children.push(el);
    stack.push(el);
  }
  pushText(src.slice(pos));
  return root.children;
}

/** Plain text of a subtree (for [url]x[/url] and [img] bodies). */
export function textOf(nodes: BBNode[]): string {
  return nodes.map((n) => (typeof n === "string" ? n : textOf(n.children))).join("");
}

/** Only web links; never javascript:, file: or app-internal schemes. */
export function safeUrl(raw: string | undefined): string | null {
  const t = (raw ?? "").trim();
  if (!t) return null;
  const withScheme = /^www\./i.test(t) ? `https://${t}` : t;
  try {
    const u = new URL(withScheme);
    return u.protocol === "http:" || u.protocol === "https:" ? u.toString() : null;
  } catch {
    return null;
  }
}

const NAMED: Record<string, string> = {
  black: "#000000",
  white: "#ffffff",
};

function luminance(hex: string): number | null {
  let h = hex.replace("#", "");
  if (h.length === 3 || h.length === 4) h = [...h.slice(0, 3)].map((c) => c + c).join("");
  if (h.length !== 6 && h.length !== 8) return null;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * A colour safe to put in a style attribute, or null. Near-black and
 * near-white are dropped: posts are written for F95's own background and
 * would vanish on one of our themes.
 */
export function safeColor(raw: string | undefined): string | null {
  const t = (raw ?? "").trim().toLowerCase();
  const rgb = t.match(/^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*[\d.]+\s*)?\)$/);
  let hex: string | null = null;
  if (/^#[0-9a-f]{3,8}$/.test(t)) hex = t;
  else if (rgb) hex = "#" + rgb.slice(1, 4).map((n) => Math.min(255, Number(n)).toString(16).padStart(2, "0")).join("");
  else if (NAMED[t]) hex = NAMED[t];
  else if (/^[a-z]{3,20}$/.test(t)) return t;
  if (!hex) return null;
  const l = luminance(hex);
  if (l === null || l < 0.06 || l > 0.94) return null;
  return hex;
}

/** [SIZE=n] (1-7) or px values, as a clamped em size. */
export function sizeEm(raw: string | undefined): number | null {
  const t = (raw ?? "").trim().toLowerCase();
  const steps: Record<string, number> = { "1": 0.75, "2": 0.875, "3": 1, "4": 1.125, "5": 1.25, "6": 1.5, "7": 1.75 };
  if (steps[t]) return steps[t];
  const px = t.match(/^(\d{1,3})(?:px)?$/);
  if (px) return Math.min(1.75, Math.max(0.75, Number(px[1]) / 15));
  return null;
}

const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " ", "#39": "'" };

/** Decodes the few HTML entities that leak into API text. */
export function decodeEntities(s: string): string {
  return s.replace(/&(amp|lt|gt|quot|apos|nbsp|#39|#\d{1,6}|#x[0-9a-f]{1,6});/gi, (all, e: string) => {
    const k = e.toLowerCase();
    if (ENTITIES[k]) return ENTITIES[k];
    const code = k.startsWith("#x") ? parseInt(k.slice(2), 16) : k.startsWith("#") ? parseInt(k.slice(1), 10) : NaN;
    return Number.isFinite(code) && code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : all;
  });
}
