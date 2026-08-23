import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { ColumnDef } from '@tanstack/react-table'

import { ContractsTablePanel } from '@/features/contracts/components/contracts-table-panel'
import { useContractsList } from '@/features/contracts/hooks/use-contracts-list'
import type { ContractListRow } from '@/features/contracts/types'

afterEach(cleanup)

type FakeList = ReturnType<typeof useContractsList>

function makeFakeList(
    overrides: Partial<FakeList> = {},
): FakeList {
    const base: FakeList = {
        url: {
            q: undefined,
            metric: 'all',
            page: 1,
            pageSize: 20,
            sort: undefined,
            dir: undefined,
            customerId: undefined,
            settlementPartyId: undefined,
            owner: undefined,
            upload: undefined,
        },
        q: undefined,
        metric: 'all',
        page: 1,
        pageSize: 20,
        sort: undefined,
        dir: undefined,
        customerId: undefined,
        settlementPartyId: undefined,
        owner: undefined,
        upload: undefined,
        hasStructuredFilters: false,
        searchDraft: '',
        setSearchDraft: vi.fn(),
        searchInputRef: { current: null },
        settlementPartyIdDraft: null,
        setSettlementPartyIdDraft: vi.fn(),
        ownerDraft: null,
        setOwnerDraft: vi.fn(),
        panelOpen: false,
        setPanelOpen: vi.fn(),
        filtered: [],
        sorting: [],
        sorted: [],
        pagination: { pageIndex: 0, pageSize: 20 },
        pageRows: [],
        metrics: {
            all: 0,
            effective: 0,
            expiring_30d: 0,
            expired: 0,
            terminated: 0,
        },
        appliedChips: [],
        filterDescription: '按将到期优先排序展示当前业务范围内的合同。',
        filterSnapshotLabel: '',
        settlementPartyOptions: [],
        ownerOptions: [],
        isFiltered: false,
        applyFilters: vi.fn(),
        resetMoreFilters: vi.fn(),
        removeFilter: vi.fn(),
        handleMetricChange: vi.fn(),
        handleSortingChange: vi.fn(),
        handlePaginationChange: vi.fn(),
        clearAllFilters: vi.fn(),
    }
    return { ...base, ...overrides } as FakeList
}

function renderPanel(list: FakeList) {
    return render(
        <ContractsTablePanel
            list={list}
            columns={[] as ColumnDef<ContractListRow>[]}
            isError={false}
            error={null}
            isPending={false}
            onRetry={vi.fn()}
            onOpenUpload={vi.fn()}
            onPreview={vi.fn()}
        />,
    )
}

