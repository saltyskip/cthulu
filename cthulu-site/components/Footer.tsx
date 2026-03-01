export default function Footer() {
  return (
    <footer className="border-t border-border">
      {/* AI-built banner */}
      <div className="border-b border-border bg-bg-secondary px-6 py-8 text-center">
        <p className="text-sm font-medium uppercase tracking-widest text-accent">
          Yes, this entire site was built by Cthulu.
        </p>
        <p className="mt-1 text-sm text-text-secondary">
          The copy. The code. The design. No, we&apos;re not sorry.
        </p>
      </div>

      <div className="px-6 py-10">
        <div className="mx-auto flex max-w-6xl flex-col items-center justify-between gap-4 sm:flex-row">
          <div className="flex items-center gap-3">
            <div
              className="h-8 w-8"
              style={{
                WebkitMaskImage: "url(/cthulu-logo.png)",
                WebkitMaskSize: "150%",
                WebkitMaskRepeat: "no-repeat",
                WebkitMaskPosition: "center",
                maskImage: "url(/cthulu-logo.png)",
                maskSize: "150%",
                maskRepeat: "no-repeat",
                maskPosition: "center",
                backgroundColor: "var(--accent)",
              }}
            />
            <span className="text-lg font-bold text-text">Cthulu</span>
            <span className="text-sm text-text-secondary">
              Agent Orchestration Platform
            </span>
          </div>
        </div>
      </div>
    </footer>
  );
}
