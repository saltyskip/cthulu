import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import DashboardView from "../DashboardView";
import * as api from "../../api/client";

// --- Mock API client ---
vi.mock("../../api/client", () => ({
  getDashboardConfig: vi.fn(),
  saveDashboardConfig: vi.fn(),
  getDashboardMessages: vi.fn(),
  getDashboardSummary: vi.fn(),
}));

// --- Mock UI components ---
vi.mock("@/components/ui/dialog", () => ({
  Dialog: ({ children, open }: any) => (open ? <div data-testid="dialog">{children}</div> : null),
  DialogContent: ({ children }: any) => <div>{children}</div>,
  DialogHeader: ({ children }: any) => <div>{children}</div>,
  DialogTitle: ({ children }: any) => <div>{children}</div>,
  DialogFooter: ({ children }: any) => <div>{children}</div>,
}));

vi.mock("@/components/ui/button", () => ({
  Button: ({ children, ...props }: any) => <button {...props}>{children}</button>,
}));

const mockConfig = (overrides: Partial<api.DashboardConfig> = {}): api.DashboardConfig => ({
  channels: ["general", "engineering"],
  slack_token_env: "SLACK_USER_TOKEN",
  first_run: false,
  ...overrides,
});

const mockMessages = (overrides: Partial<api.DashboardMessages> = {}): api.DashboardMessages => ({
  channels: [
    {
      channel: "general",
      count: 2,
      messages: [
        { time: "2026-03-20T09:00:00Z", user: "alice", text: "Hello team!", ts: "1742461200.000100" },
        { time: "2026-03-20T09:15:00Z", user: "bob", text: "Morning!", ts: "1742462100.000200" },
      ],
    },
    {
      channel: "engineering",
      count: 1,
      messages: [
        {
          time: "2026-03-20T10:00:00Z",
          user: "charlie",
          text: "Deploy is green",
          ts: "1742464800.000300",
          reply_count: 1,
          replies: [
            { time: "2026-03-20T10:05:00Z", user: "dave", text: "Nice!", ts: "1742465100.000301" },
          ],
        },
      ],
    },
  ],
  fetched_at: "2026-03-20T10:30:00Z",
  ...overrides,
});

const mockSummaryResponse = (): api.DashboardSummaryResponse => ({
  summaries: [
    { channel: "#general", summary: "Team greeted each other. Productive start." },
    { channel: "#engineering", summary: "Deployment was successful." },
  ],
  generated_at: "2026-03-20T10:35:00Z",
});

