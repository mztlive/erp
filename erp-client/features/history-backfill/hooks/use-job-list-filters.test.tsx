import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render, renderHook } from "@testing-library/react"

import { useJobListFilters } from "@/features/history-backfill/hooks/use-job-list-filters"
import { parseHistoryBackfillSearchParams } from "@/features/history-backfill/lib/url-state"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"

vi.mock("@/features/entity-selectors/hooks/queries", () => ({
    useMallSelectorQuery: () => ({ data: [] }),
}))

function makeUrlState(init?: Record<string, string>): HistoryBackfillUrlState {
    return parseHistoryBackfillSearchParams(
        new URLSearchParams(init ?? {}),
    )
}

function renderFilters(init?: Record<string, string>) {
    const urlState = makeUrlState(init)
    const patchUrl = vi.fn()
    const hook = renderHook(() => useJobListFilters(urlState, patchUrl))
    return { ...hook, patchUrl }
}

describe("useJobListFilters", () => {
    afterEach(() => {
        cleanup()
    })

    it("uses defaults for an empty url", () => {
        const { result } = renderFilters()

        expect(result.current.searchDraft).toBe("")
        expect(result.current.mallIdDraft).toBeNull()
        expect(result.current.environmentDraft).toBe("all")
        expect(result.current.processingStatusDraft).toBe("all")
        expect(result.current.reportReviewStatusDraft).toBe("all")
        expect(result.current.basisDraft).toBe("all")
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.appliedChips).toEqual([])
        expect(result.current.panelOpen).toBe(false)
    })

    it("parses applied filters from the url and opens the panel on a deep link", () => {
        const { result } = renderFilters({
            q: "JOB-001",
            mallId: "mall-1",
            environment: "production",
            processingStatus: "RUNNING",
            reportReviewStatus: "PENDING",
            basis: "NONE",
            view: "active",
        })

        expect(result.current.q).toBe("JOB-001")
        expect(result.current.mallId).toBe("mall-1")
        expect(result.current.environment).toBe("production")
        expect(result.current.processingStatus).toBe("RUNNING")
        expect(result.current.reportReviewStatus).toBe("PENDING")
        expect(result.current.basis).toBe("NONE")
        expect(result.current.hasStructuredFilters).toBe(true)
        // 深链带结构化条件时面板必须自动展开，条件要可见可改
        expect(result.current.panelOpen).toBe(true)
    })

    it("keeps drafts local and applies them in a single patch", () => {
        const { result, patchUrl } = renderFilters()

        act(() => {
            result.current.setSearchDraft("  JOB-001  ")
            result.current.setMallIdDraft("mall-1")
            result.current.setEnvironmentDraft("verification")
            result.current.setProcessingStatusDraft("RUNNING")
        })
        expect(patchUrl).not.toHaveBeenCalled()

        act(() => result.current.applyFilters())

        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({
            q: "JOB-001",
            mallId: "mall-1",
            environment: "verification",
            processingStatus: "RUNNING",
            reportReviewStatus: undefined,
            basis: undefined,
            page: 1,
        })
        expect(result.current.panelOpen).toBe(false)
    })

    it("maps all drafts to omitted url params", () => {
        const { result, patchUrl } = renderFilters()

        act(() => result.current.applyFilters())

        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            mallId: undefined,
            environment: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            page: 1,
        })
    })

    it("clears every filter, the drafts and the panel while keeping view", () => {
        const { result, patchUrl } = renderFilters({
            q: "x",
            mallId: "mall-1",
            processingStatus: "RUNNING",
            basis: "NONE",
            view: "active",
        })

        act(() => result.current.clearAllFilters())

        expect(patchUrl).toHaveBeenCalledWith({
            q: undefined,
            mallId: undefined,
            environment: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            page: 1,
        })
        expect(result.current.searchDraft).toBe("")
        expect(result.current.mallIdDraft).toBeNull()
        expect(result.current.processingStatusDraft).toBe("all")
        expect(result.current.basisDraft).toBe("all")
        expect(result.current.panelOpen).toBe(false)
    })

    it("resets only more-filter conditions and preserves search and the panel", () => {
        const { result, patchUrl } = renderFilters({
            q: "x",
            mallId: "mall-1",
            processingStatus: "RUNNING",
        })

        act(() => {
            result.current.setPanelOpen(true)
            result.current.resetMoreFilters()
        })

        expect(patchUrl).toHaveBeenCalledWith({
            mallId: undefined,
            environment: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            page: 1,
        })
        expect(result.current.mallIdDraft).toBeNull()
        expect(result.current.processingStatusDraft).toBe("all")
        expect(result.current.searchDraft).toBe("x")
        expect(result.current.panelOpen).toBe(true)
    })

    it("removes a single applied condition without touching the others", () => {
        const { result, patchUrl } = renderFilters({
            q: "x",
            environment: "production",
            basis: "NONE",
        })

        act(() => result.current.removeFilter("environment"))

        expect(patchUrl).toHaveBeenCalledWith({
            environment: undefined,
            page: 1,
        })
        expect(result.current.environmentDraft).toBe("all")
        expect(result.current.q).toBe("x")
        expect(result.current.basis).toBe("NONE")
    })

    it("syncs drafts from url changes without reopening a closed panel", () => {
        const urlState = makeUrlState()
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ state }) => useJobListFilters(state, patchUrl),
            { initialProps: { state: urlState } },
        )

        expect(result.current.panelOpen).toBe(false)

        rerender({
            state: makeUrlState({
                mallId: "mall-1",
                processingStatus: "FAILED",
            }),
        })

        expect(result.current.mallIdDraft).toBe("mall-1")
        expect(result.current.processingStatusDraft).toBe("FAILED")
        // 已挂载页面的 URL 回填不得抢夺用户当前的展开态
        expect(result.current.panelOpen).toBe(false)
    })

    it("builds chips from applied filters with user-facing labels", () => {
        const { result } = renderFilters({
            q: "JOB-001",
            environment: "production",
            processingStatus: "RUNNING",
            reportReviewStatus: "CONFIRMED",
            basis: "NONE",
        })

        expect(result.current.appliedChips).toEqual([
            { key: "q", label: "搜索：JOB-001" },
            { key: "environment", label: "环境：生产环境" },
            { key: "processingStatus", label: "处理状态：运行中" },
            { key: "reportReviewStatus", label: "报告确认：已确认" },
            { key: "basis", label: "成本口径：未覆盖" },
        ])
        expect(result.current.hasActiveFilters).toBe(true)
    })

    it("focuses the search input on / and ignores form fields and overlays", () => {
        const urlState = makeUrlState()
        const patchUrl = vi.fn()

        function Host() {
            const filters = useJobListFilters(urlState, patchUrl)
            return (
                <input
                    ref={filters.searchInputRef}
                    value={filters.searchDraft}
                    onChange={(e) => filters.setSearchDraft(e.target.value)}
                    aria-label="搜索回填任务"
                />
            )
        }

        const { unmount } = render(<Host />)
        const input = document.querySelector(
            'input[aria-label="搜索回填任务"]',
        ) as HTMLInputElement

        fireEvent.keyDown(window, { key: "/" })
        expect(document.activeElement).toBe(input)

        input.focus()
        fireEvent.keyDown(window, { key: "/" })
        expect(document.activeElement).toBe(input)

        ;(document.activeElement as HTMLElement).blur()
        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)
        fireEvent.keyDown(window, { key: "/" })
        expect(document.activeElement).not.toBe(input)
        dialog.remove()

        unmount()
        fireEvent.keyDown(window, { key: "/" })
        expect(document.activeElement).toBe(document.body)
    })
})
