import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'

import { useContractsList } from '@/features/contracts/hooks/use-contracts-list'
import type { ContractListRow } from '@/features/contracts/types'

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock('next/navigation', () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => '/sales/contracts',
}))

let rowSeq = 0
function makeRow(
    overrides: Partial<ContractListRow> = {},
): ContractListRow {
    rowSeq += 1
    const id = String(rowSeq)
    return {
        contractId: `ct-${id}`,
        contractNo: `CT-${id}`,
        customer: { customerId: `c-${id}`, customerNo: `C-${id}`, displayName: `客户${id}` },
        settlementParty: { partyId: `p-${id}`, displayName: `主体${id}` },
        status: 'EFFECTIVE',
        statusLabel: '生效',
        statusTone: 'success',
        revisionNo: 1,
        validFrom: '2026-01-01',
        validTo: '9999-12-31',
        expiringWithin30Days: false,
        salesOrderCount: 0,
        activeSalesOrderCount: 0,
        ownerLabel: `负责人${id}`,
        ownerKind: 'current_customer_owner',
        allowedActions: ['PRINT'],
        actionBlockers: [],
        ...overrides,
    }
}

describe('useContractsList', () => {
    beforeEach(() => {
        rowSeq = 0
        navMocks.searchParams = new URLSearchParams()
        navMocks.replace.mockClear()
        navMocks.push.mockClear()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('parses empty URL to defaults and passes through all rows', () => {
        const rows = [makeRow(), makeRow({ status: 'EXPIRED', statusLabel: '到期', statusTone: 'warning' })]
        const { result } = renderHook(() => useContractsList(rows))

        expect(result.current.q).toBeUndefined()
        expect(result.current.metric).toBe('all')
        expect(result.current.page).toBe(1)
        expect(result.current.pageSize).toBe(20)
        expect(result.current.sort).toBeUndefined()
        expect(result.current.dir).toBeUndefined()
        expect(result.current.customerId).toBeUndefined()
        expect(result.current.isFiltered).toBe(false)
        expect(result.current.filtered).toHaveLength(2)
        expect(result.current.pageRows).toHaveLength(2)
        expect(result.current.metrics).toEqual({
            all: 2,
            effective: 1,
            expiring_30d: 0,
            expired: 1,
            terminated: 0,
        })
    })

    it('default-sorts expiring rows first, then by validTo ascending', () => {
        const rows = [
            makeRow({ contractId: 'ct-late', validTo: '2030-01-01' }),
            makeRow({ contractId: 'ct-expiring', expiringWithin30Days: true, validTo: '2035-01-01' }),
            makeRow({ contractId: 'ct-early', validTo: '2028-01-01' }),
        ]
        const { result } = renderHook(() => useContractsList(rows))

        expect(result.current.sorted.map((r) => r.contractId)).toEqual([
            'ct-expiring',
            'ct-early',
            'ct-late',
        ])
    })

    it('parses q/metric/sort/page/customerId from URL and filters', () => {
        navMocks.searchParams = new URLSearchParams(
            'q=客户1&metric=effective&page=2&pageSize=1&sort=contractNo&dir=asc&customerId=c-1',
        )
        const rows = [
            makeRow({ contractId: 'ct-1', customer: { customerId: 'c-1', customerNo: 'C-1', displayName: '客户1' } }),
            makeRow({ contractId: 'ct-2', customer: { customerId: 'c-2', customerNo: 'C-2', displayName: '客户2' } }),
        ]
        const { result } = renderHook(() => useContractsList(rows))

        expect(result.current.q).toBe('客户1')
        expect(result.current.metric).toBe('effective')
        expect(result.current.page).toBe(2)
        expect(result.current.pageSize).toBe(1)
        expect(result.current.sort).toBe('contractNo')
        expect(result.current.dir).toBe('asc')
        expect(result.current.isFiltered).toBe(true)
        expect(result.current.filtered.map((r) => r.contractId)).toEqual([
            'ct-1',
        ])
        expect(result.current.lockedCustomer?.displayName).toBe('客户1')
        expect(result.current.filterSnapshotLabel).toBe(
            '指标=有效 · 搜索=客户1 · 客户=客户1',
        )
    })

    it('exposes pagination and sorts by the requested column', () => {
        navMocks.searchParams = new URLSearchParams('sort=sales&dir=desc&page=1&pageSize=2')
        const rows = [
            makeRow({ salesOrderCount: 1 }),
            makeRow({ salesOrderCount: 9 }),
            makeRow({ salesOrderCount: 5 }),
        ]
        const { result } = renderHook(() => useContractsList(rows))

        expect(result.current.sorted.map((r) => r.salesOrderCount)).toEqual([
            9, 5, 1,
        ])
        expect(result.current.pageRows.map((r) => r.salesOrderCount)).toEqual([
            9, 5,
        ])
        expect(result.current.pagination).toEqual({ pageIndex: 0, pageSize: 2 })
    })

    it('debounces search draft and pushes a trimmed q with page reset', () => {
        vi.useFakeTimers()
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.setSearchDraft('  甲  ')
        })

        expect(navMocks.replace).not.toHaveBeenCalled()

        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(navMocks.replace).toHaveBeenCalledTimes(1)
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/contracts?q=%E7%94%B2',
            { scroll: false },
        )
    })

    it('skips the URL push when the debounced draft equals the current q', () => {
        vi.useFakeTimers()
        navMocks.searchParams = new URLSearchParams('q=甲')
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.setSearchDraft('甲')
        })
        act(() => {
            vi.advanceTimersByTime(300)
        })

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('commits search immediately via handleSearchCommit', () => {
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.handleSearchCommit('  CT-1 ')
        })

        expect(result.current.searchDraft).toBe('  CT-1 ')
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/contracts?q=CT-1',
            { scroll: false },
        )
    })

    it('updates the URL for metric, sorting, pagination and customer-lock changes', () => {
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.handleMetricChange('terminated')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts?metric=terminated',
            { scroll: false },
        )

        act(() => {
            result.current.handleSortingChange([{ id: 'contractNo', desc: true }])
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts?sort=contractNo&dir=desc',
            { scroll: false },
        )

        act(() => {
            result.current.handleSortingChange([])
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts',
            { scroll: false },
        )

        act(() => {
            result.current.handlePaginationChange({ pageIndex: 3, pageSize: 50 })
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts?page=4&pageSize=50',
            { scroll: false },
        )

        act(() => {
            result.current.handleClearCustomerLock()
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts',
            { scroll: false },
        )
    })

    it('clears q, metric, customer lock and page but keeps sort on clearAllFilters', () => {
        navMocks.searchParams = new URLSearchParams(
            'q=甲&metric=expired&customerId=c-9&page=3&sort=contractNo&dir=asc',
        )
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.clearAllFilters()
        })

        expect(result.current.searchDraft).toBe('')
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/contracts?sort=contractNo&dir=asc',
            { scroll: false },
        )
    })

    it('focuses the search input on "/" outside of inputs', () => {
        const input = document.createElement('input')
        input.setAttribute('data-slot', 'contracts-search')
        document.body.appendChild(input)
        const focusSpy = vi.spyOn(input, 'focus')

        const { unmount } = renderHook(() => useContractsList([]))

        const event = new KeyboardEvent('keydown', {
            key: '/',
            bubbles: true,
            cancelable: true,
        })
        window.dispatchEvent(event)
        expect(event.defaultPrevented).toBe(true)
        expect(focusSpy).toHaveBeenCalled()

        focusSpy.mockClear()
        const inInput = new KeyboardEvent('keydown', {
            key: '/',
            bubbles: true,
            cancelable: true,
        })
        input.dispatchEvent(inInput)
        expect(inInput.defaultPrevented).toBe(false)
        expect(focusSpy).not.toHaveBeenCalled()

        unmount()
        input.remove()
    })
})
