import { describe, it, expect } from "vitest";
import { computeDiffLines } from "./diff";

describe("computeDiffLines", () => {
  it("returns empty for two empty strings", () => {
    const result = computeDiffLines("", "");
    // Both split to [""], so we get one ctx line
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({ type: "ctx", text: "" });
  });

  it("shows all additions for empty old string", () => {
    const result = computeDiffLines("", "line1\nline2");
    const adds = result.filter((l) => l.type === "add");
    expect(adds).toHaveLength(2);
    expect(adds[0].text).toBe("line1");
    expect(adds[1].text).toBe("line2");
  });

  it("shows all deletions for empty new string", () => {
    const result = computeDiffLines("line1\nline2", "");
    const dels = result.filter((l) => l.type === "del");
    expect(dels).toHaveLength(2);
  });

  it("shows context lines for identical strings", () => {
    const result = computeDiffLines("a\nb\nc", "a\nb\nc");
    expect(result.every((l) => l.type === "ctx")).toBe(true);
    expect(result).toHaveLength(3);
  });

  it("detects a single line change", () => {
    const result = computeDiffLines("a\nb\nc", "a\nB\nc");
    expect(result).toHaveLength(4); // a(ctx), b(del), B(add), c(ctx)
    expect(result[0]).toEqual({ type: "ctx", text: "a" });
    expect(result[1]).toEqual({ type: "del", text: "b" });
    expect(result[2]).toEqual({ type: "add", text: "B" });
    expect(result[3]).toEqual({ type: "ctx", text: "c" });
  });

  it("detects additions in the middle", () => {
    const result = computeDiffLines("a\nc", "a\nb\nc");
    const adds = result.filter((l) => l.type === "add");
    expect(adds).toHaveLength(1);
    expect(adds[0].text).toBe("b");
  });

  it("detects deletions in the middle", () => {
    const result = computeDiffLines("a\nb\nc", "a\nc");
    const dels = result.filter((l) => l.type === "del");
    expect(dels).toHaveLength(1);
    expect(dels[0].text).toBe("b");
  });

  it("falls back to full del/add for large inputs (>500 lines)", () => {
    const oldLines = Array.from({ length: 300 }, (_, i) => `old-${i}`).join("\n");
    const newLines = Array.from({ length: 250 }, (_, i) => `new-${i}`).join("\n");
    const result = computeDiffLines(oldLines, newLines);
    // Should be 300 dels + 250 adds = 550
    expect(result.filter((l) => l.type === "del")).toHaveLength(300);
    expect(result.filter((l) => l.type === "add")).toHaveLength(250);
    expect(result.filter((l) => l.type === "ctx")).toHaveLength(0);
  });
});
