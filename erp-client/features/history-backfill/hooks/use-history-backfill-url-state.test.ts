import { beforeEach, describe, expect, it, vi } from 'vitest'
import { renderHook } from '@testing-library/react'

const { replaceMock } = vi.hoisted(() => ({ replaceMock: vi.fn() }))

vi.mock('next/navigation', () => ({
    useRouter: vi.fn(() => ({
        push: vi.fn(),
        replace: replaceMock,
        back: vi.fn(),
    })),
    useSearchParams: vi.fn(() => new URLSearchParams()),
    usePathname: vi.fn(() => '/governance/history-backfill'),
    useParams: vi.fn(() => ({})),
}))

import { useSearchParams } from 'next/navigation'
import { useHistoryBackfillUrlState } from '@/features/history-backfill/hooks/use-history-backfill-url-state'

function setSearchParams(query: string) {
    vi.mocked(useSearchParams).mockReturnValue(
        new URLSearchParams(query) as ReturnType<typeof useSearchParams>,
    )
}

describe("useHistoryBackfillUrlState", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        setSearchParams("")
    })

    it("parses defaults from empty search params", () => {
        const { result } = renderHook(() => useHistoryBackfillUrlState())

        expect(result.current.urlState).toMatchObject({
            view: "active",
            page: 1,
            section: "overview",
            jobId: undefined,
        })
        expect(result.current.jobId).toBeUndefined()
        expect(result.current.pathname).toBe("/governance/history-backfill")
    })

    it("parses existing search params", () => {
        setSearchParams("view=all&page=3&q=abc&section=facts")

        const { result } = renderHook(() => useHistoryBackfillUrlState())

        expect(result.current.urlState).toMatchObject({
            view: "all",
            page: 3,
            q: "abc",
            section: "facts",
        })
    })

    it("prefers the route jobId over the query param", () => {
        setSearchParams("jobId=job-from-query")

        const { result } = renderHook(() =>
            useHistoryBackfillUrlState("job-from-route"),
        )

        expect(result.current.jobId).toBe("job-from-route")
    })

    it("falls back to the jobId query param when no route jobId", () => {
        setSearchParams("jobId=job-from-query")

        const { result } = renderHook(() => useHistoryBackfillUrlState())

        expect(result.current.jobId).toBe("job-from-query")
    })

    it("patchUrl replaces the list URL with the new params", () => {
        const { result } = renderHook(() => useHistoryBackfillUrlState())

        result.current.patchUrl({ page: 2 })

        expect(replaceMock).toHaveBeenCalledWith(
            "/governance/history-backfill?page=2",
            { scroll: false },
        )
    })

    it("patchUrl writes to the detail URL when a job is open", () => {
        const { result } = renderHook(() =>
            useHistoryBackfillUrlState("job-9"),
        )

        result.current.patchUrl({ page: 2 })

        expect(replaceMock).toHaveBeenCalledWith(
            "/governance/history-backfill/job-9?page=2",
            { scroll: false },
        )
    })

    it("omits jobId from built URLs", () => {
        setSearchParams("jobId=job-5&section=facts")

        const { result } = renderHook(() => useHistoryBackfillUrlState())

        result.current.patchUrl({ page: 3 })

        expect(replaceMock).toHaveBeenCalledWith(
            "/governance/history-backfill/job-5?page=3&section=facts",
            { scroll: false },
        )
    })

    it("openJob resets detail filters and section", () => {
        setSearchParams("q=abc&section=facts&result=FAILED&factType=ORDER_CANCELED&costBasis=NONE&page=4")

        const { result } = renderHook(() => useHistoryBackfillUrlState())

        result.current.openJob("job-2")

        expect(replaceMock).toHaveBeenCalledWith(
            "/governance/history-backfill/job-2",
            { scroll: false },
        )
    })

    it("backToList drops jobId and resets the section", () => {
        setSearchParams("jobId=job-5&section=facts&page=2")

        const { result } = renderHook(() => useHistoryBackfillUrlState())

        result.current.backToList()

        expect(replaceMock).toHaveBeenCalledWith(
            "/governance/history-backfill?page=2",
            { scroll: false },
        )
    })
})
