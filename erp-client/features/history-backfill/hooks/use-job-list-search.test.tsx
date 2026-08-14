import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'

import { useJobListSearch } from '@/features/history-backfill/hooks/use-job-list-search'
import { parseHistoryBackfillSearchParams } from '@/features/history-backfill/lib/url-state'

function makeUrlState(q?: string) {
    const params = new URLSearchParams()
    if (q) params.set("q", q)
    return parseHistoryBackfillSearchParams(params)
}

function SearchHost({
    urlState,
    patchUrl,
}: {
    urlState: ReturnType<typeof parseHistoryBackfillSearchParams>
    patchUrl: (patch: Partial<ReturnType<typeof parseHistoryBackfillSearchParams>>) => void
}) {
    const { qDraft, setQDraft, searchInputRef } = useJobListSearch(
        urlState,
        patchUrl,
    )
    return (
        <input
            ref={searchInputRef}
            value={qDraft}
            onChange={(e) => setQDraft(e.target.value)}
            aria-label="搜索"
        />
    )
}

function searchInput(): HTMLInputElement {
    return screen.getByLabelText("搜索") as HTMLInputElement
}

describe("useJobListSearch", () => {
    beforeEach(() => {
        vi.useFakeTimers()
    })

    afterEach(() => {
        cleanup()
        vi.useRealTimers()
    })

    it("initializes the draft from the URL", () => {
        const patchUrl = vi.fn()
        render(<SearchHost urlState={makeUrlState("abc")} patchUrl={patchUrl} />)

        expect(searchInput().value).toBe("abc")
    })

    it("debounces draft changes into the URL after 300ms", () => {
        const patchUrl = vi.fn()
        render(<SearchHost urlState={makeUrlState()} patchUrl={patchUrl} />)

        fireEvent.change(searchInput(), { target: { value: "abc" } })
        act(() => {
            vi.advanceTimersByTime(299)
        })
        expect(patchUrl).not.toHaveBeenCalled()

        act(() => {
            vi.advanceTimersByTime(1)
        })
        expect(patchUrl).toHaveBeenCalledWith({ q: "abc", page: 1 })
    })

    it("does not patch when the trimmed draft equals the URL value", () => {
        const patchUrl = vi.fn()
        render(<SearchHost urlState={makeUrlState("abc")} patchUrl={patchUrl} />)

        fireEvent.change(searchInput(), { target: { value: " abc " } })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("clearing the draft before the debounce fires cancels the patch", () => {
        const patchUrl = vi.fn()
        render(<SearchHost urlState={makeUrlState()} patchUrl={patchUrl} />)

        const input = searchInput()
        fireEvent.change(input, { target: { value: "abc" } })
        act(() => {
            vi.advanceTimersByTime(100)
        })
        fireEvent.change(input, { target: { value: "" } })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("syncs the draft when the URL changes externally without patching", () => {
        const patchUrl = vi.fn()
        const { rerender } = render(
            <SearchHost urlState={makeUrlState()} patchUrl={patchUrl} />,
        )

        rerender(<SearchHost urlState={makeUrlState("xyz")} patchUrl={patchUrl} />)
        expect(searchInput().value).toBe("xyz")

        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it("focuses the search input on '/' outside of form fields", () => {
        const patchUrl = vi.fn()
        render(<SearchHost urlState={makeUrlState()} patchUrl={patchUrl} />)

        fireEvent.keyDown(window, { key: "/" })

        expect(document.activeElement).toBe(searchInput())
    })

    it("ignores '/' when focus is inside an input", () => {
        const patchUrl = vi.fn()
        render(<SearchHost urlState={makeUrlState()} patchUrl={patchUrl} />)

        const input = searchInput()
        input.focus()
        fireEvent.keyDown(window, { key: "/" })

        expect(document.activeElement).toBe(input)
    })

    it("removes the keydown listener on unmount", () => {
        const patchUrl = vi.fn()
        const { unmount } = render(
            <SearchHost urlState={makeUrlState()} patchUrl={patchUrl} />,
        )
        unmount()

        fireEvent.keyDown(window, { key: "/" })
        expect(document.activeElement).toBe(document.body)
    })
})
