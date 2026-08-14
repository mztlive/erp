import { act, renderHook, waitFor } from '@testing-library/react'
import { usePathname, useSearchParams } from 'next/navigation'
import type { ReadonlyURLSearchParams } from 'next/navigation'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { DateBasis, DateBasisConfig } from '../types'
import { resolvePeriod } from '../lib/url-state'
import { useCardBusinessPageState } from './use-card-business-page-state'

const navigationMocks = vi.hoisted(() => ({
    router: { push: vi.fn(), replace: vi.fn(), back: vi.fn() },
}))

vi.mock('next/navigation', () => ({
    useRouter: () => navigationMocks.router,
    useSearchParams: vi.fn(() => new URLSearchParams() as unknown as ReadonlyURLSearchParams),
    usePathname: vi.fn(() => '/analytics/card-business'),
    useParams: vi.fn(() => ({})),
}))

function makeBasisConfig(
    configuredDateBasis?: DateBasis,
): DateBasisConfig {
    return {
        configuredDateBasis,
        allowedDateBases: [
            { code: 'consumption', label: '消费发生日', explanation: '' },
            { code: 'sales', label: '销售发生日', explanation: '' },
            { code: 'expiry', label: '履约到期日', explanation: '' },
        ],
        configurationVersion: 'v1',
    }
}

function routerReplaceMock() {
    return navigationMocks.router.replace
}

beforeEach(() => {
    vi.mocked(useSearchParams).mockReturnValue(new URLSearchParams() as unknown as ReadonlyURLSearchParams)
    vi.mocked(usePathname).mockReturnValue('/analytics/card-business')
    routerReplaceMock().mockClear()
})

describe('useCardBusinessPageState — URL 参数解析', () => {
    it('falls back to defaults when no params are present', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        expect(result.current.periodPreset).toBe('month-to-date')
        expect(result.current.periodPresetValue).toBe('')
        expect(result.current.from).toBe('')
        expect(result.current.to).toBe('')
        expect(result.current.dateBasis).toBe('')
        expect(result.current.dimension).toBe('customer')
        expect(result.current.customerId).toBeUndefined()
        expect(result.current.salesOrderId).toBeUndefined()
        expect(result.current.costBasis).toBeUndefined()
        expect(result.current.expiryState).toBe('all')
        expect(result.current.coverage).toBe('all')
        expect(result.current.sort).toBe('consumption:desc')
        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 50,
        })
        expect(result.current.analysisBlocked).toBe(false)
        expect(result.current.analysisReady).toBe(false)
        expect(result.current.analysisQuery).toBeNull()
    })

    it('parses a fully populated URL and derives the analysis query', () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams(
                'from=2026-08-01&to=2026-08-07&dateBasis=consumption&dimension=sales_order&customerId=c1&salesOrderId=so1&costBasis=ACTUAL,STANDARD&expiryState=active&coverage=none&sort=refund:asc&page=2&periodPreset=last-month',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() =>
            useCardBusinessPageState(makeBasisConfig('consumption'), true),
        )
        expect(result.current.periodPreset).toBe('last-month')
        expect(result.current.from).toBe('2026-08-01')
        expect(result.current.to).toBe('2026-08-07')
        expect(result.current.dateBasis).toBe('consumption')
        expect(result.current.dimension).toBe('sales_order')
        expect(result.current.customerId).toBe('c1')
        expect(result.current.salesOrderId).toBe('so1')
        expect(result.current.costBasis).toEqual(['ACTUAL', 'STANDARD'])
        expect(result.current.expiryState).toBe('active')
        expect(result.current.coverage).toBe('none')
        expect(result.current.sort).toBe('refund:asc')
        expect(result.current.pagination).toEqual({
            pageIndex: 1,
            pageSize: 50,
        })
        expect(result.current.analysisReady).toBe(true)
        expect(result.current.analysisQuery).toEqual({
            from: '2026-08-01',
            to: '2026-08-07',
            dateBasis: 'consumption',
            dimension: 'sales_order',
            customerId: 'c1',
            salesOrderId: 'so1',
            costBasis: ['ACTUAL', 'STANDARD'],
            expiryState: 'active',
            coverage: 'none',
            sort: 'refund:asc',
            page: 2,
            pageSize: 50,
        })
    })

    it('sanitizes invalid enum values and page numbers', () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams(
                'dimension=garbage&expiryState=weird&coverage=weird&costBasis=FOO,BAR&page=abc&dateBasis=weird',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        expect(result.current.dimension).toBe('customer')
        expect(result.current.expiryState).toBe('all')
        expect(result.current.coverage).toBe('all')
        expect(result.current.costBasis).toBeUndefined()
        expect(result.current.dateBasis).toBe('')
        expect(result.current.pagination.pageIndex).toBe(0)
        expect(result.current.analysisReady).toBe(false)
    })

    it('parses cost basis lists ignoring unknown entries', () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams('costBasis=ACTUAL, NONE, BOOM') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        expect(result.current.costBasis).toEqual(['ACTUAL', 'NONE'])
    })
})

