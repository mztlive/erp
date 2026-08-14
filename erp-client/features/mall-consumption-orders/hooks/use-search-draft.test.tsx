import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, fireEvent, act, cleanup } from '@testing-library/react'

import { useSearchDraft } from './use-search-draft'

type ReplaceParams = (
    patch: Record<string, string | undefined>,
    resetPage?: boolean,
) => void

function SearchHarness({
    qParam,
    replaceParams,
}: {
    qParam: string
    replaceParams: ReplaceParams
}) {
    const { searchInput, setSearchInput, searchInputRef, commitSearch } =
        useSearchDraft({ qParam, replaceParams })
    return (
        <input
            ref={searchInputRef}
            value={searchInput}
            aria-label="search"
            onChange={(e) => setSearchInput(e.target.value)}
            onKeyDown={(e) => {
                if (e.key === 'Enter') commitSearch()
            }}
        />
    )
}

beforeEach(() => {
    vi.useFakeTimers()
})

afterEach(() => {
    vi.useRealTimers()
    cleanup()
})

describe('useSearchDraft', () => {
    it('initializes the draft from the URL value', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="SO-1" replaceParams={replaceParams} />,
        )

        expect(
            (getByLabelText('search') as HTMLInputElement).value,
        ).toBe('SO-1')
        // 初始值与 URL 一致：防抖不写回。
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(replaceParams).not.toHaveBeenCalled()
    })

    it('writes the trimmed draft to the URL after the 300ms debounce', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')

        fireEvent.change(input, { target: { value: 'abc' } })
        act(() => {
            vi.advanceTimersByTime(299)
        })
        expect(replaceParams).not.toHaveBeenCalled()

        act(() => {
            vi.advanceTimersByTime(1)
        })
        expect(replaceParams).toHaveBeenCalledTimes(1)
        expect(replaceParams).toHaveBeenCalledWith({ q: 'abc' })
    })

    it('collapses rapid typing into a single write', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')

        fireEvent.change(input, { target: { value: 'a' } })
        act(() => {
            vi.advanceTimersByTime(100)
        })
        fireEvent.change(input, { target: { value: 'ab' } })
        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(replaceParams).toHaveBeenCalledTimes(1)
        expect(replaceParams).toHaveBeenCalledWith({ q: 'ab' })
    })

    it('does not write back when the trimmed draft matches the URL value', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="abc" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')

        fireEvent.change(input, { target: { value: ' abc ' } })
        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(replaceParams).not.toHaveBeenCalled()
    })

    it('commits the search immediately on Enter', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')

        fireEvent.change(input, { target: { value: 'efg' } })
        fireEvent.keyDown(input, { key: 'Enter' })

        expect(replaceParams).toHaveBeenCalledWith({ q: 'efg' })
    })

    it('keeps the draft while the input is focused when the URL value changes', () => {
        const replaceParams = vi.fn()
        const { getByLabelText, rerender } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')

        fireEvent.change(input, { target: { value: 'draft' } })
        input.focus()
        rerender(
            <SearchHarness qParam="fresh" replaceParams={replaceParams} />,
        )

        expect(
            (getByLabelText('search') as HTMLInputElement).value,
        ).toBe('draft')
    })

    it('adopts the URL value when the input is not focused', () => {
        const replaceParams = vi.fn()
        const { getByLabelText, rerender } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')

        fireEvent.change(input, { target: { value: 'draft' } })
        input.blur()
        rerender(
            <SearchHarness qParam="fresh" replaceParams={replaceParams} />,
        )

        expect(
            (getByLabelText('search') as HTMLInputElement).value,
        ).toBe('fresh')
    })

    it('focuses the input on "/" outside form fields', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')
        input.blur()

        fireEvent.keyDown(window, { key: '/' })

        expect(document.activeElement).toBe(input)
    })

    it('ignores "/" with modifier keys', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')
        input.blur()

        fireEvent.keyDown(window, { key: '/', metaKey: true })
        fireEvent.keyDown(window, { key: '/', ctrlKey: true })
        fireEvent.keyDown(window, { key: '/', altKey: true })

        expect(document.activeElement).not.toBe(input)
    })

    it('does not steal focus while typing inside an input', () => {
        const replaceParams = vi.fn()
        const { getByLabelText } = render(
            <SearchHarness qParam="" replaceParams={replaceParams} />,
        )
        const input = getByLabelText('search')
        input.focus()

        fireEvent.keyDown(input, { key: '/' })

        expect(document.activeElement).toBe(input)
    })
})