describe("DashboardView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── Loading & First-Run ──────────────────────────────────

  it("shows loading state initially", async () => {
    // Config resolves after a delay
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockReturnValue(new Promise(() => {}));

    render(<DashboardView />);
    expect(screen.getByText(/Fetching messages from Slack/i)).toBeTruthy();
  });

  it("shows channel dialog on first run", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(
      mockConfig({ channels: [], first_run: true }),
    );

    render(<DashboardView />);
    await waitFor(() => {
      expect(screen.getByTestId("dialog")).toBeTruthy();
    });
    expect(screen.getByText("Slack Channels")).toBeTruthy();
  });

  it("shows empty state when no channels configured and not first run", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(
      mockConfig({ channels: [], first_run: false }),
    );

    render(<DashboardView />);
    await waitFor(() => {
      expect(screen.getByText(/No channels configured/i)).toBeTruthy();
    });
  });

  // ── Messages Display ────────────────────────────────────

  it("loads and displays messages grouped by channel", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(mockMessages());

    render(<DashboardView />);
    await waitFor(() => {
      expect(screen.getByText("general")).toBeTruthy();
      expect(screen.getByText("engineering")).toBeTruthy();
    });

    // Messages are displayed
    expect(screen.getByText("Hello team!")).toBeTruthy();
    expect(screen.getByText("Morning!")).toBeTruthy();
    expect(screen.getByText("Deploy is green")).toBeTruthy();
  });

  it("displays message count stats", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(mockMessages());

    render(<DashboardView />);
    await waitFor(() => {
      expect(screen.getByText(/3 messages across 2 channels/i)).toBeTruthy();
    });
  });

  it("shows thread replies with expand/collapse toggle", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(mockMessages());

    const user = userEvent.setup();

    render(<DashboardView />);
    await waitFor(() => {
      expect(screen.getByText("Deploy is green")).toBeTruthy();
    });

    // Thread toggle should be visible
    const toggle = screen.getByText(/1 reply/i);
    expect(toggle).toBeTruthy();

    // Reply not visible yet
    expect(screen.queryByText("Nice!")).toBeNull();

    // Click to expand
    await user.click(toggle);
    expect(screen.getByText("Nice!")).toBeTruthy();

    // Click to collapse
    await user.click(toggle);
    expect(screen.queryByText("Nice!")).toBeNull();
  });

  // ── Error Handling ──────────────────────────────────────

  it("shows error when message fetch fails", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("Network error"),
    );

    render(<DashboardView />);
    await waitFor(() => {
      expect(screen.getByText(/Network error/i)).toBeTruthy();
    });
  });

  it("shows error when config fetch fails", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("Config unreachable"),
    );

    render(<DashboardView />);
    await waitFor(() => {
      expect(screen.getByText(/Config unreachable/i)).toBeTruthy();
    });
  });

  // ── Summarize ───────────────────────────────────────────

  it("calls getDashboardSummary and displays results", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(mockMessages());
    (api.getDashboardSummary as ReturnType<typeof vi.fn>).mockResolvedValue(mockSummaryResponse());

    const user = userEvent.setup();
    render(<DashboardView />);

    await waitFor(() => {
      expect(screen.getByText("general")).toBeTruthy();
    });

    // Click Summarize
    const summarizeBtn = screen.getByText("Summarize");
    await user.click(summarizeBtn);

    await waitFor(() => {
      expect(api.getDashboardSummary).toHaveBeenCalledTimes(1);
    });

    // Summaries should be displayed
    await waitFor(() => {
      expect(screen.getByText(/Team greeted each other/i)).toBeTruthy();
      expect(screen.getByText(/Deployment was successful/i)).toBeTruthy();
    });
  });

  it("shows summary error when summarize fails", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(mockMessages());
    (api.getDashboardSummary as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("Claude timeout"),
    );

    const user = userEvent.setup();
    render(<DashboardView />);

    await waitFor(() => {
      expect(screen.getByText("Summarize")).toBeTruthy();
    });

    await user.click(screen.getByText("Summarize"));

    await waitFor(() => {
      expect(screen.getByText(/Claude timeout/i)).toBeTruthy();
    });
  });

  // ── Refresh ─────────────────────────────────────────────

  it("refresh button reloads messages and clears summaries", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(mockMessages());
    (api.getDashboardSummary as ReturnType<typeof vi.fn>).mockResolvedValue(mockSummaryResponse());

    const user = userEvent.setup();
    render(<DashboardView />);

    await waitFor(() => {
      expect(screen.getByText("general")).toBeTruthy();
    });

    // First summarize
    await user.click(screen.getByText("Summarize"));
    await waitFor(() => {
      expect(screen.getByText(/Team greeted each other/i)).toBeTruthy();
    });

    // Now refresh — summaries should clear, messages should reload
    await user.click(screen.getByText("Refresh"));

    await waitFor(() => {
      expect(api.getDashboardMessages).toHaveBeenCalledTimes(2); // initial + refresh
    });
  });

  // ── Greeting & Date ─────────────────────────────────────

  it("displays a time-of-day greeting", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(
      mockConfig({ channels: [], first_run: false }),
    );

    render(<DashboardView />);
    await waitFor(() => {
      // Should contain one of the greeting options
      const greeting = screen.getByRole("heading", { level: 1 });
      expect(
        greeting.textContent === "Good morning" ||
          greeting.textContent === "Good afternoon" ||
          greeting.textContent === "Good evening",
      ).toBe(true);
    });
  });

  // ── Channel Config Dialog ────────────────────────────────

  it("opens channel config dialog when Channels button clicked", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue(mockConfig());
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(mockMessages());

    const user = userEvent.setup();
    render(<DashboardView />);

    await waitFor(() => {
      expect(screen.getByText("Channels")).toBeTruthy();
    });

    await user.click(screen.getByText("Channels"));

    await waitFor(() => {
      expect(screen.getByTestId("dialog")).toBeTruthy();
      expect(screen.getByText("Slack Channels")).toBeTruthy();
    });
  });

  it("saves channel config and reloads messages", async () => {
    (api.getDashboardConfig as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce(mockConfig({ channels: [], first_run: true }))
      .mockResolvedValueOnce(mockConfig({ channels: ["devops"] }));
    (api.saveDashboardConfig as ReturnType<typeof vi.fn>).mockResolvedValue({ ok: true });
    (api.getDashboardMessages as ReturnType<typeof vi.fn>).mockResolvedValue(
      mockMessages({ channels: [{ channel: "devops", count: 0, messages: [] }] }),
    );

    const user = userEvent.setup();
    render(<DashboardView />);

    // Wait for first-run dialog
    await waitFor(() => {
      expect(screen.getByTestId("dialog")).toBeTruthy();
    });

    // Type channel name and save
    const input = screen.getByPlaceholderText(/general, devops/i);
    await user.type(input, "devops");
    await user.click(screen.getByText("Save"));

    await waitFor(() => {
      expect(api.saveDashboardConfig).toHaveBeenCalledWith(["devops"]);
    });
  });
});
