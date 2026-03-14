import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Cthulu — AI agents that do the work you keep putting off",
  description:
    "Sandboxed AI agents with full filesystem and tool access. Automate PR reviews, content creation, and more — defined in JSON, deployed via git.",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>
        <a
          href="#main-content"
          className="sr-only focus:not-sr-only focus:fixed focus:top-4 focus:left-4 focus:z-50 focus:rounded-lg focus:bg-[var(--accent)] focus:px-4 focus:py-2 focus:text-[var(--primary-foreground)]"
        >
          Skip to content
        </a>
        {children}
      </body>
    </html>
  );
}
