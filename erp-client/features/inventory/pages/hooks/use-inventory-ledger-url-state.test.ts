import { beforeEach, describe, expect, it, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useInventoryLedgerUrlState } from './use-inventory-ledger-url-state'

vi.mock('next/navigation', () => ({
    useRouter: vi.fn(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() })),
    useSearchParams: vi.fn(() => params()),
    usePathname: vi.fn(() => '/inventory'),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useRouter, useSearchParams } from 'next/navigation'
import type { ReadonlyURLSearchParams } from 'next/navigation'

const mockedRouter = vi.mocked(useRouter)
const mockedSearchParams = vi.mocked(useSearchParams)
const mockedPathname = vi.mocked(usePathname)

function params(raw = ''): ReadonlyURLSearchParams {
    return new URLSearchParams(raw) as unknown as ReadonlyURLSearchParams
}

function setupRouter() {
    const router = {
        push: vi.fn(),
        replace: vi.fn(),
        back: vi.fn(),
    }
    mockedRouter.mockReturnValue(
        router as unknown as ReturnType<typeof useRouter>,
    )
    return router
}

beforeEach(() => {
    vi.clearAllMocks()
    mockedSearchParams.mockReturnValue(params())
    mockedPathname.mockReturnValue('/inventory')
    setupRouter()
})

