import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
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

    afterEach(() => {
        vi.useRealTimers()
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
        expect(result.current.searchInput).toBe('')
        expect(result.current.hasActiveFilters).toBe(false)
    })

    it('parses every list filter from the url', () => {
        currentSearchParams = new URLSearchParams(
            'q=shoe&skuId=sku-1&supplierOfferingRevisionId=offer-1&mall=mall-1' +
                '&publicationStatus=PAUSED&deliveryStatus=failed&metric=paused&page=3',
        )
        const { result } = renderFilters()

        expect(result.current.qParam).toBe('shoe')
        expect(result.current.searchInput).toBe('shoe')
        expect(result.current.skuId).toBe('sku-1')
        expect(result.current.supplierOfferingRevisionId).toBe('offer-1')
        expect(result.current.mallId).toBe('mall-1')
        expect(result.current.publicationStatus).toBe('PAUSED')
        expect(result.current.deliveryStatus).toBe('failed')
        expect(result.current.metric).toBe('paused')
        expect(result.current.page).toBe(3)
        expect(result.current.hasActiveFilters).toBe(true)
    })

    it('falls back for invalid or missing metric values', () => {
        currentSearchParams = new URLSearchParams('metric=bogus')
        const first = renderFilters()
        expect(first.result.current.metric).toBe('all')
        first.unmount()

        currentSearchParams = new URLSearchParams()
        const second = renderFilters()
        expect(second.result.current.metric).toBe('all')
    })

    it('clamps page values to at least 1', () => {
        currentSearchParams = new URLSearchParams('page=0')
        const zero = renderFilters()
        expect(zero.result.current.page).toBe(1)
        zero.unmount()

        currentSearchParams = new URLSearchParams('page=abc')
        const nan = renderFilters()
        expect(nan.result.current.page).toBe(1)
    })

    it('debounces the search draft into the url and drops the page param', () => {
        vi.useFakeTimers()
        currentSearchParams = new URLSearchParams('page=2')
        const { result } = renderFilters()

        act(() => result.current.setSearchInput('socks'))
        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(navMocks.replace).toHaveBeenCalledTimes(1)
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?q=socks',
        )
    })

    it('does not write the url when the debounced draft is unchanged', () => {
        vi.useFakeTimers()
        currentSearchParams = new URLSearchParams('q=socks')
        const { result } = renderFilters()

        act(() => result.current.setSearchInput('socks'))
        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('commits the search immediately on Enter without waiting for debounce', () => {
        vi.useFakeTimers()
        const { result, rerender } = renderFilters()

        act(() => result.current.setSearchInput('  shoes  '))
        act(() => result.current.commitSearch())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?q=shoes',
        )

        // 提交后 URL 已写入：模拟路由回传，防抖回跳应视为无变化
        currentSearchParams = new URLSearchParams('q=shoes')
        act(() => {
            rerender()
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })
        expect(navMocks.replace).toHaveBeenCalledTimes(1)
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
        )

        navMocks.replace.mockClear()
        act(() => result.current.replaceParams({ metric: undefined }))
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?skuId=sku-1',
        )
    })

    it('clears every filter, resets pagination and blanks the search draft', () => {
        currentSearchParams = new URLSearchParams(
            'q=x&skuId=sku-1&mall=mall-1&publicationStatus=PAUSED&deliveryStatus=failed&metric=paused&page=4',
        )
        const { result } = renderFilters()

        act(() => result.current.setSearchInput('x'))
        act(() => result.current.clearFilters())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications',
        )
        expect(result.current.searchInput).toBe('')
    })

    it('navigates pages through the url page param and keeps pageSize local', () => {
        const { result } = renderFilters()

        act(() => {
            result.current.handlePaginationChange({ pageIndex: 2, pageSize: 50 })
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications?page=3',
        )
        expect(result.current.pageSize).toBe(50)

        navMocks.replace.mockClear()
        act(() => {
            result.current.handlePaginationChange({ pageIndex: 0, pageSize: 50 })
        })
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/commerce/publications',
        )
    })

    it('syncs the search draft when the url changes outside the search box', () => {
        const { result, rerender } = renderFilters()

        currentSearchParams = new URLSearchParams('q=external')
        act(() => {
            rerender()
        })

        expect(result.current.searchInput).toBe('external')
        expect(result.current.qParam).toBe('external')
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
})
