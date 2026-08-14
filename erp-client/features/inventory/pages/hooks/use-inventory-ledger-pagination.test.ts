import { describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { useInventoryLedgerPagination } from './use-inventory-ledger-pagination'
import type { LedgerPatchUrl } from './use-inventory-ledger-url-state'

function setup({
    view = 'balance' as const,
    pageSize = 20,
    cursorOffset = 0,
}: {
    view?: 'balance' | 'movement' | 'reservation' | 'adjustment'
    pageSize?: number
    cursorOffset?: number
} = {}) {
    const patchUrl = vi.fn<LedgerPatchUrl>()
    const rendered = renderHook(
        (props: { pageSize: number; cursorOffset: number; view: string }) =>
            useInventoryLedgerPagination({
                view: props.view as typeof view,
                pageSize: props.pageSize,
                cursorOffset: props.cursorOffset,
                patchUrl,
            }),
        { initialProps: { pageSize, cursorOffset, view } },
    )
    return { ...rendered, patchUrl }
}

describe('useInventoryLedgerPagination', () => {
    it('derives the initial page from the cursor offset', () => {
        const { result } = setup({ cursorOffset: 40, pageSize: 20 })
        expect(result.current.pagination).toEqual({
            pageIndex: 2,
            pageSize: 20,
        })
    })

    it('resetPagination goes back to page 0 and is a no-op there', () => {
        const { result } = setup({ cursorOffset: 40, pageSize: 20 })
        const initial = result.current.pagination
        act(() => {
            result.current.resetPagination()
        })
        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 20,
        })

        const afterReset = result.current.pagination
        act(() => {
            result.current.resetPagination()
        })
        expect(result.current.pagination).toBe(afterReset)
        expect(initial).not.toBe(afterReset)
    })

    it('handlePaginationChange writes cursor and pageSize to the URL', () => {
        const { result, patchUrl } = setup()
        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 2,
                pageSize: 20,
            })
        })
        expect(result.current.pagination).toEqual({
            pageIndex: 2,
            pageSize: 20,
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { cursor: 'w10:balance:40', pageSize: '20' },
            { replace: true },
        )
    })

    it('clears the cursor param on the first page', () => {
        const { result, patchUrl } = setup()
        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 0,
                pageSize: 20,
            })
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { cursor: null, pageSize: '20' },
            { replace: true },
        )
    })

    it('encodes the cursor with the active view', () => {
        const { result, patchUrl } = setup({ view: 'movement' })
        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 3,
                pageSize: 10,
            })
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { cursor: 'w10:movement:30', pageSize: '10' },
            { replace: true },
        )
    })

    it('syncs pagination when the URL-derived cursor or pageSize change', () => {
        const { result, rerender } = setup({ cursorOffset: 40, pageSize: 20 })
        expect(result.current.pagination.pageIndex).toBe(2)

        rerender({ pageSize: 20, cursorOffset: 60, view: 'balance' })
        expect(result.current.pagination).toEqual({
            pageIndex: 3,
            pageSize: 20,
        })

        rerender({ pageSize: 50, cursorOffset: 60, view: 'balance' })
        expect(result.current.pagination).toEqual({
            pageIndex: 1,
            pageSize: 50,
        })
    })

    it('keeps the pagination object identity when nothing changed', () => {
        const { result, rerender } = setup({ cursorOffset: 0, pageSize: 20 })
        const before = result.current.pagination
        rerender({ pageSize: 20, cursorOffset: 0, view: 'balance' })
        expect(result.current.pagination).toBe(before)
    })
})
