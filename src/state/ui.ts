import { create } from "zustand";
import { persist } from "zustand/middleware";

export type LibraryView = "grid" | "list";

interface UiState {
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  paletteOpen: boolean;
  setPaletteOpen: (v: boolean) => void;
  libraryView: LibraryView;
  setLibraryView: (v: LibraryView) => void;
  cardSize: number;
  setCardSize: (v: number) => void;
}

/** Per-device UI conveniences. Real data lives in Rust/the API. */
export const useUi = create<UiState>()(
  persist(
    (set) => ({
      sidebarCollapsed: false,
      toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
      paletteOpen: false,
      setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
      libraryView: "grid",
      setLibraryView: (libraryView) => set({ libraryView }),
      cardSize: 240,
      setCardSize: (cardSize) => set({ cardSize }),
    }),
    {
      name: "f95gm-ui",
      partialize: ({ sidebarCollapsed, libraryView, cardSize }) => ({ sidebarCollapsed, libraryView, cardSize }),
    },
  ),
);