describe('ContractsTablePanel 筛选区验收（docs/ui-filter-design.md §14）', () => {
    it('renders exactly one semantic form inside the toolbar', () => {
        const { container } = renderPanel(makeFakeList())

        expect(container.querySelectorAll('form')).toHaveLength(1)
        expect(screen.getByRole('toolbar')).toBeTruthy()
    })

    it('collapsed state has no submit button and no panel submit', () => {
        renderPanel(makeFakeList())

        expect(
            screen.queryByRole('button', { name: '应用搜索与筛选' }),
        ).toBeNull()
        expect(screen.queryByRole('button', { name: '应用全部筛选' })).toBeNull()
        expect(screen.queryByRole('button', { name: '重置更多条件' })).toBeNull()
    })

    it('expanded state hides the arrow and shows the single main submit', () => {
        const list = makeFakeList({ panelOpen: true })
        renderPanel(list)

        expect(screen.queryByLabelText('应用搜索与筛选')).toBeNull()

        const submit = screen.getByRole('button', { name: '应用全部筛选' })
        expect((submit as HTMLButtonElement).type).toBe('submit')
        const reset = screen.getByRole('button', { name: '重置更多条件' })
        expect((reset as HTMLButtonElement).type).toBe('button')
        expect(
            screen.getByText('将同时应用上方关键词和以下筛选条件；结果也用于导出。'),
        ).toBeTruthy()
        expect(screen.getByLabelText('合同更多筛选条件')).toBeTruthy()
    })

    it('more-filters toggle exposes aria-expanded and aria-controls pointing to the panel id', () => {
        const setPanelOpen = vi.fn()
        const list = makeFakeList({ panelOpen: true, setPanelOpen })
        const { container } = renderPanel(list)

        const toggle = screen.getByRole('button', { name: /更多筛选/ })
        expect(toggle.getAttribute('aria-expanded')).toBe('true')
        const panelId = toggle.getAttribute('aria-controls')
        expect(panelId).toBeTruthy()
        expect(container.ownerDocument.getElementById(panelId!)).toBeTruthy()

        fireEvent.click(toggle)
        expect(setPanelOpen).toHaveBeenCalled()
    })

    it('marks the more-filters button with 已启用 when structured filters are applied', () => {
        renderPanel(makeFakeList({ hasStructuredFilters: true }))
        expect(screen.getByText('已启用')).toBeTruthy()
    })

    it('shows every applied condition as a chip with 已筛选 and 清空全部', () => {
        const clearAllFilters = vi.fn()
        const list = makeFakeList({
            isFiltered: true,
            appliedChips: [
                { key: 'q', label: '搜索：CT-1' },
                { key: 'metric', label: '指标：有效' },
                { key: 'customerId', label: '客户：东方企业' },
            ],
            clearAllFilters,
        })
        renderPanel(list)

        expect(screen.getByText('已筛选')).toBeTruthy()
        expect(screen.getByText('搜索：CT-1')).toBeTruthy()
        expect(screen.getByText('指标：有效')).toBeTruthy()
        expect(screen.getByText('客户：东方企业')).toBeTruthy()

        fireEvent.click(screen.getByRole('button', { name: '清空全部' }))
        expect(clearAllFilters).toHaveBeenCalledTimes(1)
    })

    it('removes a single chip through removeFilter', () => {
        const removeFilter = vi.fn()
        const list = makeFakeList({
            appliedChips: [{ key: 'owner', label: '负责人：张三' }],
            removeFilter,
        })
        renderPanel(list)

        fireEvent.click(screen.getByLabelText('移除负责人：张三'))
        expect(removeFilter).toHaveBeenCalledWith('owner')
    })

    it('submits the form through applyFilters', () => {
        const applyFilters = vi.fn()
        const { container } = renderPanel(makeFakeList({ applyFilters }))

        const form = container.querySelector('form')
        expect(form).toBeTruthy()
        fireEvent.submit(form!)
        expect(applyFilters).toHaveBeenCalledTimes(1)
    })

    it('keeps the toolbar mounted and clears filters from the filter empty state', () => {
        const clearAllFilters = vi.fn()
        const { container } = renderPanel(
            makeFakeList({ isFiltered: true, clearAllFilters }),
        )

        expect(screen.getByText('换一个关键词或清除筛选后再试。')).toBeTruthy()
        expect(container.querySelector('form')).toBeTruthy()

        fireEvent.click(screen.getByRole('button', { name: '清除筛选' }))
        expect(clearAllFilters).toHaveBeenCalledTimes(1)
    })

    it('shows the result count in the frame header', () => {
        renderPanel(
            makeFakeList({
                sorted: [
                    {
                        contractId: 'ct-1',
                        contractNo: 'CT-1',
                        customer: {
                            customerId: 'c-1',
                            customerNo: 'C-1',
                            displayName: '客户1',
                        },
                        settlementParty: {
                            partyId: 'p-1',
                            displayName: '主体1',
                        },
                        status: 'EFFECTIVE',
                        statusLabel: '生效',
                        statusTone: 'success',
                        revisionNo: 1,
                        validFrom: '2026-01-01',
                        validTo: '9999-12-31',
                        expiringWithin30Days: false,
                        salesOrderCount: 0,
                        activeSalesOrderCount: 0,
                        ownerLabel: '负责人1',
                        ownerKind: 'current_customer_owner',
                        allowedActions: ['PRINT'],
                        actionBlockers: [],
                    } satisfies ContractListRow,
                ],
                pageRows: [] as ContractListRow[],
            }),
        )

        expect(screen.getByText('1 条')).toBeTruthy()
    })
})
