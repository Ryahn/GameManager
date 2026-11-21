/** The catalog's content types and the words the UI uses for each. */
export type Kind = "games" | "comics" | "animations";

export const KINDS: { value: Kind; label: string; one: string; many: string }[] = [
  { value: "games", label: "Games", one: "game", many: "games" },
  { value: "comics", label: "Comics", one: "comic", many: "comics" },
  { value: "animations", label: "Animations", one: "animation", many: "animations" },
];

export function kindOf(category: string | null | undefined): Kind {
  const c = (category ?? "").toLowerCase();
  if (c.startsWith("comic")) return "comics";
  if (c.startsWith("animation")) return "animations";
  return "games";
}

export const kindMeta = (k: Kind) => KINDS.find((x) => x.value === k)!;

/** "3 comics", "1 animation". */
export function countLabel(n: number, k: Kind) {
  const m = kindMeta(k);
  return `${n} ${n === 1 ? m.one : m.many}`;
}
