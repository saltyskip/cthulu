import { useState, useEffect, useRef } from "react";
import { getSingletonHighlighter } from "shiki";

export interface Token {
  content: string;
  color?: string;
}

/**
 * Tokenize code with shiki and return per-line token arrays.
 * Returns null while loading or if language is not provided.
 */
export function useShikiTokens(
  code: string | undefined,
  lang: string | undefined,
  theme: string | Record<string, unknown>,
): Token[][] | null {
  const [lines, setLines] = useState<Token[][] | null>(null);
  const reqRef = useRef(0);

  useEffect(() => {
    if (!code || !lang) {
      setLines(null);
      return;
    }

    const reqId = ++reqRef.current;

    (async () => {
      try {
        const themeName = typeof theme === "string" ? theme : (theme as { name?: string }).name || "github-dark";
        const highlighter = await getSingletonHighlighter({
          themes: typeof theme === "string" ? [theme] : [theme as Parameters<typeof getSingletonHighlighter>[0] extends { themes?: (infer T)[] } ? T : never],
          langs: [lang as Parameters<typeof getSingletonHighlighter>[0] extends { langs?: (infer L)[] } ? L : never],
        });

        if (reqRef.current !== reqId) return; // stale

        const result = highlighter.codeToTokensBase(code, {
          lang: lang as never,
          theme: themeName as never,
        });

        if (reqRef.current !== reqId) return;

        setLines(
          result.map((line) =>
            line.map((t) => ({ content: t.content, color: t.color })),
          ),
        );
      } catch {
        if (reqRef.current === reqId) setLines(null);
      }
    })();
  }, [code, lang, theme]);

  return lines;
}
