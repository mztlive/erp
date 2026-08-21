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
        expect(result.current.settlementPartyId).toBeUndefined()
        expect(result.current.owner).toBeUndefined()
        expect(result.current.hasStructuredFilters).toBe(false)
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.isFiltered).toBe(false)
        expect(result.current.appliedChips).toEqual([])
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

    it('parses q/metric/sort/page/customerId/structured filters from URL and filters', () => {
        navMocks.searchParams = new URLSearchParams(
            'q=客户1&metric=effective&page=2&pageSize=1&sort=contractNo&dir=asc&customerId=c-1&settlementPartyId=p-1&owner=负责人1',
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
        expect(result.current.customerId).toBe('c-1')
        expect(result.current.settlementPartyId).toBe('p-1')
        expect(result.current.owner).toBe('负责人1')
        expect(result.current.hasStructuredFilters).toBe(true)
        expect(result.current.panelOpen).toBe(true)
        expect(result.current.isFiltered).toBe(true)
        expect(result.current.filtered.map((r) => r.contractId)).toEqual([
            'ct-1',
        ])
        expect(result.current.appliedChips.map((c) => c.label)).toEqual([
            '搜索：客户1',
            '指标：有效',
            '客户：客户1',
            '结算主体：主体1',
            '负责人：负责人1',
        ])
        expect(result.current.filterSnapshotLabel).toBe(
            '指标=有效 · 搜索=客户1 · 客户=客户1 · 结算主体=主体1 · 负责人=负责人1',
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

    it('does not write the URL while the draft changes', () => {
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.setSearchDraft('  甲  ')
            result.current.setSettlementPartyIdDraft('p-1')
            result.current.setOwnerDraft('张三')
        })

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('applies the keyword and structured drafts in one URL write and collapses the panel', () => {
        navMocks.searchParams = new URLSearchParams('settlementPartyId=p-1')
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        expect(result.current.panelOpen).toBe(true)

        act(() => {
            result.current.setSearchDraft('  CT-1 ')
            result.current.setOwnerDraft('张三')
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(navMocks.replace).toHaveBeenCalledTimes(1)
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/contracts?q=CT-1&settlementPartyId=p-1&owner=%E5%BC%A0%E4%B8%89',
            { scroll: false },
        )
        expect(result.current.panelOpen).toBe(false)
    })

    it('submits trimmed keyword and defaults become URL omissions', () => {
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.setSearchDraft('   ')
        })
        act(() => {
            result.current.applyFilters()
        })

        expect(navMocks.replace).toHaveBeenCalledWith('/sales/contracts', {
            scroll: false,
        })
    })

    it('syncs drafts from URL backfill without reopening a closed panel', () => {
        navMocks.searchParams = new URLSearchParams('settlementPartyId=p-1')
        const rows = [makeRow(), makeRow()]
        const { result, rerender } = renderHook(() => useContractsList(rows))

        expect(result.current.panelOpen).toBe(true)
        act(() => {
            result.current.setPanelOpen(false)
        })

        navMocks.searchParams = new URLSearchParams(
            'settlementPartyId=p-2&owner=李四',
        )
        rerender()

        expect(result.current.settlementPartyIdDraft).toBe('p-2')
        expect(result.current.ownerDraft).toBe('李四')
        expect(result.current.panelOpen).toBe(false)
    })

    it('resets only the more-filters conditions and keeps the panel open', () => {
        navMocks.searchParams = new URLSearchParams(
            'q=甲&metric=effective&settlementPartyId=p-1&owner=张三',
        )
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.resetMoreFilters()
        })

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/contracts?q=%E7%94%B2&metric=effective',
            { scroll: false },
        )
        expect(result.current.settlementPartyIdDraft).toBeNull()
        expect(result.current.ownerDraft).toBeNull()
        expect(result.current.searchDraft).toBe('甲')
        expect(result.current.panelOpen).toBe(true)
    })

    it('removes a single applied condition via removeFilter', () => {
        navMocks.searchParams = new URLSearchParams(
            'q=甲&metric=expired&settlementPartyId=p-1&owner=张三',
        )
        const rows = [makeRow(), makeRow()]
        const { result, rerender } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.removeFilter('metric')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts?q=%E7%94%B2&settlementPartyId=p-1&owner=%E5%BC%A0%E4%B8%89',
            { scroll: false },
        )

        // 模拟 router.replace 已生效：后续 patch 基于更新后的 URL 构建（Next.js 行为）
        navMocks.searchParams = new URLSearchParams(
            'q=甲&settlementPartyId=p-1&owner=张三',
        )
        rerender()

        act(() => {
            result.current.removeFilter('settlementPartyId')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts?q=%E7%94%B2&owner=%E5%BC%A0%E4%B8%89',
            { scroll: false },
        )
        expect(result.current.settlementPartyIdDraft).toBeNull()

        navMocks.searchParams = new URLSearchParams('q=甲&owner=张三')
        rerender()

        act(() => {
            result.current.removeFilter('q')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts?owner=%E5%BC%A0%E4%B8%89',
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe('')
    })

    it('updates the URL for metric, sorting and pagination changes', () => {
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
    })

    it('removes the customer lock chip via removeFilter', () => {
        navMocks.searchParams = new URLSearchParams('customerId=c-9&sort=sales&dir=asc')
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        act(() => {
            result.current.removeFilter('customerId')
        })
        expect(navMocks.replace).toHaveBeenLastCalledWith(
            '/sales/contracts?sort=sales&dir=asc',
            { scroll: false },
        )
    })

    it('clears q, metric, structured filters, customer lock and page but keeps sort on clearAllFilters', () => {
        navMocks.searchParams = new URLSearchParams(
            'q=甲&metric=expired&customerId=c-9&settlementPartyId=p-1&owner=张三&page=3&sort=contractNo&dir=asc',
        )
        const rows = [makeRow(), makeRow()]
        const { result } = renderHook(() => useContractsList(rows))

        expect(result.current.panelOpen).toBe(true)

        act(() => {
            result.current.clearAllFilters()
        })

        expect(result.current.searchDraft).toBe('')
        expect(result.current.settlementPartyIdDraft).toBeNull()
        expect(result.current.ownerDraft).toBeNull()
        expect(result.current.panelOpen).toBe(false)
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/contracts?sort=contractNo&dir=asc',
            { scroll: false },
        )
    })

    it('focuses the search input on "/" outside of inputs and dialogs', () => {
        const { result, unmount } = renderHook(() => useContractsList([]))
        const input = document.createElement('input')
        result.current.searchInputRef.current = input
        document.body.appendChild(input)
        const focusSpy = vi.spyOn(input, 'focus')

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

        focusSpy.mockClear()
        const dialog = document.createElement('div')
        dialog.setAttribute('role', 'dialog')
        document.body.appendChild(dialog)
        const whileDialog = new KeyboardEvent('keydown', {
            key: '/',
            bubbles: true,
            cancelable: true,
        })
        window.dispatchEvent(whileDialog)
        expect(whileDialog.defaultPrevented).toBe(false)
        expect(focusSpy).not.toHaveBeenCalled()

        dialog.remove()
        input.remove()
        unmount()
    })
})