describe('useInventoryLedgerUrlState', () => {
    it('applies defaults when no params are present', () => {
        const { result } = renderHook(() => useInventoryLedgerUrlState())
        expect(result.current.view).toBe('balance')
        expect(result.current.qParam).toBe('')
        expect(result.current.warehouseId).toBeUndefined()
        expect(result.current.skuId).toBeUndefined()
        expect(result.current.salesOrderLineId).toBeUndefined()
        expect(result.current.availability).toBe('all')
        expect(result.current.balanceIdParam).toBeUndefined()
        expect(result.current.adjustmentIdParam).toBeUndefined()
        expect(result.current.movementType).toEqual([])
        expect(result.current.occurredFrom).toBeUndefined()
        expect(result.current.occurredTo).toBeUndefined()
        expect(result.current.sortValue).toBe(
            'warehouseCode:asc,skuCode:asc',
        )
        expect(result.current.pageSize).toBe(20)
        expect(result.current.cursorParam).toBeUndefined()
        expect(result.current.cursorOffset).toBe(0)
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it('parses every param from the URL', () => {
        mockedSearchParams.mockReturnValue(
            params(
                'view=movement&q=SKU-1&warehouseId=w1&skuId=s1' +
                    '&salesOrderLineId=l1&availability=zero&balanceId=b1' +
                    '&adjustmentId=a1&movementType=A,B&occurredFrom=2026-08-01' +
                    '&occurredTo=2026-08-14&sort=occurredAt:asc,movementId:asc' +
                    '&pageSize=50&cursor=w10%3Amovement%3A100',
            ),
        )
        const { result } = renderHook(() => useInventoryLedgerUrlState())
        expect(result.current.view).toBe('movement')
        expect(result.current.qParam).toBe('SKU-1')
        expect(result.current.warehouseId).toBe('w1')
        expect(result.current.skuId).toBe('s1')
        expect(result.current.salesOrderLineId).toBe('l1')
        expect(result.current.availability).toBe('zero')
        expect(result.current.balanceIdParam).toBe('b1')
        expect(result.current.adjustmentIdParam).toBe('a1')
        expect(result.current.movementType).toEqual(['A', 'B'])
        expect(result.current.occurredFrom).toBe('2026-08-01')
        expect(result.current.occurredTo).toBe('2026-08-14')
        expect(result.current.sortValue).toBe(
            'occurredAt:asc,movementId:asc',
        )
        expect(result.current.pageSize).toBe(50)
        expect(result.current.cursorParam).toBe('w10:movement:100')
        expect(result.current.cursorOffset).toBe(100)
        expect(result.current.hasActiveFilters).toBe(true)
    })

    it('falls back to defaults for unknown enum values', () => {
        mockedSearchParams.mockReturnValue(
            params('view=bogus&availability=sometimes'),
        )
        const { result } = renderHook(() => useInventoryLedgerUrlState())
        expect(result.current.view).toBe('balance')
        expect(result.current.availability).toBe('all')
    })

    it('clamps pageSize: invalid, zero and negative become 20; oversized becomes 100', () => {
        for (const [raw, expected] of [
            ['abc', 20],
            ['0', 20],
            ['-5', 20],
            ['20.5', 20],
            ['300', 100],
            ['NaN', 20],
        ] as const) {
            mockedSearchParams.mockReturnValue(
                params(`pageSize=${raw}`),
            )
            const rendered = renderHook(() => useInventoryLedgerUrlState())
            expect(rendered.result.current.pageSize).toBe(expected)
            rendered.unmount()
        }
    })

    it('decodes a valid cursor for the active view and ignores mismatches or garbage', () => {
        mockedSearchParams.mockReturnValue(
            params('view=movement&cursor=w10%3Amovement%3A40'),
        )
        const valid = renderHook(() => useInventoryLedgerUrlState())
        expect(valid.result.current.cursorOffset).toBe(40)
        valid.unmount()

        mockedSearchParams.mockReturnValue(
            params('view=movement&cursor=w10%3Abalance%3A40'),
        )
        const mismatched = renderHook(() => useInventoryLedgerUrlState())
        expect(mismatched.result.current.cursorOffset).toBe(0)
        mismatched.unmount()

        mockedSearchParams.mockReturnValue(
            params('view=balance&cursor=garbage'),
        )
        const garbage = renderHook(() => useInventoryLedgerUrlState())
        expect(garbage.result.current.cursorOffset).toBe(0)
        garbage.unmount()

        mockedSearchParams.mockReturnValue(
            params(
                'view=balance&cursor=w10%3Abalance%3A9007199254740993',
            ),
        )
        const unsafe = renderHook(() => useInventoryLedgerUrlState())
        expect(unsafe.result.current.cursorOffset).toBe(0)
    })

    it('derives the default sort per view', () => {
        mockedSearchParams.mockReturnValue(
            params('view=adjustment'),
        )
        const { result } = renderHook(() => useInventoryLedgerUrlState())
        expect(result.current.sortValue).toBe(
            'createdAt:desc,adjustmentId:desc',
        )
    })

    it('patchUrl removes null values, keeps the view param and clears the cursor', () => {
        mockedSearchParams.mockReturnValue(
            params(
                'view=balance&cursor=w10%3Abalance%3A40&warehouseId=w1&q=x',
            ),
        )
        const router = setupRouter()
        const { result, rerender } = renderHook(() =>
            useInventoryLedgerUrlState(),
        )

        act(() => {
            result.current.patchUrl({ warehouseId: null, q: 'SKU-2' })
        })
        expect(router.push).toHaveBeenCalledWith(
            '/inventory?view=balance&q=SKU-2',
        )

        mockedSearchParams.mockReturnValue(
            params('view=balance&q=SKU-2'),
        )
        rerender()
        act(() => {
            result.current.patchUrl({ availability: 'zero' }, { replace: true })
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/inventory?view=balance&q=SKU-2&availability=zero',
        )
    })

    it('patchUrl keeps the cursor when the patch contains cursor or pageSize', () => {
        mockedSearchParams.mockReturnValue(
            params('view=balance&cursor=w10%3Abalance%3A40'),
        )
        const router = setupRouter()
        const { result } = renderHook(() => useInventoryLedgerUrlState())

        act(() => {
            result.current.patchUrl({ pageSize: '50' }, { replace: true })
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/inventory?view=balance&cursor=w10%3Abalance%3A40&pageSize=50',
        )
    })

    it('patchUrl backfills the view param when it is missing', () => {
        mockedSearchParams.mockReturnValue(params('q=x'))
        const router = setupRouter()
        const { result } = renderHook(() => useInventoryLedgerUrlState())

        act(() => {
            result.current.patchUrl({ q: null })
        })
        expect(router.push).toHaveBeenCalledWith('/inventory?view=balance')
    })

    it('hasActiveFilters covers filters, deep links and blank occurrence dates', () => {
        mockedSearchParams.mockReturnValue(params())
        const none = renderHook(() => useInventoryLedgerUrlState())
        expect(none.result.current.hasActiveFilters).toBe(false)
        none.unmount()

        mockedSearchParams.mockReturnValue(
            params('view=balance&availability=zero'),
        )
        const availability = renderHook(() => useInventoryLedgerUrlState())
        expect(availability.result.current.hasActiveFilters).toBe(true)
        availability.unmount()

        mockedSearchParams.mockReturnValue(
            params('view=balance&availability=all'),
        )
        const allAvailability = renderHook(() =>
            useInventoryLedgerUrlState(),
        )
        expect(allAvailability.result.current.hasActiveFilters).toBe(false)
        allAvailability.unmount()

        mockedSearchParams.mockReturnValue(
            params('view=balance&movementType=A'),
        )
        const movement = renderHook(() => useInventoryLedgerUrlState())
        expect(movement.result.current.hasActiveFilters).toBe(true)
        movement.unmount()

        mockedSearchParams.mockReturnValue(
            params('view=balance&occurredFrom='),
        )
        const blankDate = renderHook(() => useInventoryLedgerUrlState())
        expect(blankDate.result.current.hasActiveFilters).toBe(true)
    })

    it('re-parses when the URL search params change', () => {
        mockedSearchParams.mockReturnValue(params())
        const { result, rerender } = renderHook(() =>
            useInventoryLedgerUrlState(),
        )
        expect(result.current.warehouseId).toBeUndefined()

        mockedSearchParams.mockReturnValue(
            params('view=balance&warehouseId=w9'),
        )
        rerender()
        expect(result.current.warehouseId).toBe('w9')
    })
})
