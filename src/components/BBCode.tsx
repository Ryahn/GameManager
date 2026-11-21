/**
 * Renders F95 BBCode as React elements. Never uses innerHTML: every tag maps
 * to a known element, links only open http(s) in the system browser, and
 * images only load through the rate-limited image cache.
 */
import { memo, useMemo, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ChevronRight, ImageOff, Paperclip, PlayCircle } from "lucide-react";
import { type BBElement, type BBNode, decodeEntities, parseBBCode, safeColor, safeUrl, sizeEm, textOf } from "@/lib/bbcode";
import { cn, imageSrc } from "@/lib/utils";

const BLOCK = new Set(["center", "left", "right", "justify", "indent", "quote", "spoiler", "list", "code", "php", "html", "heading", "hr", "table"]);

function isBlock(n: BBNode | undefined): boolean {
  return typeof n !== "string" && !!n && BLOCK.has(n.tag);
}

/**
 * Blocks already start on their own line, so the newline typed next to one
 * would otherwise show up as an extra blank line.
 */
function tidy(children: BBNode[], parent: string): BBNode[] {
  const out: BBNode[] = [];
  children.forEach((c, i) => {
    if (typeof c !== "string") return out.push(c);
    let t = c;
    if (parent === "list" || parent === "table" || parent === "tr") {
      if (!t.trim()) return; // whitespace between items
    }
    if (parent === "*") t = t.replace(/^\s*\n/, "").replace(/\n\s*$/, "");
    // (A newline just before a block is already swallowed by the browser.)
    if (isBlock(children[i - 1])) t = t.replace(/^\r?\n/, "");
    // "[SPOILER]\ntext\n[/SPOILER]": the newlines just inside a block tag.
    if (BLOCK.has(parent) || parent === "ispoiler") {
      if (i === 0) t = t.replace(/^\r?\n/, "");
      if (i === children.length - 1) t = t.replace(/\r?\n$/, "");
    }
    if (t) out.push(t);
  });
  return out;
}

const URL_IN_TEXT = /(https?:\/\/[^\s<>"'[\]]+[^\s<>"'[\].,;:!?)])/gi;

function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      title={href}
      onClick={(e) => {
        e.preventDefault();
        void openUrl(href);
      }}
      className="break-words text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent"
    >
      {children}
    </a>
  );
}

/** Plain text with bare links made clickable. */
function Text({ value, linkify }: { value: string; linkify: boolean }) {
  const text = decodeEntities(value);
  if (!linkify) return <>{text}</>;
  const parts = text.split(URL_IN_TEXT);
  return (
    <>
      {parts.map((p, i) => {
        const href = i % 2 === 1 ? safeUrl(p) : null;
        return href ? (
          <ExternalLink key={i} href={href}>
            {p}
          </ExternalLink>
        ) : (
          p
        );
      })}
    </>
  );
}

function BBImage({ src, alt }: { src: string; alt?: string }) {
  const [failed, setFailed] = useState(false);
  if (failed) {
    return (
      <ExternalLink href={src}>
        <ImageOff className="mr-1 inline size-3.5 align-[-2px]" />
        {alt || "Image"}
      </ExternalLink>
    );
  }
  return (
    <img
      src={imageSrc(src)}
      alt={alt ?? ""}
      loading="lazy"
      decoding="async"
      onError={() => setFailed(true)}
      className="my-2 inline-block max-h-[480px] max-w-full rounded-lg border border-border align-middle"
    />
  );
}

function Spoiler({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <details className="group my-2 rounded-lg border border-border bg-surface-2/60 [&[open]>summary_svg]:rotate-90">
      <summary className="flex cursor-default list-none items-center gap-1.5 px-3 py-2 text-[13px] font-medium text-muted hover:text-text [&::-webkit-details-marker]:hidden">
        <ChevronRight className="size-4 transition-transform" />
        Spoiler{title ? `: ${title}` : ""}
      </summary>
      <div className="border-t border-border px-3 py-2.5">{children}</div>
    </details>
  );
}

function render(nodes: BBNode[], parent: string, linkify: boolean): ReactNode[] {
  return tidy(nodes, parent).map((n, i) => {
    if (typeof n === "string") return <Text key={i} value={n} linkify={linkify} />;
    return <Element key={i} el={n} linkify={linkify} />;
  });
}

