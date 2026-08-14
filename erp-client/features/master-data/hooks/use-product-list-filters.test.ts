import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { useProductListFilters } from './use-product-list-filters'

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    back: vi.fn(),
}))

let currentSearchParams = new URLSearchParams()

vi.mock('next/navigation', () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
    usePathname: () => '/master-data/products',
    useSearchParams: () => currentSearchParams,
    useParams: () => ({}),
}))

function renderFilters() {
    const searchInputRef = { current: null as HTMLInputElement | null }
    return renderHook(() => useProductListFilters(searchInputRef))
}

describe('useProductListFilters', () => {
    beforeEach(() => {
        navMocks.replace.mockClear()
        navMocks.push.mockClear()
        currentSearchParams = new URLSearchParams()
    })

    it('uses defaults for an empty url', () => {
        const { result } = renderFilters()

        expect(result.current.q).toBe('')
        expect(result.current.lifecycleStatus).toBe('all')
        expect(result.current.revisionTiming).toBe('all')
        expect(result.current.productKind).toBeUndefined()
        expect(result.current.metricKey).toBe('all')
        expect(result.current.hasStructuredProductFilters).toBe(false)
        expect(result.current.pagination.pageIndex).toBe(0)
        expect(result.current.pagination.pageSize).toBe(20)
    })

    it('parses every product filter from the url', () => {
        currentSearchParams = new URLSearchParams(
            'q=shoe&lifecycleStatus=enabled&revisionTiming=future&productKind=PHYSICAL' +
                '&productCategoryId=c1&productBrandId=b1&productSupplierId=s1' +
                '&productListingStatus=listed&productSupplyCoverage=complete' +
                '&productSalesPriceMin=1&productSalesPriceMax=99&page=3&metricKey=enabled',
        )
        const { result } = renderFilters()

        expect(result.current.q).toBe('shoe')
        expect(result.current.lifecycleStatus).toBe('enabled')
        expect(result.current.revisionTiming).toBe('future')
        expect(result.current.productKind).toBe('PHYSICAL')
        expect(result.current.productCategoryId).toBe('c1')
        expect(result.current.productBrandId).toBe('b1')
        expect(result.current.productSupplierId).toBe('s1')
        expect(result.current.productListingStatus).toBe('listed')
        expect(result.current.productSupplyCoverage).toBe('complete')
        expect(result.current.productSalesPriceMin).toBe('1')
        expect(result.current.productSalesPriceMax).toBe('99')
        expect(result.current.metricKey).toBe('enabled')
        expect(result.current.hasStructuredProductFilters).toBe(true)
        expect(result.current.pagination.pageIndex).toBe(2)
    })

    it('ignores invalid enum values from the url', () => {
        currentSearchParams = new URLSearchParams(
            'productKind=NOPE&productListingStatus=bogus&lifecycleStatus=z',
        )
        const { result } = renderFilters()

        expect(result.current.productKind).toBeUndefined()
        expect(result.current.productListingStatus).toBeUndefined()
        expect(result.current.lifecycleStatus).toBe('all')
    })

    it('commits a changed search draft to the url and resets pagination', () => {
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft('  socks  '))
        act(() => result.current.commitSearch())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/master-data/products?q=socks',
            { scroll: false },
        )
    })

    it('does not touch the url when the search draft is unchanged', () => {
        currentSearchParams = new URLSearchParams('q=socks')
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft('socks'))
        act(() => result.current.commitSearch())

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('switches lifecycle and keeps the metric key in sync', () => {
        const { result } = renderFilters()

        act(() => result.current.changeLifecycle('disabled'))
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/master-data/products?lifecycleStatus=disabled&metricKey=disabled',
            { scroll: false },
        )

        currentSearchParams = new URLSearchParams(
            'lifecycleStatus=disabled&metricKey=disabled',
        )
        navMocks.replace.mockClear()
        const fromDisabled = renderFilters()
        act(() => fromDisabled.result.current.changeLifecycle('all'))
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/master-data/products',
            { scroll: false },
        )
    })

    it('rejects an invalid sales price range without patching the url', () => {
        const { result } = renderFilters()

        act(() => result.current.setProductSalesPriceMinDraft('100'))
        act(() => result.current.setProductSalesPriceMaxDraft('99'))
        act(() => result.current.applyProductFilters())

        expect(result.current.productSalesPriceError).toBe(
            '最低价不能高于最高价',
        )
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('applies the structured filter drafts to the url', () => {
        const { result } = renderFilters()

        act(() => {
            result.current.setProductKindDraft('VOUCHER')
            result.current.setLifecycleStatusDraft('enabled')
            result.current.setRevisionTimingDraft('current')
            result.current.setProductListingStatusDraft('unlisted')
            result.current.setProductSupplyCoverageDraft('none')
            result.current.setProductCategoryIdDraft('c1')
            result.current.setProductBrandIdDraft('b1')
            result.current.setProductSupplierIdDraft('s1')
            result.current.setProductSalesPriceMinDraft('10')
            result.current.setProductSalesPriceMaxDraft('20')
            result.current.setSearchDraft('gift')
        })
        act(() => result.current.applyProductFilters())

        expect(result.current.productSalesPriceError).toBeNull()
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/master-data/products?q=gift&productKind=VOUCHER&lifecycleStatus=enabled' +
                '&metricKey=enabled&revisionTiming=current&productListingStatus=unlisted' +
                '&productSupplyCoverage=none&productCategoryId=c1&productBrandId=b1' +
                '&productSupplierId=s1&productSalesPriceMin=10&productSalesPriceMax=20',
            { scroll: false },
        )
    })

    it('clears every filter and resets pagination', () => {
        currentSearchParams = new URLSearchParams('q=x&productKind=PHYSICAL')
        const { result } = renderFilters()

        act(() => result.current.clearAllFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/master-data/products',
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe('')
        expect(result.current.productKindDraft).toBe('all')
    })

    it('navigates pages through the url page param', () => {
        const { result } = renderFilters()

        act(() => {
            result.current.changePagination({ pageIndex: 2, pageSize: 20 })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/master-data/products?page=3',
            { scroll: false },
        )
    })
})
