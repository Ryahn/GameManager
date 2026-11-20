import type { ApiError } from "./ipc";

export interface FriendlyError {
  title: string;
  detail: string;
  /** What the user can do next, if anything. */
  action?: "sign_in" | "retry" | "wait";
}

function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "kind" in e;
}

/** Turns any thrown value into a message a person can act on. */
export function friendlyError(e: unknown): FriendlyError {
  if (!isApiError(e)) {
    return { title: "Something went wrong", detail: String(e ?? "Unknown error"), action: "retry" };
  }
  switch (e.kind) {
    case "not_signed_in":
      return { title: "You're signed out", detail: "Sign in to load the catalog and your library.", action: "sign_in" };
    case "unauthorized":
      return {
        title: "Sign-in didn't work",
        detail: e.message || "Your key or session is invalid or was revoked. Sign in again or create a new key.",
        action: "sign_in",
      };
    case "forbidden":
      return {
        title: "Your account can't do that yet",
        detail: e.message || "Your access may still be waiting for approval. Check your account on f95api.xyz.",
      };
    case "not_found":
      return { title: "Not in the catalog yet", detail: "This game hasn't been indexed yet. Try again later." };
    case "rate_limited": {
      const s = e.retry_in_secs;
      const when = s >= 90 ? `${Math.ceil(s / 60)} minutes` : `${s} seconds`;
      return {
        title: "Slowing down to protect your account",
        detail: `The server asked us to pause. We'll stop sending requests for about ${when}.`,
        action: "wait",
      };
    }
    case "network":
      return { title: "Can't reach the server", detail: "Check your internet connection, then try again.", action: "retry" };
    case "server":
      return { title: "The server had a problem", detail: `It answered ${e.status}. Try again in a moment.`, action: "retry" };
    case "invalid":
      return { title: "That didn't look right", detail: e.message };
    case "internal":
      return { title: "Something went wrong", detail: e.message, action: "retry" };
  }
}
