import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useCustomerReceivablesUrlState } from './use-customer-receivables-url-state'

vi.mock('next/navigation', () => ({
    useRouter: vi.fn(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() })),
    useSearchParams: vi.fn(() => new URLSearchParams() as unknown as ReadonlyURLSearchParams),
    usePathname: vi.fn(() => '/finance/customer-accounts'),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useRouter, useSearchParams } from 'next/navigation'
import type { ReadonlyURLSearchParams } from 'next/navigation'

const mockedRouter = vi.mocked(useRouter)
const mockedSearchParams = vi.mocked(useSearchParams)
const mockedPathname = vi.mocked(usePathname)

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
    mockedSearchParams.mockReturnValue(new URLSearchParams() as unknown as ReadonlyURLSearchParams)
    mockedPathname.mockReturnValue('/finance/customer-accounts')
    setupRouter()
})

afterEach(() => {
    vi.useRealTimers()
})

describe('useCustomerReceivablesUrlState', () => {
    it('applies defaults when no params are present', () => {
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.view).toBe('receivable')
        expect(result.current.qParam).toBe('')
        expect(result.current.counterpartyPartyId).toBeUndefined()
        expect(result.current.customerId).toBeUndefined()
        expect(result.current.due).toBeUndefined()
        expect(result.current.status).toBeUndefined()
        expect(result.current.reviewStatus).toBeUndefined()
        expect(result.current.focusId).toBeUndefined()
        expect(result.current.salesOrderId).toBeUndefined()
        expect(result.current.receivableAccountId).toBeUndefined()
        expect(result.current.returnTo).toBeUndefined()
        expect(result.current.from).toBeUndefined()
        expect(result.current.sessionId).toBeUndefined()
        expect(result.current.previewKind).toBeNull()
        expect(result.current.previewId).toBeUndefined()
        expect(result.current.workItemId).toBeUndefined()
        expect(result.current.searchInput).toBe('')
        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.pageFromUrl).toBe(1)
        expect(result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 20,
        })
    })

    it('parses every param from the URL', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receipt&q=SO-1&counterpartyId=p1&customerId=c1&due=overdue' +
                    '&status=open&reviewStatus=reviewed&focusId=f1&salesOrderId=s1' +
                    '&receivableAccountId=r1&returnTo=%2Forders&from=W05' +
                    '&sessionId=ses1&previewKind=receipt&previewId=prv1&page=3' +
                    '&currentWorkItemId=wi-cr-1',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.view).toBe('receipt')
        expect(result.current.qParam).toBe('SO-1')
        expect(result.current.counterpartyPartyId).toBe('p1')
        expect(result.current.customerId).toBe('c1')
        expect(result.current.due).toBe('overdue')
        expect(result.current.status).toBe('open')
        expect(result.current.reviewStatus).toBe('reviewed')
        expect(result.current.focusId).toBe('f1')
        expect(result.current.salesOrderId).toBe('s1')
        expect(result.current.receivableAccountId).toBe('r1')
        expect(result.current.returnTo).toBe('/orders')
        expect(result.current.from).toBe('W05')
        expect(result.current.sessionId).toBe('ses1')
        expect(result.current.previewKind).toBe('receipt')
        expect(result.current.previewId).toBe('prv1')
        expect(result.current.workItemId).toBe('wi-cr-1')
        expect(result.current.searchInput).toBe('SO-1')
        expect(result.current.pageFromUrl).toBe(3)
        expect(result.current.pagination.pageIndex).toBe(2)
    })

    it('parses refund previewKind without treating it as invoice', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receipt&previewKind=refund&previewId=crf-1',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.previewKind).toBe('refund')
        expect(result.current.previewId).toBe('crf-1')
    })

    it('parses reversal previewKind without treating it as refund or invoice', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receipt&previewKind=reversal&previewId=rr-1',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.previewKind).toBe('reversal')
        expect(result.current.previewId).toBe('rr-1')
    })

    it('keeps invoice previewKind on the no-approval path', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=sales_invoice&previewKind=invoice&previewId=inv-1',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.previewKind).toBe('invoice')
        expect(result.current.previewId).toBe('inv-1')
    })

    it('reads workItemId when currentWorkItemId is absent', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receipt&workItemId=wi-cr-2',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.workItemId).toBe('wi-cr-2')
    })

    it('clamps invalid page values back to page 1', () => {
        mockedSearchParams.mockReturnValue(new URLSearchParams('page=abc') as unknown as ReadonlyURLSearchParams)
        const first = renderHook(() => useCustomerReceivablesUrlState())
        expect(first.result.current.pageFromUrl).toBe(1)
        first.unmount()

        mockedSearchParams.mockReturnValue(new URLSearchParams('page=0') as unknown as ReadonlyURLSearchParams)
        const second = renderHook(() => useCustomerReceivablesUrlState())
        expect(second.result.current.pageFromUrl).toBe(1)
        second.unmount()

        mockedSearchParams.mockReturnValue(new URLSearchParams('page=-2') as unknown as ReadonlyURLSearchParams)
        const third = renderHook(() => useCustomerReceivablesUrlState())
        expect(third.result.current.pageFromUrl).toBe(1)
    })

    it('ignores unknown enum values by falling back to defaults', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=bogus&due=soon') as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.view).toBe('receivable')
        expect(result.current.due).toBeUndefined()
    })

    it('builds the list query from params with empty values coalesced', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receivable&q=&counterpartyId=p1&due=all&returnTo=%2Forders&from=W05',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.query).toEqual({
            view: 'receivable',
            q: undefined,
            counterpartyPartyId: 'p1',
            customerId: undefined,
            due: 'all',
            status: undefined,
            reviewStatus: undefined,
            salesOrderId: undefined,
            receivableAccountId: undefined,
            returnTo: '/orders',
            from: 'W05',
        })
    })

    it('derives hasActiveFilters from filter params, ignoring blank q', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&q=%20%20') as unknown as ReadonlyURLSearchParams,
        )
        const blank = renderHook(() => useCustomerReceivablesUrlState())
        expect(blank.result.current.hasActiveFilters).toBe(false)
        blank.unmount()

        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&status=open') as unknown as ReadonlyURLSearchParams,
        )
        const filtered = renderHook(() => useCustomerReceivablesUrlState())
        expect(filtered.result.current.hasActiveFilters).toBe(true)
    })

    it('patchUrl removes null values, sets strings and keeps the view param', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&q=x&due=overdue') as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.patchUrl(
                { due: null, q: 'SO-2', page: '2' },
                { replace: true },
            )
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&q=SO-2&page=2',
        )

        act(() => {
            result.current.patchUrl({ q: null }, { replace: false })
        })
        expect(router.push).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&due=overdue',
        )
    })

    it('clearFilters resets every filter param, page and the search input', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receipt&q=abc&counterpartyId=p1&customerId=c1&due=overdue' +
                    '&status=open&reviewStatus=reviewed&salesOrderId=s1' +
                    '&receivableAccountId=r1&focusId=f1&previewKind=receipt' +
                    '&previewId=prv1&page=3',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.clearFilters()
        })
        expect(result.current.searchInput).toBe('')
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receipt',
        )
    })

    it('handlePaginationChange removes the page param on page 1 and sets it otherwise', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&page=3') as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.handlePaginationChange({ pageIndex: 0, pageSize: 20 })
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable',
        )

        act(() => {
            result.current.handlePaginationChange({ pageIndex: 4, pageSize: 20 })
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&page=5',
        )
    })

    it('debounces search input into the q param and resets the page', () => {
        vi.useFakeTimers()
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&page=2') as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.setSearchInput('SO-9')
        })
        vi.advanceTimersByTime(299)
        expect(router.replace).not.toHaveBeenCalled()

        vi.advanceTimersByTime(1)
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&q=SO-9',
        )
    })

    it('does not write the URL when the debounced input equals the URL value', () => {
        vi.useFakeTimers()
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&q=SO-9') as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.setSearchInput('SO-9')
        })
        vi.advanceTimersByTime(300)
        expect(router.replace).not.toHaveBeenCalled()
        expect(result.current.searchInput).toBe('SO-9')
    })

    it('trims the debounced q and writes null for whitespace-only input', () => {
        vi.useFakeTimers()
        mockedSearchParams.mockReturnValue(new URLSearchParams('view=receipt') as unknown as ReadonlyURLSearchParams)
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.setSearchInput('   ')
        })
        vi.advanceTimersByTime(300)
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receipt',
        )
    })

    it('syncs the search input when the URL q param changes', () => {
        const { result, rerender } = renderHook(() =>
            useCustomerReceivablesUrlState(),
        )
        expect(result.current.searchInput).toBe('')

        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&q=EXT') as unknown as ReadonlyURLSearchParams,
        )
        rerender()
        expect(result.current.searchInput).toBe('EXT')
    })

    it('focuses the search input on "/" unless modifiers or editable targets are active', () => {
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        const input = document.createElement('input')
        document.body.appendChild(input)
        result.current.searchInputRef.current = input
        const focusSpy = vi.spyOn(input, 'focus')

        window.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', cancelable: true }),
        )
        expect(focusSpy).toHaveBeenCalledTimes(1)

        focusSpy.mockClear()
        window.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', metaKey: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        focusSpy.mockClear()
        const other = document.createElement('input')
        document.body.appendChild(other)
        other.dispatchEvent(
            new KeyboardEvent('keydown', { key: '/', bubbles: true }),
        )
        expect(focusSpy).not.toHaveBeenCalled()

        document.body.removeChild(input)
        document.body.removeChild(other)
    })
})