describe('useCardBusinessPageState — URL 写入', () => {
    it('patchUrl replaces the URL, drops page and resets pagination', () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams('page=3&from=2026-08-01') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        act(() => {
            result.current.patchUrl({ customerId: 'c1' })
        })
        expect(routerReplaceMock()).toHaveBeenCalledWith(
            '/analytics/card-business?from=2026-08-01&customerId=c1',
        )
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it('patchUrl deletes params whose value is null or empty', () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams('customerId=c1') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        act(() => {
            result.current.patchUrl({ customerId: null })
        })
        expect(routerReplaceMock()).toHaveBeenCalledWith(
            '/analytics/card-business',
        )
    })

    it('handlePaginationChange writes the page param and drops it on page 1', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 2,
                pageSize: 50,
            })
        })
        expect(routerReplaceMock()).toHaveBeenCalledWith(
            '/analytics/card-business?page=3',
        )
        expect(result.current.pagination.pageIndex).toBe(2)

        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 0,
                pageSize: 50,
            })
        })
        expect(routerReplaceMock()).toHaveBeenLastCalledWith(
            '/analytics/card-business',
        )
    })

    it('syncs pagination back from URL changes', () => {
        const { result, rerender } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        expect(result.current.pagination.pageIndex).toBe(0)
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams('page=2') as unknown as ReadonlyURLSearchParams,
        )
        rerender()
        expect(result.current.pagination.pageIndex).toBe(1)
    })

    it('handleTableSortingChange maps sorting state to the sort param', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        act(() => {
            result.current.handleTableSortingChange([
                { id: 'refund', desc: true },
            ])
        })
        expect(routerReplaceMock()).toHaveBeenCalledWith(
            '/analytics/card-business?sort=refund%3Adesc',
        )
        expect(result.current.pagination.pageIndex).toBe(0)

        act(() => {
            result.current.handleTableSortingChange([])
        })
        expect(routerReplaceMock()).toHaveBeenLastCalledWith(
            '/analytics/card-business?sort=consumption%3Adesc',
        )
    })

    it('derives tableSorting from the sort param', () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams('sort=refund:asc') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        expect(result.current.tableSorting).toEqual([
            { id: 'refund', desc: false },
        ])
    })
})

describe('useCardBusinessPageState — 期间与日期口径', () => {
    it('applyPreset writes preset period and a date basis', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        const expected = resolvePeriod('last-month')
        act(() => {
            result.current.applyPreset('last-month')
        })
        expect(routerReplaceMock()).toHaveBeenCalledWith(
            `/analytics/card-business?from=${expected.from}&to=${expected.to}&periodPreset=last-month&dateBasis=consumption`,
        )
    })

    it('applyExplicitPeriod writes the explicit range and clears the preset', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        act(() => {
            result.current.setExplicitFrom('2026-08-01')
            result.current.setExplicitTo('2026-08-07')
            result.current.setExplicitDateBasis('expiry')
        })
        act(() => {
            result.current.applyExplicitPeriod()
        })
        expect(routerReplaceMock()).toHaveBeenCalledWith(
            '/analytics/card-business?from=2026-08-01&to=2026-08-07&dateBasis=expiry',
        )
    })

    it('applyExplicitPeriod ignores inverted ranges and incomplete inputs', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(undefined, false),
        )
        act(() => {
            result.current.setExplicitFrom('2026-08-09')
            result.current.setExplicitTo('2026-08-01')
            result.current.setExplicitDateBasis('consumption')
        })
        act(() => {
            result.current.applyExplicitPeriod()
        })
        expect(routerReplaceMock()).not.toHaveBeenCalled()
    })

    it('backfills a configured default date basis into an empty URL', async () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(makeBasisConfig('expiry'), true),
        )
        const resolved = resolvePeriod('month-to-date')
        await waitFor(() =>
            expect(routerReplaceMock()).toHaveBeenCalledWith(
                `/analytics/card-business?dateBasis=expiry&from=${resolved.from}&to=${resolved.to}`,
            ),
        )
        expect(result.current.analysisReady).toBe(false)
    })

    it('does not backfill when the URL already has a complete period', async () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams(
                'from=2026-08-01&to=2026-08-07&dateBasis=consumption',
            ) as unknown as ReadonlyURLSearchParams,
        )
        renderHook(() => useCardBusinessPageState(makeBasisConfig('expiry'), true))
        await act(async () => {
            await Promise.resolve()
        })
        expect(routerReplaceMock()).not.toHaveBeenCalled()
    })
})

describe('useCardBusinessPageState — 阻断与就绪', () => {
    it('blocks analysis when no default basis is configured and period is missing', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(makeBasisConfig(), true),
        )
        expect(result.current.analysisBlocked).toBe(true)
        expect(result.current.analysisReady).toBe(false)
        expect(result.current.analysisQuery).toBeNull()
    })

    it('is not blocked while the basis config is still loading', () => {
        const { result } = renderHook(() =>
            useCardBusinessPageState(makeBasisConfig(), false),
        )
        expect(result.current.analysisBlocked).toBe(false)
        expect(result.current.analysisReady).toBe(false)
    })

    it('is ready when a complete period is in the URL even without a default basis', () => {
        vi.mocked(useSearchParams).mockReturnValue(
            new URLSearchParams(
                'from=2026-08-01&to=2026-08-07&dateBasis=consumption',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() =>
            useCardBusinessPageState(makeBasisConfig(), true),
        )
        expect(result.current.analysisBlocked).toBe(false)
        expect(result.current.analysisReady).toBe(true)
        expect(result.current.analysisQuery?.from).toBe('2026-08-01')
    })
})
