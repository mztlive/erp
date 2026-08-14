import { describe, expect, it, vi } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { useLedgerFilterActions } from './use-ledger-filter-actions'
import type { LedgerPatchUrl } from './use-inventory-ledger-url-state'

function setup(sortValue = '') {
    const patchUrl = vi.fn<LedgerPatchUrl>()
    const resetPagination = vi.fn()
    const setSearchInput = vi.fn()
    const rendered = renderHook(() =>
        useLedgerFilterActions({
            patchUrl,
            resetPagination,
            setSearchInput,
            sortValue,
        }),
    )
    return {
        result: rendered.result,
        patchUrl,
        resetPagination,
        setSearchInput,
    }
}

describe('useLedgerFilterActions', () => {
    it('applyFilterPatch writes the patch with replace and resets pagination', () => {
        const { result, patchUrl, resetPagination } = setup()
        act(() => {
            result.current.handleApplyFilterPatch({ warehouseId: 'w1' })
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { warehouseId: 'w1' },
            { replace: true },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)
    })

    it('view change writes the view and drops a sort that does not belong to it', () => {
        const { result, patchUrl, resetPagination } = setup(
            'occurredAt:desc,movementId:desc',
        )
        act(() => {
            result.current.handleViewChange('balance')
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { view: 'balance', sort: null },
            { replace: true },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)
    })

    it('view change keeps a sort value that is valid for the target view', () => {
        const { result, patchUrl } = setup('occurredAt:desc,movementId:desc')
        act(() => {
            result.current.handleViewChange('movement')
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { view: 'movement' },
            { replace: true },
        )
    })

    it('view change keeps no sort when none is set', () => {
        const { result, patchUrl } = setup()
        act(() => {
            result.current.handleViewChange('adjustment')
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { view: 'adjustment' },
            { replace: true },
        )
    })

    it('clearAllFilters clears every filter param, the search input and pagination', () => {
        const { result, patchUrl, resetPagination, setSearchInput } = setup()
        act(() => {
            result.current.handleClearAllFilters()
        })
        expect(setSearchInput).toHaveBeenCalledWith('')
        expect(patchUrl).toHaveBeenCalledWith(
            {
                q: null,
                warehouseId: null,
                availability: 'all',
                skuId: null,
                salesOrderLineId: null,
                adjustmentId: null,
                movementType: null,
                occurredFrom: null,
                occurredTo: null,
            },
            { replace: true },
        )
        expect(resetPagination).toHaveBeenCalledTimes(1)
    })

    it('clearFiltersEmptyState clears only the filter params without resetting pagination', () => {
        const { result, patchUrl, resetPagination, setSearchInput } = setup()
        act(() => {
            result.current.handleClearFiltersEmptyState()
        })
        expect(setSearchInput).toHaveBeenCalledWith('')
        expect(patchUrl).toHaveBeenCalledWith(
            {
                q: null,
                warehouseId: null,
                availability: 'all',
                skuId: null,
                salesOrderLineId: null,
                adjustmentId: null,
            },
            { replace: true },
        )
        expect(resetPagination).not.toHaveBeenCalled()
    })
})
