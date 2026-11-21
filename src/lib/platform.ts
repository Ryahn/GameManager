import { useAppInfo } from "@/hooks/queries";

export type Platform = "windows" | "macos" | "linux";

/** Words that differ per operating system. */
export function osWords(platform: Platform | undefined) {
  switch (platform) {
    case "macos":
      return { keychain: "the macOS Keychain", trash: "Trash", signIn: "Open at login", program: "app" };
    case "linux":
      return { keychain: "your system keyring", trash: "Trash", signIn: "Start when you sign in", program: "program" };
    default:
      return { keychain: "Windows Credential Manager", trash: "Recycle Bin", signIn: "Start with Windows", program: ".exe" };
  }
}

export function useOs() {
  const info = useAppInfo();
  const platform = (info.data?.platform ?? "windows") as Platform;
  return {
    platform,
    steamDeck: !!info.data?.steam_deck,
    pickExtensions: info.data?.pick_extensions ?? ["exe", "html", "htm", "bat", "lnk"],
    ...osWords(platform),
  };
}
