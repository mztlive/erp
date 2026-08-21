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
        expect(result.current.searchDraft).toBe('')
        expect(result.current.hasActiveFilters).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.panelOpen).toBe(false)
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
        expect(result.current.searchDraft).toBe('SO-1')
        expect(result.current.counterpartyPartyIdDraft).toBe('p1')
        expect(result.current.dueDraft).toBe('overdue')
        expect(result.current.statusDraft).toBe('open')
        expect(result.current.reviewStatusDraft).toBe('reviewed')
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
            new URLSearchParams(
                'view=bogus&due=soon&status=bogus&reviewStatus=weird',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.view).toBe('receivable')
        expect(result.current.due).toBeUndefined()
        expect(result.current.status).toBeUndefined()
        expect(result.current.reviewStatus).toBeUndefined()
        expect(result.current.dueDraft).toBe('all')
        expect(result.current.statusDraft).toBe('all')
        expect(result.current.reviewStatusDraft).toBe('all')
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

    it('derives hasActiveFilters from filter params, ignoring blank q and all', () => {
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
        filtered.unmount()

        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&due=all') as unknown as ReadonlyURLSearchParams,
        )
        const dueAll = renderHook(() => useCustomerReceivablesUrlState())
        expect(dueAll.result.current.hasActiveFilters).toBe(false)
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
            { scroll: false },
        )

        act(() => {
            result.current.patchUrl({ q: null }, { replace: false })
        })
        expect(router.push).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&due=overdue',
        )
    })

    it('typing the search draft does not write the URL', () => {
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.setSearchDraft('SO-9')
        })
        expect(result.current.searchDraft).toBe('SO-9')
        expect(router.replace).not.toHaveBeenCalled()
        expect(router.push).not.toHaveBeenCalled()
    })

    it('applyFilters patches every structured filter at once, resets page and closes the panel', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&page=3') as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.setSearchDraft('SO-9')
            result.current.setCounterpartyPartyIdDraft('p1')
            result.current.setDueDraft('overdue')
            result.current.setStatusDraft('open')
            result.current.setReviewStatusDraft('pending_opening')
            result.current.setPanelOpen(true)
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(router.replace).toHaveBeenCalledTimes(1)
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&q=SO-9&counterpartyId=p1&due=overdue&status=open&reviewStatus=pending_opening',
            { scroll: false },
        )
        expect(result.current.panelOpen).toBe(false)
    })

    it('applyFilters omits defaults from the URL', () => {
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())

        act(() => {
            result.current.setSearchDraft('   ')
            result.current.applyFilters()
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable',
            { scroll: false },
        )
    })

    it('removeFilter removes a single applied condition and its draft', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receivable&q=abc&due=overdue&counterpartyId=p1&salesOrderId=s1',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result, rerender } = renderHook(() =>
            useCustomerReceivablesUrlState(),
        )

        act(() => {
            result.current.removeFilter('q')
        })
        expect(result.current.searchDraft).toBe('')
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&due=overdue&counterpartyId=p1&salesOrderId=s1',
            { scroll: false },
        )

        // router.replace 后 useSearchParams 已同步（同 openPreview/closePreview 的模拟方式）
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receivable&due=overdue&counterpartyId=p1&salesOrderId=s1',
            ) as unknown as ReadonlyURLSearchParams,
        )
        rerender()
        act(() => {
            result.current.removeFilter('due')
        })
        expect(result.current.dueDraft).toBe('all')
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&counterpartyId=p1&salesOrderId=s1',
            { scroll: false },
        )

        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receivable&counterpartyId=p1&salesOrderId=s1',
            ) as unknown as ReadonlyURLSearchParams,
        )
        rerender()
        act(() => {
            result.current.removeFilter('salesOrderId')
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&counterpartyId=p1',
            { scroll: false },
        )
    })

    it('resetMoreFilters clears structured conditions but keeps q and customerId and keeps the panel open', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receivable&q=SO-1&customerId=c1&counterpartyId=p1&due=overdue&status=open&reviewStatus=reviewed',
            ) as unknown as ReadonlyURLSearchParams,
        )
        const router = setupRouter()
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.panelOpen).toBe(true)

        act(() => {
            result.current.resetMoreFilters()
        })

        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&q=SO-1&customerId=c1',
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe('SO-1')
        expect(result.current.counterpartyPartyIdDraft).toBeNull()
        expect(result.current.dueDraft).toBe('all')
        expect(result.current.statusDraft).toBe('all')
        expect(result.current.reviewStatusDraft).toBe('all')
        expect(result.current.panelOpen).toBe(true)
    })

    it('clearFilters resets every filter param, drafts, panel and page but keeps the view', () => {
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
        expect(result.current.searchDraft).toBe('')
        expect(result.current.counterpartyPartyIdDraft).toBeNull()
        expect(result.current.dueDraft).toBe('all')
        expect(result.current.statusDraft).toBe('all')
        expect(result.current.reviewStatusDraft).toBe('all')
        expect(result.current.panelOpen).toBe(false)
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receipt',
            { scroll: false },
        )
    })

    it('opens the panel initially for deep links with structured filters', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&due=overdue') as unknown as ReadonlyURLSearchParams,
        )
        const first = renderHook(() => useCustomerReceivablesUrlState())
        expect(first.result.current.panelOpen).toBe(true)
        first.unmount()

        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&q=SO-1') as unknown as ReadonlyURLSearchParams,
        )
        const second = renderHook(() => useCustomerReceivablesUrlState())
        expect(second.result.current.panelOpen).toBe(false)
    })

    it('URL backfill syncs drafts without forcing the panel back open', () => {
        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&due=overdue') as unknown as ReadonlyURLSearchParams,
        )
        const { result, rerender } = renderHook(() =>
            useCustomerReceivablesUrlState(),
        )
        expect(result.current.panelOpen).toBe(true)

        act(() => {
            result.current.applyFilters()
        })
        expect(result.current.panelOpen).toBe(false)

        mockedSearchParams.mockReturnValue(
            new URLSearchParams(
                'view=receivable&due=overdue&status=open',
            ) as unknown as ReadonlyURLSearchParams,
        )
        rerender()
        expect(result.current.dueDraft).toBe('overdue')
        expect(result.current.statusDraft).toBe('open')
        expect(result.current.panelOpen).toBe(false)
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
            { scroll: false },
        )

        act(() => {
            result.current.handlePaginationChange({ pageIndex: 4, pageSize: 20 })
        })
        expect(router.replace).toHaveBeenCalledWith(
            '/finance/customer-accounts?view=receivable&page=5',
            { scroll: false },
        )
    })

    it('syncs the search draft when the URL q param changes', () => {
        const { result, rerender } = renderHook(() =>
            useCustomerReceivablesUrlState(),
        )
        expect(result.current.searchDraft).toBe('')

        mockedSearchParams.mockReturnValue(
            new URLSearchParams('view=receivable&q=EXT') as unknown as ReadonlyURLSearchParams,
        )
        rerender()
        expect(result.current.searchDraft).toBe('EXT')
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
