import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from "react";
import { type ThemeDefinition, findTheme, getDefaultThemeId } from "./themes";

const STORAGE_KEY = "cthulu_theme";

interface ThemeContextValue {
  theme: ThemeDefinition;
  setThemeId: (id: string) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function applyTheme(def: ThemeDefinition) {
  const el = document.documentElement;

  // Set all CSS variables
  for (const [key, value] of Object.entries(def.vars)) {
    el.style.setProperty(`--${key}`, value);
  }

  // Theme metadata for selectors
  el.setAttribute("data-theme", def.id);

  // Native scrollbar / form control appearance
  el.style.colorScheme = def.colorScheme;

  // Tailwind dark: variant (shadcn components)
  el.classList.toggle("dark", def.colorScheme === "dark");
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [themeId, setThemeIdRaw] = useState(getDefaultThemeId);
  const theme = findTheme(themeId);

  const setThemeId = useCallback((id: string) => {
    localStorage.setItem(STORAGE_KEY, id);
    setThemeIdRaw(id);
  }, []);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  return (
    <ThemeContext.Provider value={{ theme, setThemeId }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
