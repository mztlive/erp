import { describe, expect, it } from "vitest"

import {
    formatClock,
    formatSourceWatermark,
    freshnessPresentation,
    metricReliabilityDetail,
} from "./presentation"

describe("formatClock", () => {
    it("formats an ISO timestamp as a zh-CN HH:MM clock", () => {
        const out = formatClock("2026-08-01T09:35:48+08:00")
        expect(out).toMatch(/^\d{2}:\d{2}$/)
    })

    it("returns the input unchanged when it is not a valid date", () => {
        expect(formatClock("not-a-date")).toBe("not-a-date")
    })
})

describe("formatSourceWatermark", () => {
    it("extracts the readable timestamp portion", () => {
        expect(
            formatSourceWatermark("outbox:cq:2026-08-01T09:35:48+08:00"),
        ).toBe("2026-08-01T09:35:48")
    })

    it("returns the input unchanged when no timestamp matches", () => {
        expect(formatSourceWatermark("outbox:cq:unknown")).toBe(
            "outbox:cq:unknown",
        )
    })
})

describe("freshnessPresentation", () => {
    it("prioritizes the refreshing and refresh-failed signals", () => {
        expect(freshnessPresentation("fresh", false, true)).toEqual({
            state: "syncing",
            statusLabel: "正在刷新",
        })
        expect(freshnessPresentation("fresh", true, false)).toEqual({
            state: "failed",
            statusLabel: "刷新失败（保留旧数据）",
        })
    })

    it("maps each freshness state to its status label", () => {
        expect(freshnessPresentation("failed")).toEqual({
            state: "failed",
            statusLabel: "数据加载失败",
        })
        expect(freshnessPresentation("rebuilding")).toEqual({
            state: "syncing",
            statusLabel: "正在重建",
        })
        expect(freshnessPresentation("stale")).toEqual({
            state: "stale",
            statusLabel: "数据可能不是最新",
        })
        expect(freshnessPresentation("fresh")).toEqual({
            state: "fresh",
            statusLabel: "数据已更新",
        })
    })
})

describe("metricReliabilityDetail", () => {
    it("explains denied fields first", () => {
        expect(metricReliabilityDetail("reliable", "解释", true)).toBe(
            "当前角色不可查看",
        )
    })

    it("uses the explanation when provided", () => {
        expect(metricReliabilityDetail("partial", "解释", false)).toBe("解释")
        expect(metricReliabilityDetail("reliable", "解释", false)).toBe(
            "解释",
        )
    })

    it("falls back to per-level default wording", () => {
        expect(metricReliabilityDetail("partial", undefined, false)).toBe(
            "部分可靠",
        )
        expect(metricReliabilityDetail("unavailable", undefined, false)).toBe(
            "暂无可靠口径",
        )
        expect(metricReliabilityDetail("reliable", undefined, false)).toBe(
            undefined,
        )
    })
})
