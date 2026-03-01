"use client";

import { FC } from "react";
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

/**
 * SyntaxHighlighter component, using react-shiki
 * Use it by passing to `defaultComponents` in `markdown-text.tsx`
 *
 * @example
 * const defaultComponents = memoizeMarkdownComponents({
 *   SyntaxHighlighter,
 *   h1: //...
 *   //...other elements...
 * });
 */
export const SyntaxHighlighter: FC<HighlighterProps> = ({
  code,
  language,
  theme: themeProp,
  className,
  addDefaultStyles = false, // assistant-ui requires custom base styles
  showLanguage = false, // assistant-ui/react-markdown handles language labels
  node: _node,
  components: _components,
  ...props
}) => {
  const { theme: appTheme } = useTheme();
  const shiki = appTheme.shikiTheme;
  const theme = themeProp ?? { dark: shiki as string, light: shiki as string };
  return (
    <ShikiHighlighter
      {...props}
      language={language}
      theme={theme}
      addDefaultStyles={addDefaultStyles}
      showLanguage={showLanguage}
      defaultColor="light-dark()"
      className={cn(
        "aui-shiki-base [&_pre]:overflow-x-auto [&_pre]:rounded-b-lg [&_pre]:p-4 [&_pre]:border [&_pre]:border-[var(--border)]",
        className,
      )}
    >
      {code.trim()}
    </ShikiHighlighter>
  );
};

SyntaxHighlighter.displayName = "SyntaxHighlighter";
