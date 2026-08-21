import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act, render, fireEvent, cleanup } from '@testing-library/react'
import * as React from 'react'

import { useConsumptionOrdersFilters } from './use-consumption-orders-filters'

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

function SearchHarness() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    useConsumptionOrdersFilters(searchInputRef)
    return React.createElement('input', {
        ref: searchInputRef,
        'aria-label': 'search',
    })
}

beforeEach(() => {
    vi.clearAllMocks()
    mockedSearchParams.mockReturnValue(params())
    mockedPathname.mockReturnValue('/test')
    setupRouter()
})

afterEach(() => {
    cleanup()
})

describe('useConsumptionOrdersFilters', () => {
    it('initializes drafts from the URL and keeps the panel closed without structured filters', () => {
        mockedSearchParams.mockReturnValue(
            params('q=SO-1&occurredFrom=2026-08-01&occurredTo=2026-08-07&metric=paid'),
        )
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        expect(result.current.searchDraft).toBe('SO-1')
        expect(result.current.filterDraft.occurredFrom).toBe('2026-08-01')
        expect(result.current.filterDraft.occurredTo).toBe('2026-08-07')
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
    })

    it('opens the panel on initial deep links that carry structured filters', () => {
        mockedSearchParams.mockReturnValue(
            params('factType=PAYMENT_SUCCEEDED&occurredFrom=2026-08-01&occurredTo=2026-08-07'),
        )
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        expect(result.current.panelOpen).toBe(true)
        expect(result.current.hasStructuredFilters).toBe(true)
    })

    it('applyFilters patches q and every panel field, resets the page and closes the panel', () => {
        const router = setupRouter()
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.setSearchDraft(' SO-2 ')
            result.current.setFilterDraft({
                mallId: 'm1',
                attributionStatus: 'PENDING',
                fulfillmentChain: 'ERP_AUTOMATED',
                paymentSource: 'CARD',
                costBasis: 'NONE',
                factTypes: ['PAYMENT_SUCCEEDED', 'REFUND_SUCCEEDED'],
                supplierStatuses: ['SHIPPED'],
                dataSources: ['BACKFILL'],
                occurredFrom: '2026-08-01',
                occurredTo: '2026-08-07',
            })
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/test?q=SO-2&mall=m1&attributionStatus=PENDING&fulfillmentChain=ERP_AUTOMATED&paymentSource=CARD&costBasis=NONE&factType=PAYMENT_SUCCEEDED%2CREFUND_SUCCEEDED&supplierStatus=SHIPPED&dataSource=BACKFILL&occurredFrom=2026-08-01&occurredTo=2026-08-07',
            { scroll: false },
        )
        expect(result.current.panelOpen).toBe(false)
    })

    it('omits defaults and "all" values from the URL on apply', () => {
        const router = setupRouter()
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.applyFilters()
        })

        expect(router.replace).toHaveBeenCalledWith('/test', {
            scroll: false,
        })
    })

    it('does not touch the URL while the draft changes', () => {
        const router = setupRouter()
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.setSearchDraft('abc')
            result.current.setFilterDraft((draft) => ({
                ...draft,
                costBasis: 'ACTUAL',
                mallId: 'm1',
            }))
        })

        expect(router.replace).not.toHaveBeenCalled()
    })

    it('clearAllFilters resets drafts and the panel, clears filter params and keeps the period', () => {
        mockedSearchParams.mockReturnValue(
            params(
                'q=a&mall=m1&occurredFrom=2026-08-01&occurredTo=2026-08-07&factType=ORDER_CANCELED&attributionStatus=PENDING&supplierStatus=SHIPPED&paymentSource=CARD&costBasis=ACTUAL&dataSource=REALTIME&fulfillmentChain=LEGACY_MANUAL&metric=paid&page=5&size=10',
            ),
        )
        const router = setupRouter()
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.setPanelOpen(true)
            result.current.clearAllFilters()
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/test?occurredFrom=2026-08-01&occurredTo=2026-08-07&size=10',
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe('')
        expect(result.current.filterDraft).toEqual({
            mallId: '',
            attributionStatus: 'all',
            fulfillmentChain: 'all',
            paymentSource: 'all',
            costBasis: 'all',
            factTypes: [],
            supplierStatuses: [],
            dataSources: [],
            // 期间是分析维度：清空全部后草稿跟随 URL 保留
            occurredFrom: '2026-08-01',
            occurredTo: '2026-08-07',
        })
        expect(result.current.panelOpen).toBe(false)
    })

    it('resetMoreFilters clears structured conditions but keeps q, metric, the period and the panel', () => {
        mockedSearchParams.mockReturnValue(
            params(
                'q=a&mall=m1&occurredFrom=2026-08-01&occurredTo=2026-08-07&factType=ORDER_CANCELED&attributionStatus=PENDING&metric=paid',
            ),
        )
        const router = setupRouter()
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.setPanelOpen(true)
            result.current.resetMoreFilters()
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/test?q=a&occurredFrom=2026-08-01&occurredTo=2026-08-07&metric=paid',
            { scroll: false },
        )
        expect(result.current.filterDraft.attributionStatus).toBe('all')
        expect(result.current.filterDraft.factTypes).toEqual([])
        expect(result.current.filterDraft.mallId).toBe('')
        expect(result.current.filterDraft.occurredFrom).toBe('2026-08-01')
        expect(result.current.filterDraft.occurredTo).toBe('2026-08-07')
        expect(result.current.searchDraft).toBe('a')
        expect(result.current.panelOpen).toBe(true)
    })

    it('URL backfill syncs the draft without reopening the panel', () => {
        const { result, rerender } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.setPanelOpen(true)
            result.current.applyFilters()
        })
        // 提交后回填 effect 会把草稿同步回 URL；面板保持用户关闭后的状态
        expect(result.current.panelOpen).toBe(false)

        mockedSearchParams.mockReturnValue(
            params('mall=m2&occurredFrom=2026-08-01&occurredTo=2026-08-07'),
        )
        rerender()

        expect(result.current.filterDraft.mallId).toBe('m2')
        expect(result.current.filterDraft.occurredFrom).toBe('2026-08-01')
        // URL 回填不重新强制展开面板
        expect(result.current.panelOpen).toBe(false)
    })

    it('removeFilter removes a single applied condition', () => {
        mockedSearchParams.mockReturnValue(
            params('q=a&mall=m1&occurredFrom=2026-08-01&occurredTo=2026-08-07&metric=paid'),
        )
        const router = setupRouter()
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.removeFilter('mall')
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/test?q=a&occurredFrom=2026-08-01&occurredTo=2026-08-07&metric=paid',
            { scroll: false },
        )
    })

    it('removeFilter("q") clears the keyword draft immediately', () => {
        mockedSearchParams.mockReturnValue(params('q=abc'))
        const { result } = renderHook(() =>
            useConsumptionOrdersFilters({ current: null }),
        )

        act(() => {
            result.current.removeFilter('q')
        })

        expect(result.current.searchDraft).toBe('')
    })

    it('focuses the search input on "/" outside form fields', () => {
        const { getByLabelText } = render(
            React.createElement(SearchHarness),
        )
        const input = getByLabelText('search') as HTMLInputElement
        input.blur()

        fireEvent.keyDown(window, { key: '/' })

        expect(document.activeElement).toBe(input)
    })

    it('ignores "/" with modifier keys', () => {
        const { getByLabelText } = render(
            React.createElement(SearchHarness),
        )
        const input = getByLabelText('search') as HTMLInputElement
        input.blur()

        fireEvent.keyDown(window, { key: '/', metaKey: true })
        fireEvent.keyDown(window, { key: '/', ctrlKey: true })
        fireEvent.keyDown(window, { key: '/', altKey: true })

        expect(document.activeElement).not.toBe(input)
    })

    it('does not steal focus while typing inside an input', () => {
        const { getByLabelText } = render(
            React.createElement(SearchHarness),
        )
        const input = getByLabelText('search') as HTMLInputElement
        input.focus()

        fireEvent.keyDown(input, { key: '/' })

        expect(document.activeElement).toBe(input)
    })

    it('ignores "/" while a dialog or sheet is open', () => {
        const { getByLabelText } = render(
            React.createElement(SearchHarness),
        )
        const input = getByLabelText('search') as HTMLInputElement
        input.blur()
        const dialog = document.createElement('div')
        dialog.setAttribute('role', 'dialog')
        document.body.appendChild(dialog)

        fireEvent.keyDown(window, { key: '/' })

        expect(document.activeElement).not.toBe(input)
        dialog.remove()
    })
})
