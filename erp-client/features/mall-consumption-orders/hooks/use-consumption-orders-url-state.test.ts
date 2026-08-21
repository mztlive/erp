import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useConsumptionOrdersUrlState } from './use-consumption-orders-url-state'

vi.mock('next/navigation', () => ({
    useRouter: vi.fn(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() })),
    useSearchParams: vi.fn(() => params()),
    usePathname: vi.fn(() => '/test'),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useRouter, useSearchParams } from 'next/navigation'
import type { ReadonlyURLSearchParams } from 'next/navigation'

/** next/navigation 的 useSearchParams 返回只读包装；测试里以 URLSearchParams 兜底。 */
const params = (raw?: string) =>
    new URLSearchParams(raw) as unknown as ReadonlyURLSearchParams

const mockedSearchParams = vi.mocked(useSearchParams)
const mockedPathname = vi.mocked(usePathname)
const mockedRouter = vi.mocked(useRouter)

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
    mockedPathname.mockReturnValue('/test')
    setupRouter()
})

describe('useConsumptionOrdersUrlState', () => {
    it('applies defaults when no params are present', () => {
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        expect(result.current.qParam).toBe('')
        expect(result.current.mallId).toBe('all')
        expect(result.current.fulfillmentChain).toBe('all')
        expect(result.current.attributionStatus).toBe('all')
        expect(result.current.paymentSource).toBe('all')
        expect(result.current.costBasis).toBe('all')
        expect(result.current.occurredFrom).toBe('')
        expect(result.current.occurredTo).toBe('')
        expect(result.current.factTypes).toEqual([])
        expect(result.current.supplierStatuses).toEqual([])
        expect(result.current.dataSources).toEqual([])
        expect(result.current.periodSelected).toBe(false)
        expect(result.current.metric).toBe('all')
        expect(result.current.previewId).toBeNull()
        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 8,
        })
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it('parses single-value and multi-value params from the URL', () => {
        mockedSearchParams.mockReturnValue(
            params(
                'q=SO-1&mall=m1&fulfillmentChain=ERP_AUTOMATED&attributionStatus=DIFFERENCE&paymentSource=CARD&costBasis=ACTUAL&occurredFrom=2026-08-01&occurredTo=2026-08-07&factType=PAYMENT_SUCCEEDED,REFUND_SUCCEEDED&supplierStatus=SHIPPED,COMPLETED&dataSource=REALTIME,BACKFILL&metric=paid&preview=mo-9&page=3&size=20',
            ),
        )
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        expect(result.current.qParam).toBe('SO-1')
        expect(result.current.mallId).toBe('m1')
        expect(result.current.fulfillmentChain).toBe('ERP_AUTOMATED')
        expect(result.current.attributionStatus).toBe('DIFFERENCE')
        expect(result.current.paymentSource).toBe('CARD')
        expect(result.current.costBasis).toBe('ACTUAL')
        expect(result.current.occurredFrom).toBe('2026-08-01')
        expect(result.current.occurredTo).toBe('2026-08-07')
        expect(result.current.factTypes).toEqual([
            'PAYMENT_SUCCEEDED',
            'REFUND_SUCCEEDED',
        ])
        expect(result.current.supplierStatuses).toEqual([
            'SHIPPED',
            'COMPLETED',
        ])
        expect(result.current.dataSources).toEqual(['REALTIME', 'BACKFILL'])
        expect(result.current.periodSelected).toBe(true)
        expect(result.current.metric).toBe('paid')
        expect(result.current.previewId).toBe('mo-9')
        expect(result.current.pagination).toEqual({ pageIndex: 2, pageSize: 20 })
        expect(result.current.hasActiveFilters).toBe(true)
    })

    it('ignores unknown values in multi-value params', () => {
        mockedSearchParams.mockReturnValue(
            params(
                'factType=BOGUS,PAYMENT_SUCCEEDED&supplierStatus=NOPE&dataSource=JUNK&metric=bogus',
            ),
        )
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        expect(result.current.factTypes).toEqual(['PAYMENT_SUCCEEDED'])
        expect(result.current.supplierStatuses).toEqual([])
        expect(result.current.dataSources).toEqual([])
        expect(result.current.metric).toBe('all')
    })

    it('clamps page and size from the URL', () => {
        mockedSearchParams.mockReturnValue(
            params('page=0&size=999'),
        )
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        expect(result.current.pagination.pageIndex).toBe(0)
        expect(result.current.pagination.pageSize).toBe(50)
    })

    it('derives the list query input from URL state', () => {
        mockedSearchParams.mockReturnValue(
            params(
                'q=x&mall=m1&fulfillmentChain=LEGACY_MANUAL&attributionStatus=PENDING&paymentSource=WECHAT&costBasis=NONE&occurredFrom=2026-08-01&occurredTo=2026-08-02&factType=ORDER_CANCELED&supplierStatus=EXCEPTION&dataSource=BACKFILL&metric=cost_none&page=2&size=10',
            ),
        )
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        expect(result.current.listQueryInput).toEqual({
            q: 'x',
            mallIds: ['m1'],
            occurredFrom: '2026-08-01',
            occurredTo: '2026-08-02',
            factTypes: ['ORDER_CANCELED'],
            fulfillmentChains: ['LEGACY_MANUAL'],
            attributionStatuses: ['PENDING'],
            paymentSources: ['WECHAT'],
            supplierStatuses: ['EXCEPTION'],
            costBases: ['NONE'],
            dataSources: ['BACKFILL'],
            metric: 'cost_none',
            page: 2,
            pageSize: 10,
            sort: 'occurredAt.desc',
        })
    })

    it('omits filter fields from the query input when they are unset', () => {
        mockedSearchParams.mockReturnValue(
            params('occurredFrom=2026-08-01&occurredTo=2026-08-02'),
        )
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        const input = result.current.listQueryInput
        expect(input.q).toBeUndefined()
        expect(input.mallIds).toBeUndefined()
        expect(input.factTypes).toBeUndefined()
        expect(input.fulfillmentChains).toBeUndefined()
        expect(input.attributionStatuses).toBeUndefined()
        expect(input.paymentSources).toBeUndefined()
        expect(input.supplierStatuses).toBeUndefined()
        expect(input.costBases).toBeUndefined()
        expect(input.dataSources).toBeUndefined()
        expect(input.metric).toBeUndefined()
        expect(input.page).toBe(1)
        expect(input.pageSize).toBe(8)
        expect(input.sort).toBe('occurredAt.desc')
    })

    it('replaceParams writes changed params to the URL and resets page', () => {
        mockedSearchParams.mockReturnValue(
            params('mall=m1&page=4&size=10'),
        )
        mockedPathname.mockReturnValue('/commerce/consumption-orders')
        const router = setupRouter()
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        act(() => {
            result.current.replaceParams({ q: 'hello', mall: 'm2' })
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders?mall=m2&size=10&q=hello',
            { scroll: false },
        )
        expect(result.current.pagination.pageIndex).toBe(0)
    })

    it('replaceParams deletes params set to undefined or "all"', () => {
        mockedSearchParams.mockReturnValue(
            params('q=old&mall=m1'),
        )
        mockedPathname.mockReturnValue('/commerce/consumption-orders')
        const router = setupRouter()
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        act(() => {
            result.current.replaceParams({ q: undefined, mall: 'all' })
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders',
            { scroll: false },
        )
    })

    it('replaceParams keeps the current page when resetPage is false', () => {
        mockedSearchParams.mockReturnValue(
            params('page=3&size=8'),
        )
        mockedPathname.mockReturnValue('/commerce/consumption-orders')
        const router = setupRouter()
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        act(() => {
            result.current.replaceParams({ preview: 'mo-1' }, false)
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders?page=3&size=8&preview=mo-1',
            { scroll: false },
        )
        expect(result.current.pagination.pageIndex).toBe(2)
    })

    it('handlePaginationChange writes page/size and omits defaults', () => {
        mockedSearchParams.mockReturnValue(params('q=a'))
        mockedPathname.mockReturnValue('/commerce/consumption-orders')
        const router = setupRouter()
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        act(() => {
            result.current.handlePaginationChange({
                pageIndex: 2,
                pageSize: 20,
            })
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders?q=a&page=3&size=20',
            { scroll: false },
        )
        expect(result.current.pagination).toEqual({ pageIndex: 2, pageSize: 20 })

        act(() => {
            result.current.handlePaginationChange({ pageIndex: 0, pageSize: 8 })
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders?q=a',
            { scroll: false },
        )
    })

    it('openPreview/closePreview keep pagination and only touch the preview param', () => {
        mockedSearchParams.mockReturnValue(params('page=2'))
        mockedPathname.mockReturnValue('/commerce/consumption-orders')
        const router = setupRouter()
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        act(() => {
            result.current.openPreview('mo-5')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders?page=2&preview=mo-5',
            { scroll: false },
        )

        mockedSearchParams.mockReturnValue(
            params('page=2&preview=mo-5'),
        )
        act(() => {
            result.current.closePreview()
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders?page=2',
            { scroll: false },
        )
    })

    it('toggleMetric activates and deactivates the metric param', () => {
        mockedSearchParams.mockReturnValue(params())
        mockedPathname.mockReturnValue('/commerce/consumption-orders')
        const router = setupRouter()
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        act(() => {
            result.current.toggleMetric('paid')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/commerce/consumption-orders?metric=paid',
            { scroll: false },
        )
    })

    it('synchronizes pagination when the URL page changes externally', () => {
        mockedSearchParams.mockReturnValue(params('page=2'))
        const { result, rerender } = renderHook(() =>
            useConsumptionOrdersUrlState(),
        )
        expect(result.current.pagination.pageIndex).toBe(1)

        mockedSearchParams.mockReturnValue(params('page=4'))
        rerender()
        expect(result.current.pagination.pageIndex).toBe(3)
    })

    it('builds the return href with the current query string', () => {
        mockedSearchParams.mockReturnValue(
            params('q=a&metric=paid'),
        )
        mockedPathname.mockReturnValue('/commerce/consumption-orders')
        const { result } = renderHook(() => useConsumptionOrdersUrlState())

        expect(result.current.listReturnHref).toBe(
            '/commerce/consumption-orders?q=a&metric=paid',
        )
    })
})
