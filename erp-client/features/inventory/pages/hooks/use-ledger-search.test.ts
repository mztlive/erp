import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { useLedgerSearch } from './use-ledger-search'
import type { LedgerPatchUrl } from './use-inventory-ledger-url-state'

function setup(qParam = '') {
    const patchUrl = vi.fn<LedgerPatchUrl>()
    const rendered = renderHook(
        (props: { qParam: string }) =>
            useLedgerSearch({ qParam: props.qParam, patchUrl }),
        { initialProps: { qParam } },
    )
    return { ...rendered, patchUrl }
}

afterEach(() => {
    vi.useRealTimers()
})

describe('useLedgerSearch', () => {
    it('initialises the input from the URL q param', () => {
        const { result } = setup('SKU-1')
        expect(result.current.searchInput).toBe('SKU-1')
    })

    it('debounces input into the q param after 300ms', () => {
        vi.useFakeTimers()
        const { result, patchUrl } = setup()
        act(() => {
            result.current.setSearchInput('SKU-9')
        })
        vi.advanceTimersByTime(299)
        expect(patchUrl).not.toHaveBeenCalled()

        vi.advanceTimersByTime(1)
        expect(patchUrl).toHaveBeenCalledWith(
            { q: 'SKU-9' },
            { replace: true },
        )
    })

    it('does not write the URL when the debounced input equals the URL value', () => {
        vi.useFakeTimers()
        const { result, patchUrl } = setup('SKU-9')
        act(() => {
            result.current.setSearchInput('SKU-9')
        })
        vi.advanceTimersByTime(300)
        expect(patchUrl).not.toHaveBeenCalled()
    })

    it('trims the debounced q and writes null for whitespace-only input', () => {
        vi.useFakeTimers()
        const { result, patchUrl } = setup()
        act(() => {
            result.current.setSearchInput('   ')
        })
        vi.advanceTimersByTime(300)
        expect(patchUrl).toHaveBeenCalledWith({ q: null }, { replace: true })
    })

    it('trims surrounding whitespace from non-empty input', () => {
        vi.useFakeTimers()
        const { result, patchUrl } = setup()
        act(() => {
            result.current.setSearchInput('  SKU-1  ')
        })
        vi.advanceTimersByTime(300)
        expect(patchUrl).toHaveBeenCalledWith(
            { q: 'SKU-1' },
            { replace: true },
        )
    })

    it('clears a pending debounce when the input changes again', () => {
        vi.useFakeTimers()
        const { result, patchUrl } = setup()
        act(() => {
            result.current.setSearchInput('A')
        })
        vi.advanceTimersByTime(200)
        act(() => {
            result.current.setSearchInput('B')
        })
        vi.advanceTimersByTime(299)
        expect(patchUrl).not.toHaveBeenCalled()
        vi.advanceTimersByTime(1)
        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({ q: 'B' }, { replace: true })
    })

    it('syncs the search input when the URL q param changes', () => {
        const { result, rerender } = setup('OLD')
        expect(result.current.searchInput).toBe('OLD')
        rerender({ qParam: 'NEW' })
        expect(result.current.searchInput).toBe('NEW')
    })

    it('focuses the search input on "/" unless modifiers or editable targets are active', () => {
        const { result } = setup()
        const input = document.createElement('input')
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        const focusSpy = vi.spyOn(input, 'focus')

        window.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', cancelable: true }),
        )
        expect(focusSpy).toHaveBeenCalledTimes(1)

        focusSpy.mockClear()
        window.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', metaKey: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        window.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', ctrlKey: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        window.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', altKey: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        const other = document.createElement('input')
        document.body.appendChild(other)
        other.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', bubbles: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        const area = document.createElement('textarea')
        document.body.appendChild(area)
        area.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', bubbles: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        const select = document.createElement('select')
        document.body.appendChild(select)
        select.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', bubbles: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        document.body.removeChild(input)
        document.body.removeChild(other)
        document.body.removeChild(area)
        document.body.removeChild(select)
    })
})
