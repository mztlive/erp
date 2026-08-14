import { renderHook } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { SettlementsUrlState } from '@/features/supplier-settlements/lib/url-state'
import { useSettlementListState } from './use-settlement-list-state'

function makeState(
    overrides: Partial<SettlementsUrlState> = {},
): SettlementsUrlState {
    return {
        view: 'pending',
        page: 1,
        section: 'overview',
        ...overrides,
    }
}

describe('useSettlementListState', () => {
    it('derives the first page from a default url state', () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useSettlementListState(makeState(), patchUrl),
        )

        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 50,
        })
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it('maps the url page to a zero-based page index and clamps at zero', () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ page }: { page: number }) =>
                useSettlementListState(makeState({ page }), patchUrl),
            { initialProps: { page: 3 } },
        )

        expect(result.current.pagination.pageIndex).toBe(2)

        rerender({ page: 0 })
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it('detects active filters from any non-default field', () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({ state }: { state: SettlementsUrlState }) =>
                useSettlementListState(state, patchUrl),
            { initialProps: { state: makeState() } },
        )

        for (const patch of [
            { supplierId: 'sup1' },
            { periodFrom: '2026-01-01' },
            { periodTo: '2026-01-31' },
            { status: 'DRAFT' },
            { differenceType: 'AMOUNT' as const },
            { q: 'abc' },
            { view: 'confirmed' as const },
        ]) {
            rerender({ state: makeState(patch) })
            expect(result.current.hasActiveFilters).toBe(true)
        }

        rerender({ state: makeState({ view: 'pending' }) })
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it('clears all filters and resets view and page in one patch', () => {
        const patchUrl = vi.fn()
        const { result } = renderHook(() =>
            useSettlementListState(
                makeState({
                    supplierId: 'sup1',
                    status: 'DRAFT',
                    q: 'abc',
                    page: 3,
                    preview: 'st1',
                }),
                patchUrl,
            ),
        )

        result.current.clearFilters()

        expect(patchUrl).toHaveBeenCalledTimes(1)
        expect(patchUrl).toHaveBeenCalledWith({
            view: 'pending',
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            q: undefined,
            periodFrom: undefined,
            periodTo: undefined,
            page: 1,
        })
    })

    it('keeps a stable clearFilters callback while patchUrl is stable', () => {
        const patchUrl = vi.fn()
        const { result, rerender } = renderHook(
            ({
                state,
                patchUrl: patch,
            }: {
                state: SettlementsUrlState
                patchUrl: (patch: Partial<SettlementsUrlState>) => void
            }) => useSettlementListState(state, patch),
            { initialProps: { state: makeState(), patchUrl } },
        )

        const first = result.current.clearFilters
        rerender({ state: makeState({ q: 'abc' }), patchUrl })
        expect(result.current.clearFilters).toBe(first)

        const nextPatchUrl = vi.fn()
        rerender({ state: makeState(), patchUrl: nextPatchUrl })
        expect(result.current.clearFilters).not.toBe(first)
    })
})
