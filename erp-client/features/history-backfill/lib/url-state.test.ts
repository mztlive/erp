import { describe, expect, it } from 'vitest'

import {
    buildHistoryBackfillSearchParams,
    parseHistoryBackfillSearchParams,
} from '@/features/history-backfill/lib/url-state'

describe("parseHistoryBackfillSearchParams", () => {
    it("returns defaults for empty params", () => {
        const state = parseHistoryBackfillSearchParams(new URLSearchParams())

        expect(state).toMatchObject({
            view: "active",
            page: 1,
            section: "overview",
            mallId: undefined,
            environment: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            q: undefined,
            result: undefined,
            factType: undefined,
            costBasis: undefined,
            jobId: undefined,
        })
    })

    it("falls back to defaults for invalid enum values", () => {
        const state = parseHistoryBackfillSearchParams(
            new URLSearchParams("view=bogus&section=bogus"),
        )

        expect(state.view).toBe("active")
        expect(state.section).toBe("overview")
    })

    it("parses valid enum and string fields", () => {
        const state = parseHistoryBackfillSearchParams(
            new URLSearchParams(
                "view=all&processingStatus=RUNNING&reportReviewStatus=PENDING&basis=NONE&result=FAILED&factType=ORDER_CANCELED&costBasis=ACTUAL&q=abc",
            ),
        )

        expect(state).toMatchObject({
            view: "all",
            processingStatus: "RUNNING",
            reportReviewStatus: "PENDING",
            basis: "NONE",
            result: "FAILED",
            factType: "ORDER_CANCELED",
            costBasis: "ACTUAL",
            q: "abc",
        })
    })

    it("parses the mall alias into mallId", () => {
        const state = parseHistoryBackfillSearchParams(
            new URLSearchParams("mall=shop-1"),
        )

        expect(state.mallId).toBe("shop-1")
    })

    it("prefers mallId over the mall alias", () => {
        const state = parseHistoryBackfillSearchParams(
            new URLSearchParams("mallId=shop-2&mall=shop-1"),
        )

        expect(state.mallId).toBe("shop-2")
    })

    it("clamps page numbers: negative, non-numeric, and decimals", () => {
        expect(
            parseHistoryBackfillSearchParams(new URLSearchParams("page=0"))
                .page,
        ).toBe(1)
        expect(
            parseHistoryBackfillSearchParams(new URLSearchParams("page=abc"))
                .page,
        ).toBe(1)
        expect(
            parseHistoryBackfillSearchParams(new URLSearchParams("page=2.5"))
                .page,
        ).toBe(2)
        expect(
            parseHistoryBackfillSearchParams(new URLSearchParams("page=5"))
                .page,
        ).toBe(5)
    })
})

describe("buildHistoryBackfillSearchParams", () => {
    it("omits default values", () => {
        const state = parseHistoryBackfillSearchParams(new URLSearchParams())

        expect(buildHistoryBackfillSearchParams(state)).toBe("")
    })

    it("writes non-default values in field order", () => {
        const state = parseHistoryBackfillSearchParams(new URLSearchParams())
        const qs = buildHistoryBackfillSearchParams({
            ...state,
            q: "abc",
            section: "facts",
            page: 3,
        })

        expect(qs).toBe("?q=abc&page=3&section=facts")
    })

    it("includes jobId by default and omits it with omitJobId", () => {
        const state = parseHistoryBackfillSearchParams(new URLSearchParams())
        const withJob = { ...state, jobId: "job-5" }

        expect(buildHistoryBackfillSearchParams(withJob)).toBe("?jobId=job-5")
        expect(
            buildHistoryBackfillSearchParams(withJob, { omitJobId: true }),
        ).toBe("")
    })

    it("round-trips a non-default state", () => {
        const state = parseHistoryBackfillSearchParams(new URLSearchParams())
        const next = {
            ...state,
            view: "all",
            page: 4,
            q: "round",
            section: "report",
            processingStatus: "PARTIAL",
            jobId: "job-8",
        } as const

        const qs = buildHistoryBackfillSearchParams(next)
        const reparsed = parseHistoryBackfillSearchParams(
            new URLSearchParams(qs),
        )

        expect(reparsed).toMatchObject({
            view: "all",
            page: 4,
            q: "round",
            section: "report",
            processingStatus: "PARTIAL",
            jobId: "job-8",
        })
    })
})
