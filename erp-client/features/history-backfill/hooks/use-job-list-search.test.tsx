import { describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useJobListSearch } from "@/features/history-backfill/hooks/use-job-list-search"
import { useJobListFilters } from "@/features/history-backfill/hooks/use-job-list-filters"
import { parseHistoryBackfillSearchParams } from "@/features/history-backfill/lib/url-state"

vi.mock("@/features/entity-selectors/hooks/queries", () => ({
    useMallSelectorQuery: () => ({ data: [] }),
}))

describe("useJobListSearch", () => {
    it("is the same merged filter hook (compatibility entry)", () => {
        expect(useJobListSearch).toBe(useJobListFilters)
    })

    it("keeps drafts local until apply commits them to the url", () => {
        const urlState = parseHistoryBackfillSearchParams(
            new URLSearchParams(),
        )
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useJobListSearch(urlState, patchUrl),
        )

        act(() => result.current.setSearchDraft("JOB-001"))
        expect(patchUrl).not.toHaveBeenCalled()

        act(() => result.current.applyFilters())

        expect(patchUrl).toHaveBeenCalledWith({
            q: "JOB-001",
            mallId: undefined,
            environment: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            page: 1,
        })
    })
})