function Element({ el, linkify }: { el: BBElement; linkify: boolean }): ReactNode {
  const kids = () => render(el.children, el.tag, linkify);
  const text = () => decodeEntities(textOf(el.children));

  switch (el.tag) {
    case "b":
      return <strong className="font-semibold text-text">{kids()}</strong>;
    case "i":
      return <em>{kids()}</em>;
    case "u":
      return <span className="underline underline-offset-2">{kids()}</span>;
    case "s":
      return <s className="opacity-70">{kids()}</s>;
    case "sup":
      return <sup>{kids()}</sup>;
    case "sub":
      return <sub>{kids()}</sub>;
    case "font":
    case "user":
      return <>{kids()}</>;
    case "color": {
      const c = safeColor(el.arg);
      return c ? <span style={{ color: c }}>{kids()}</span> : <>{kids()}</>;
    }
    case "size": {
      const em = sizeEm(el.arg);
      return em ? <span style={{ fontSize: `${em}em`, lineHeight: 1.45 }}>{kids()}</span> : <>{kids()}</>;
    }
    case "url": {
      const href = safeUrl(el.arg ?? textOf(el.children));
      // A bare [url] with a link inside shouldn't double-linkify its text.
      const body = el.children.length ? render(el.children, "url", false) : href;
      return href ? <ExternalLink href={href}>{body}</ExternalLink> : <>{kids()}</>;
    }
    case "img": {
      const src = safeUrl(text());
      return src ? <BBImage src={src} alt={el.arg} /> : null;
    }
    case "center":
    case "right":
    case "justify":
      return <div className={cn(el.tag === "center" ? "text-center" : el.tag === "right" ? "text-right" : "text-justify")}>{kids()}</div>;
    case "left":
      return <div>{kids()}</div>;
    case "indent":
      return <div className="pl-6">{kids()}</div>;
    case "heading": {
      const level = Number(el.arg) || 2;
      const cls = level <= 1 ? "text-[1.3em]" : level === 2 ? "text-[1.15em]" : "text-[1.05em]";
      return <div className={cn("mb-1 mt-3 font-semibold text-text", cls)}>{kids()}</div>;
    }
    case "hr":
      return <hr className="my-3 border-border" />;
    case "quote": {
      const who = el.arg?.split(",")[0]?.trim();
      return (
        <blockquote className="my-2 rounded-r-lg border-l-[3px] border-accent/60 bg-surface-2/60 px-3 py-2">
          {who && <p className="mb-1 text-[12.5px] font-semibold text-muted">{who} said:</p>}
          {kids()}
        </blockquote>
      );
    }
    case "spoiler":
    case "ispoiler":
      return el.tag === "ispoiler" ? (
        <span className="rounded bg-text/80 px-0.5 text-transparent transition-colors hover:bg-transparent hover:text-inherit" title="Spoiler">
          {kids()}
        </span>
      ) : (
        <Spoiler title={el.arg}>{kids()}</Spoiler>
      );
    case "list": {
      const ordered = !!el.arg;
      const items = tidy(el.children, "list");
      const Tag = ordered ? "ol" : "ul";
      return (
        <Tag className={cn("my-1.5 space-y-0.5 pl-6", ordered ? "list-decimal" : "list-disc")}>
          {items.map((it, i) =>
            typeof it !== "string" && it.tag === "*" ? (
              <li key={i}>{render(it.children, "*", linkify)}</li>
            ) : (
              <li key={i} className="list-none">
                {typeof it === "string" ? <Text value={it} linkify={linkify} /> : <Element el={it} linkify={linkify} />}
              </li>
            ),
          )}
        </Tag>
      );
    }
    case "*":
      return <>{kids()}</>;
    case "code":
    case "php":
    case "html":
      return (
        <pre className="my-2 overflow-x-auto rounded-lg border border-border bg-surface-2 p-3 font-mono text-[12.5px] leading-relaxed text-text">
          {text().replace(/^\r?\n/, "")}
        </pre>
      );
    case "icode":
      return <code className="rounded bg-surface-3 px-1 py-0.5 font-mono text-[0.9em] text-text">{text()}</code>;
    case "plain":
      return <>{text()}</>;
    case "attach":
      return (
        <span className="inline-flex items-center gap-1 text-faint">
          <Paperclip className="size-3.5" /> Attachment
        </span>
      );
    case "media": {
      const id = text().trim();
      const href = el.arg?.toLowerCase() === "youtube" && /^[\w-]{6,20}$/.test(id) ? `https://www.youtube.com/watch?v=${id}` : null;
      return href ? (
        <ExternalLink href={href}>
          <PlayCircle className="mr-1 inline size-4 align-[-3px]" />
          Watch video
        </ExternalLink>
      ) : (
        <span className="text-faint">Embedded media</span>
      );
    }
    case "table":
      return (
        <div className="my-2 overflow-x-auto">
          <table className="border-collapse text-[0.95em]">
            <tbody>{kids()}</tbody>
          </table>
        </div>
      );
    case "tr":
      return <tr>{kids()}</tr>;
    case "td":
      return <td className="border border-border px-2 py-1 align-top">{kids()}</td>;
    case "th":
      return <th className="border border-border bg-surface-2 px-2 py-1 text-left font-semibold">{kids()}</th>;
    default:
      return <>{kids()}</>;
  }
}

/** Forum text (BBCode) rendered safely. Line breaks are kept as written. */
export const BBCode = memo(function BBCode({
  text,
  className,
  linkify = true,
}: {
  text: string | null | undefined;
  className?: string;
  linkify?: boolean;
}) {
  const tree = useMemo(() => parseBBCode(text ?? ""), [text]);
  return (
    <div className={cn("whitespace-pre-wrap break-words", className)} data-selectable>
      {render(tree, "root", linkify)}
    </div>
  );
});
