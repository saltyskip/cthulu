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
      <body>{children}</body>
    </html>
  );
}
