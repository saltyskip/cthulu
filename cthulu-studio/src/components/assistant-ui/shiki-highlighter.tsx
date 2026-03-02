"use client";

import { FC, useState, useCallback } from "react";
import ShikiHighlighter, { type ShikiHighlighterProps } from "react-shiki";
import type { SyntaxHighlighterProps as AUIProps } from "@assistant-ui/react-markdown";
import { cn } from "@/lib/utils";
import { useTheme } from "@/lib/ThemeContext";

/**
 * Props for the SyntaxHighlighter component
 */
export type HighlighterProps = Omit<
  ShikiHighlighterProps,
  "children" | "theme"
> & {
  theme?: ShikiHighlighterProps["theme"];
} & Pick<AUIProps, "language" | "code"> &
  Partial<Pick<AUIProps, "node" | "components">>;

export const SyntaxHighlighter: FC<HighlighterProps> = ({
  code,
  language,
  theme: themeProp,
  className,
  addDefaultStyles = false,
  showLanguage = false,
  node: _node,
  components: _components,
  ...props
}) => {
  const { theme: appTheme } = useTheme();
  const shiki = appTheme.shikiTheme;
  const theme = themeProp ?? { dark: shiki as string, light: shiki as string };
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(code.trim()).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }, [code]);

  return (
    <div className="fr-code-block">
      <div className="fr-code-header">
        {language && <span className="fr-code-lang">{language}</span>}
        <button className="fr-code-copy" onClick={handleCopy}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <ShikiHighlighter
        {...props}
        language={language}
        theme={theme}
        addDefaultStyles={addDefaultStyles}
        showLanguage={showLanguage}
        defaultColor="light-dark()"
        className={cn(
          "aui-shiki-base [&_pre]:overflow-x-auto [&_pre]:rounded-b-lg [&_pre]:p-4 [&_pre]:border [&_pre]:border-[var(--border)] [&_pre]:!rounded-t-none [&_pre]:!border-t-0",
          className,
        )}
      >
        {code.trim()}
      </ShikiHighlighter>
    </div>
  );
};

SyntaxHighlighter.displayName = "SyntaxHighlighter";
