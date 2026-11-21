// Adds "Add to F95 Game Manager" next to the thread title.
//
// The button is a plain f95gm:// link: the browser asks once to open the
// launcher, which adds the game and shows it. Nothing is sent anywhere by
// this script, and it needs no permissions.
(function () {
  "use strict";

  // /threads/some-game.12345/ or /threads/12345/
  const match = location.pathname.match(/^\/threads\/(?:[^/]*\.)?(\d+)\/?/);
  if (!match) return;
  const threadId = match[1];
  if (document.querySelector(".f95gm-add")) return;

  const title = document.querySelector(".p-title-value, h1.p-title-value, .p-title h1");
  if (!title) return;

  const link = document.createElement("a");
  link.className = "f95gm-add";
  link.href = `f95gm://add/${threadId}`;
  link.title = "Add this game to F95 Game Manager and track it for updates";
  link.textContent = "Add to F95 Game Manager";

  const host = document.querySelector(".p-title-pageAction") || title.parentElement;
  host.appendChild(link);
})();
