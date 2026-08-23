import { describe, it, expect } from "vitest"

import {
    filterSummary,
    instantToIso,
    mapPriority,
    mapReviewResultFrontend,
    mapReviewTypeFrontend,
} from "./mappers"
import type { CardFundsReviewQueueQuery } from "@/features/card-funds-review/types"

function query(
    overrides: Partial<CardFundsReviewQueueQuery> = {},
): CardFundsReviewQueueQuery {
    return {
        scope: "mine",
        type: "all",
        status: "OPEN",
        due: "all",
        ...overrides,
    }
}

describe("instantToIso", () => {
    it("converts epoch seconds to ISO strings", () => {
        expect(instantToIso(0)).toBe("1970-01-01T00:00:00.000Z")
        expect(instantToIso(1722470400)).toBe("2024-08-01T00:00:00.000Z")
    })

    it("returns an empty string for missing or invalid values", () => {
        expect(instantToIso(undefined)).toBe("")
        expect(instantToIso(null)).toBe("")
        expect(instantToIso(Number.NaN)).toBe("")
    })
})

describe("mapPriority", () => {
    it("passes numeric priorities through", () => {
        expect(mapPriority(42)).toBe(42)
    })

    it("maps known string priorities and defaults unknown ones", () => {
        expect(mapPriority("urgent")).toBe(100)
        expect(mapPriority("high")).toBe(80)
        expect(mapPriority("low")).toBe(20)
        expect(mapPriority("normal")).toBe(50)
        expect(mapPriority("whatever")).toBe(50)
    })
})

describe("mapReviewResultFrontend", () => {
    it("maps passed and APPROVED to APPROVED, everything else to REJECTED", () => {
        expect(mapReviewResultFrontend("passed")).toBe("APPROVED")
        expect(mapReviewResultFrontend("APPROVED")).toBe("APPROVED")
        expect(mapReviewResultFrontend("rejected")).toBe("REJECTED")
        expect(mapReviewResultFrontend("REJECTED")).toBe("REJECTED")
        expect(mapReviewResultFrontend("")).toBe("REJECTED")
    })
})

describe("mapReviewTypeFrontend", () => {
    it("maps sync delta variants to SYNC_DELTA and everything else to OPENING", () => {
        expect(mapReviewTypeFrontend("sync_delta")).toBe("SYNC_DELTA")
        expect(mapReviewTypeFrontend("SYNC_DELTA")).toBe("SYNC_DELTA")
        expect(mapReviewTypeFrontend("opening")).toBe("OPENING")
        expect(mapReviewTypeFrontend("OPENING")).toBe("OPENING")
        expect(mapReviewTypeFrontend("")).toBe("OPENING")
    })
})

describe("filterSummary", () => {
    it("builds the summary from scope, type, status and due", () => {
        expect(filterSummary(query({ scope: "mine" }))).toBe(
            "仅我的 · 全部类型 · 待处理有效队列 · 全部时限",
        )
        expect(
            filterSummary(
                query({ scope: "history", status: "COMPLETED", due: "today" }),
            ),
        ).toBe("处理历史 · 全部类型 · 已完成 · 今日到期")
        expect(
            filterSummary(query({ type: "delta", due: "overdue", q: "SO-1" })),
        ).toBe("仅我的 · 同步差额 · 待处理有效队列 · 已超期 · 搜索 SO-1")
    })

    it("shows closed for CLOSED status", () => {
        expect(
            filterSummary(query({ scope: "history", status: "CLOSED" })),
        ).toBe("处理历史 · 全部类型 · 已关闭 · 全部时限")
    })
})
