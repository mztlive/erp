import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { usePublicationListFilters } from './use-publication-list-filters'

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
    usePathname: () => '/commerce/publications',
    useSearchParams: () => currentSearchParams,
    useParams: () => ({}),
}))

function renderFilters() {
    return renderHook(() => usePublicationListFilters())
}

describe('usePublicationListFilters', () => {
    beforeEach(() => {
        navMocks.replace.mockClear()
        navMocks.push.mockClear()
        currentSearchParams = new URLSearchParams()
    })

    it('uses defaults for an empty url', () => {
        const { result } = renderFilters()

        expect(result.current.qParam).toBe('')
        expect(result.current.skuId).toBeUndefined()
        expect(result.current.mallId).toBeUndefined()
        expect(result.current.supplierOfferingRevisionId).toBeUndefined()
        expect(result.current.publicationStatus).toBe('all')
        expect(result.current.deliveryStatus).toBe('all')
        expect(result.current.metric).toBe('all')
        expect(result.current.page).toBe(1)
        expect(result.current.pageSize).toBe(20)
        expect(result.current.searchDraft).toBe('')
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
    })

    it('parses every list filter from the url', () => {
        currentSearchParams = new URLSearchParams(
            'q=shoe&skuId=sku-1&supplierOfferingRevisionId=offer-1&mall=mall-1' +
                '&publicationStatus=PAUSED&deliveryStatus=failed&metric=paused&page=3',
        )
        const { result } = renderFilters()

        expect(result.current.qParam).toBe('shoe')
        expect(result.current.searchDraft).toBe('shoe')
        expect(result.current.skuId).toBe('sku-1')
        expect(result.current.supplierOfferingRevisionId).toBe('offer-1')
        expect(result.current.mallId).toBe('mall-1')
        expect(result.current.publicationStatus).toBe('PAUSED')
        expect(result.current.deliveryStatus).toBe('failed')
        expect(result.current.metric).toBe('paused')
        expect(result.current.page).toBe(3)
        expect(result.current.hasActiveFilters).toBe(true)
        expect(result.current.hasStructuredFilters).toBe(true)
        // 深链带结构化条件时面板必须自动展开，条件要可见可改
        expect(result.current.panelOpen).toBe(true)
    })

    it('falls back to defaults for invalid enum values', () => {
        currentSearchParams = new URLSearchParams(
            'publicationStatus=bogus&deliveryStatus=bogus&metric=bogus&page=abc',
        )
        const { result } = renderFilters()

        expect(result.current.publicationStatus).toBe('all')
        expect(result.current.deliveryStatus).toBe('all')
        expect(result.current.metric).toBe('all')
        expect(result.current.page).toBe(1)
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it('clamps page values to at least 1', () => {
        currentSearchParams = new URLSearchParams('page=0')
        const zero = renderFilters()
        expect(zero.result.current.page).toBe(1)
        zero.unmount()

        currentSearchParams = new URLSearchParams('page=-3')
        const negative = renderFilters()
        expect(negative.result.current.page).toBe(1)
    })

    it('keeps the panel closed when only a keyword or quick filter is applied', () => {
        currentSearchParams = new URLSearchParams('q=shoe&metric=paused')
        const { result } = renderFilters()

        expect(result.current.panelOpen).toBe(false)
    })

    it('never writes the url while drafts are being edited', () => {
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft('socks'))
        act(() => result.current.setMallDraft('mall-1'))
        act(() => result.current.setPublicationStatusDraft('PAUSED'))
        act(() => result.current.setDeliveryStatusDraft('failed'))

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('applies every draft at once, resets the page, clears the metric shortcut and closes the panel', () => {
        currentSearchParams = new URLSearchParams('page=3&metric=paused')
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft('shoes'))
        act(() => result.current.setMallDraft('mall-1'))
        act(() => result.current.setDeliveryStatusDraft('failed'))
        act(() => result.current.applyFilters())

        expect(navMocks.replace).toHaveBeenCalledTimes(1)
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?q=shoes&mall=mall-1&deliveryStatus=failed',
            { scroll: false },
        )
        expect(result.current.panelOpen).toBe(false)
    })

    it('trims the keyword and omits default values from the url', () => {
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft('  shoes  '))
        act(() => result.current.applyFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?q=shoes',
            { scroll: false },
        )
    })

    it('keeps the metric shortcut when only the keyword changes', () => {
        currentSearchParams = new URLSearchParams('metric=paused')
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft('shoes'))
        act(() => result.current.applyFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?metric=paused&q=shoes',
            { scroll: false },
        )
    })

    it('clears every filter, resets pagination, drafts and the panel; keeps view and navigation params', () => {
        currentSearchParams = new URLSearchParams(
            'q=x&skuId=sku-1&supplierOfferingRevisionId=offer-1&mall=mall-1' +
                '&publicationStatus=PAUSED&deliveryStatus=failed&metric=paused' +
                '&page=4&view=default&returnTo=%2Fcommerce%2Fpublications',
        )
        const { result } = renderFilters()

        act(() => result.current.setSearchDraft('x'))
        act(() => result.current.clearAllFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?view=default&returnTo=%2Fcommerce%2Fpublications',
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe('')
        expect(result.current.mallDraft).toBeNull()
        expect(result.current.publicationStatusDraft).toBe('all')
        expect(result.current.deliveryStatusDraft).toBe('all')
        expect(result.current.panelOpen).toBe(false)
    })

    it('resets only the more-filter conditions, keeps the quick filters and the panel open', () => {
        currentSearchParams = new URLSearchParams(
            'q=x&skuId=sku-1&mall=mall-1&publicationStatus=PAUSED' +
                '&deliveryStatus=failed&metric=paused',
        )
        const { result } = renderFilters()

        act(() => result.current.setPanelOpen(true))
        act(() => result.current.resetMoreFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?q=x&skuId=sku-1&metric=paused',
            { scroll: false },
        )
        expect(result.current.mallDraft).toBeNull()
        expect(result.current.publicationStatusDraft).toBe('all')
        expect(result.current.deliveryStatusDraft).toBe('all')
        expect(result.current.panelOpen).toBe(true)
    })

    it('removes a single applied condition without touching the others', () => {
        currentSearchParams = new URLSearchParams(
            'q=pen&mall=mall-1&publicationStatus=PAUSED&deliveryStatus=failed',
        )
        const { result } = renderFilters()

        act(() => result.current.removeFilter('publicationStatus'))

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?q=pen&mall=mall-1&deliveryStatus=failed',
            { scroll: false },
        )
        expect(result.current.publicationStatusDraft).toBe('all')
    })

    it('removes the keyword chip and blanks the search draft', () => {
        currentSearchParams = new URLSearchParams('q=pen&metric=paused')
        const { result } = renderFilters()

        act(() => result.current.removeFilter('q'))

        expect(result.current.searchDraft).toBe('')
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?metric=paused',
            { scroll: false },
        )
    })

    it('removes both source-lock conditions from a single source chip', () => {
        currentSearchParams = new URLSearchParams(
            'skuId=sku-1&supplierOfferingRevisionId=offer-1&page=2',
        )
        const { result } = renderFilters()

        act(() => result.current.removeFilter('skuId'))

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications',
            { scroll: false },
        )
    })

    it('syncs drafts when the url changes and does not force the panel open again', () => {
        currentSearchParams = new URLSearchParams('mall=mall-1&page=2')
        const { result, rerender } = renderFilters()
        expect(result.current.panelOpen).toBe(true)

        // 用户手动收起面板；随后 URL 回填不得重新展开
        act(() => result.current.setPanelOpen(false))
        act(() => result.current.setMallDraft('mall-draft'))

        currentSearchParams = new URLSearchParams('mall=mall-2&q=external')
        act(() => {
            rerender()
        })

        expect(result.current.mallDraft).toBe('mall-2')
        expect(result.current.searchDraft).toBe('external')
        expect(result.current.panelOpen).toBe(false)
    })

    it('protects an in-progress keyword draft from url refills', () => {
        currentSearchParams = new URLSearchParams('q=old')
        const { result, rerender } = renderFilters()

        act(() => result.current.setSearchDraft('typing'))
        const input = document.createElement('input')
        document.body.appendChild(input)
        input.focus()
        result.current.searchInputRef.current = input

        currentSearchParams = new URLSearchParams('q=from-url')
        act(() => {
            rerender()
        })

        expect(result.current.searchDraft).toBe('typing')
        input.remove()
        result.current.searchInputRef.current = null
    })

    it('navigates pages through the url page param and keeps pageSize local', () => {
        const { result } = renderFilters()

        act(() => {
            result.current.handlePaginationChange({ pageIndex: 2, pageSize: 50 })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?page=3',
            { scroll: false },
        )
        expect(result.current.pageSize).toBe(50)

        navMocks.replace.mockClear()
        act(() => {
            result.current.handlePaginationChange({ pageIndex: 0, pageSize: 50 })
        })
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications',
            { scroll: false },
        )
    })

    it('removes keys patched with undefined or "all" and resets the page param', () => {
        currentSearchParams = new URLSearchParams('skuId=sku-1&page=2')
        const { result } = renderFilters()

        act(() =>
            result.current.replaceParams({
                mall: undefined,
                skuId: 'all',
                metric: 'paused',
            }),
        )

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?metric=paused',
            { scroll: false },
        )
    })

    it('focuses the search input on the "/" shortcut outside form fields', () => {
        const { result } = renderFilters()
        const focusSpy = vi.fn()
        result.current.searchInputRef.current = {
            focus: focusSpy,
        } as unknown as HTMLInputElement

        const event = new KeyboardEvent('keydown', {
            key: '/',
            bubbles: true,
            cancelable: true,
        })
        act(() => {
            document.dispatchEvent(event)
        })

        expect(event.defaultPrevented).toBe(true)
        expect(focusSpy).toHaveBeenCalledTimes(1)
    })

    it('ignores the "/" shortcut while typing in an input', () => {
        renderFilters()

        const input = document.createElement('input')
        document.body.appendChild(input)
        input.focus()

        const event = new KeyboardEvent('keydown', { key: '/', bubbles: true })
        act(() => {
            input.dispatchEvent(event)
        })

        expect(event.defaultPrevented).toBe(false)
        input.remove()
    })

    it('ignores the "/" shortcut while a dialog or sheet is open', () => {
        const { result } = renderFilters()
        const focusSpy = vi.fn()
        result.current.searchInputRef.current = {
            focus: focusSpy,
        } as unknown as HTMLInputElement

        const dialog = document.createElement('div')
        dialog.setAttribute('role', 'dialog')
        document.body.appendChild(dialog)

        const event = new KeyboardEvent('keydown', {
            key: '/',
            bubbles: true,
            cancelable: true,
        })
        act(() => {
            document.dispatchEvent(event)
        })

        expect(event.defaultPrevented).toBe(false)
        expect(focusSpy).not.toHaveBeenCalled()
        dialog.remove()
    })
})
