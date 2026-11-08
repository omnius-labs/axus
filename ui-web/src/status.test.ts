import { describe, expect, it } from "vitest";
import {
    isActivePublication,
    isActiveSubscription,
    isTerminalPublication,
    isTerminalSubscription,
    percent,
    safeRedirect,
} from "./status";

describe("status", () => {
    it("offers cancel for active rows and delete for terminal rows, and neither while finalizing", () => {
        expect(isActivePublication("Processing")).toBe(true);
        expect(isTerminalPublication("Committed")).toBe(true);
        expect(isActiveSubscription("Downloading")).toBe(true);
        expect(isTerminalSubscription("Completed")).toBe(true);
        expect(isActiveSubscription("Finalizing") || isTerminalSubscription("Finalizing")).toBe(false);
    });

    it("computes progress without dividing by zero", () => {
        expect(percent(0, 0)).toBe(0);
        expect(percent(1, 3)).toBe(33);
        expect(percent(5, 3)).toBe(100);
    });

    it("keeps only same-origin redirects", () => {
        expect(safeRedirect("/publications/abc")).toBe("/publications/abc");
        expect(safeRedirect("//evil.example")).toBe("/publications");
        expect(safeRedirect("https://evil.example")).toBe("/publications");
        expect(safeRedirect("/\\evil.example")).toBe("/publications");
        expect(safeRedirect(undefined)).toBe("/publications");
    });
});
