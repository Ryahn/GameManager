// Reads the download links out of an F95 thread's first post.
//
// Runs inside the F95 page (injected by the launcher with eval, never with
// access to the app), and is unit-tested from vitest with saved HTML
// (src/lib/opParser.test.ts). Plain ES2019, no dependencies.
//
// Returns one of:
//   { kind: "ok", logged_in, username, groups: [{ label, os: [...], links: [{ host, url, label, masked }] }] }
//   { kind: "challenge" }       Cloudflare check: the user has to pass it
//   { kind: "rate_limited" }    F95 or Cloudflare asked us to slow down
//   { kind: "login_required" }  links are hidden from guests
//   { kind: "not_found", logged_in, username }  no first post (error page, or the home page)

// eslint-disable-next-line no-unused-vars
function f95gmParse(doc) {
  var title = (doc.title || "").toLowerCase();
  var html = doc.documentElement;

  if (
    /just a moment|attention required|checking your browser/.test(title) ||
    doc.querySelector("#challenge-form, #challenge-running, #cf-challenge-running, .cf-turnstile, [data-translate='checking_browser']")
  ) {
    return { kind: "challenge" };
  }
  if (/\b429\b|too many requests|rate limited|error 1015/.test(title)) {
    return { kind: "rate_limited" };
  }

  var loggedIn = html.getAttribute("data-logged-in") === "true";
  var userEl = doc.querySelector(".p-navgroup-link--user .p-navgroup-linkText, .p-navgroup-link--user");
  var username = loggedIn && userEl ? (userEl.textContent || "").trim() || null : null;

  var post = doc.querySelector("article.message--post .message-body .bbWrapper, .message-threadStarterPost .bbWrapper, article.message .bbWrapper");
  if (!post) {
    if (doc.querySelector("form[action*='login'], .blockMessage--error") && !loggedIn) return { kind: "login_required", logged_in: false, username: null };
    // Also how the session check reads the home page: no post, just who's signed in.
    return { kind: "not_found", logged_in: loggedIn, username: username };
  }

  // ---- flatten the post into tokens, in reading order
  var BLOCK = /^(DIV|P|LI|UL|OL|BLOCKQUOTE|H[1-6]|TABLE|TR|TD|HR|SECTION|ARTICLE)$/;
  var tokens = [];
  function walk(node, spoiler) {
    for (var c = node.firstChild; c; c = c.nextSibling) {
      if (c.nodeType === 3) {
        tokens.push({ t: "text", v: c.nodeValue || "", spoiler: spoiler });
        continue;
      }
      if (c.nodeType !== 1) continue;
      var tag = c.tagName;
      if (tag === "SCRIPT" || tag === "STYLE" || tag === "NOSCRIPT") continue;
      if (tag === "BR") {
        tokens.push({ t: "br" });
        continue;
      }
      if (c.classList && c.classList.contains("bbCodeSpoiler-button")) continue;
      if (c.classList && c.classList.contains("bbCodeSpoiler")) {
        var titleEl = c.querySelector(".bbCodeSpoiler-button-title");
        var st = titleEl ? (titleEl.textContent || "").trim() : "";
        tokens.push({ t: "br" });
        walk(c, st || spoiler);
        tokens.push({ t: "br" });
        continue;
      }
      if (tag === "A") {
        tokens.push({ t: "link", href: c.getAttribute("href") || "", text: (c.textContent || "").trim(), hasImg: !!c.querySelector("img"), spoiler: spoiler });
        continue;
      }
      if (tag === "IMG") continue;
      var block = BLOCK.test(tag);
      if (block) tokens.push({ t: "br" });
      walk(c, spoiler);
      if (block) tokens.push({ t: "br" });
    }
  }
  walk(post, "");

  // ---- start after the DOWNLOAD heading (if there is one)
  var start = -1;
  for (var i = 0; i < tokens.length; i++) {
    if (tokens[i].t === "text" && /\bDOWNLOADS?\b/.test(tokens[i].v)) {
      start = i + 1;
      break;
    }
  }
  var strict = start === -1; // no heading: only accept links that clearly are downloads
  if (start === -1) start = 0;

  var KNOWN_HOSTS = /(^|\.)(mega\.nz|pixeldrain\.com|gofile\.io|buzzheavier\.com|mediafire\.com|workupload\.com|anonfiles\.com|uploadhaven\.com|catbox\.moe|drive\.google\.com|1drv\.ms|dropbox\.com|qiwi\.gg|files\.fm|krakenfiles\.com|racaty\.(io|net)|nopy\.to|swisstransfer\.com|datanodes\.to|trashbytes\.net|fileditch\.com|nitroflare\.com|rapidgator\.net|uploaded\.net|zippyshare\.com)$/i;

  function resolve(href) {
    var u;
    try {
      u = new URL(href, "https://f95zone.to/");
    } catch (e) {
      return null;
    }
    if (u.protocol !== "https:" && u.protocol !== "http:") return null;
    var host = u.hostname.replace(/^www\./, "").toLowerCase();
    if (host === "f95zone.to" || host.endsWith(".f95zone.to")) {
      var m = u.pathname.match(/^\/masked\/([^/]+)/);
      if (!m) return null; // threads, attachments, members: not downloads
      return { url: u.toString(), host: m[1].replace(/^www\./, "").toLowerCase(), masked: true };
    }
    if (strict && !KNOWN_HOSTS.test(host)) return null;
    return { url: u.toString(), host: host, masked: false };
  }

  function osOf(label) {
    var l = label.toLowerCase();
    var os = [];
    if (/\b(win|windows|pc)\b/.test(l)) os.push("windows");
    if (/\blinux\b/.test(l)) os.push("linux");
    if (/\b(mac|macos|osx|os x)\b/.test(l)) os.push("mac");
    if (/\bandroid\b|\bapk\b/.test(l)) os.push("android");
    return os;
  }

  function cleanLabel(s) {
    return s
      .replace(/\s+/g, " ")
      .replace(/^[\s\-–—:|•*]+|[\s\-–—:|•*]+$/g, "")
      .trim();
  }

  // ---- read line by line: "Win/Linux: MEGA - PIXELDRAIN - GOFILE"
  var groups = [];
  var byLabel = {};
  var seen = {};
  var lineText = "";
  var lineLinks = [];
  var lastLabel = "";

  function flush() {
    if (lineLinks.length) {
      var label = cleanLabel(lineText.split(":")[0]);
      // A line of only links continues the label above it (or its spoiler).
      if (!label || label.length > 60) label = lineLinks[0].spoiler || lastLabel || "Download";
      lastLabel = label;
      var g = byLabel[label];
      if (!g) {
        g = byLabel[label] = { label: label, os: osOf(label), links: [] };
        groups.push(g);
      }
      for (var k = 0; k < lineLinks.length; k++) g.links.push(lineLinks[k].link);
    } else if (cleanLabel(lineText)) {
      // A label on its own line ("Windows:") followed by links on the next.
      var t = cleanLabel(lineText.split(":")[0]);
      if (t && t.length <= 60 && /:\s*$/.test(lineText.trim())) lastLabel = t;
    }
    lineText = "";
    lineLinks = [];
  }

  for (var j = start; j < tokens.length; j++) {
    var tok = tokens[j];
    if (tok.t === "br") {
      flush();
    } else if (tok.t === "text") {
      if (!lineLinks.length) lineText += tok.v;
    } else if (tok.t === "link") {
      if (tok.hasImg && !tok.text) continue; // screenshot thumbnails
      var r = resolve(tok.href);
      if (!r || seen[r.url]) continue;
      seen[r.url] = true;
      lineLinks.push({ spoiler: tok.spoiler, link: { host: r.host, url: r.url, label: tok.text || r.host, masked: r.masked } });
    }
  }
  flush();

  return { kind: "ok", logged_in: loggedIn, username: username, groups: groups };
}
