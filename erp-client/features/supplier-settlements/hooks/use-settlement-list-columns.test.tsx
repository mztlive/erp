import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { SettlementsUrlState } from '@/features/supplier-settlements/lib/url-state'
import type { SettlementListRow } from '@/features/supplier-settlements/types'
import { useSettlementListColumns } from './use-settlement-list-columns'

// RTL 自动清理依赖全局 afterEach；vitest globals 关闭，需手动清理。
afterEach(cleanup)

function makeRow(
    overrides: Partial<SettlementListRow> = {},
): SettlementListRow {
    return {
        statementId: 'st1',
        statementNo: 'ST-2026-001',
        supplierId: 'sup1',
        supplierName: '示例供应商',
        periodStart: '2026-01-01',
        periodEnd: '2026-01-31',
        periodLabel: '2026-01',
        status: 'PENDING_RECONCILE',
        statusLabel: '待对账',
        statusTone: 'info',
        erpAmountGross: '100.00',
        supplierAmountGross: '100.00',
        differenceAmountGross: '0.00',
        differenceDirectionLabel: '无差异',
        unresolvedDifferenceCount: 0,
        preparedByLabel: '张三',
        reviewedByLabel: '待复核人',
        updatedAt: '2026-01-01T00:00:00.000Z',
        allowedActions: [],
        actionBlockers: [],
        ...overrides,
    }
}

function renderCell(
    columnId: string,
    row: SettlementListRow,
    patchUrl: (patch: Partial<SettlementsUrlState>) => void = vi.fn(),
    onOpen: (statementId: string) => void = vi.fn(),
) {
    const { result } = renderHook(() =>
        useSettlementListColumns(patchUrl, onOpen),
    )
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = { row: { original: row } } as CellContext<
        SettlementListRow,
        unknown
    >
    const cell = column!.cell as
        | ((props: CellContext<SettlementListRow, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

describe('useSettlementListColumns', () => {
    it('returns the expected column ids and headers in order', () => {
        const { result } = renderHook(() =>
            useSettlementListColumns(vi.fn(), vi.fn()),
        )
        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'statementNo',
            'supplier',
            'period',
            'erpAmount',
            'supplierAmount',
            'difference',
            'status',
            'actors',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '结算单号',
            '供应商',
            '期间',
            'ERP 金额',
            '账单金额',
            '差异',
            '状态',
            '经办/复核',
            '操作',
        ])
    })

    it('renders the statement number cell', () => {
        const cell = renderCell('statementNo', makeRow())
        expect(cell.getByText('ST-2026-001')).toBeDefined()
    })

    it('shows a placeholder when the supplier bill is not synced', () => {
        const cell = renderCell(
            'supplierAmount',
            makeRow({ supplierAmountGross: undefined }),
        )
        expect(cell.getByText('账单未同步')).toBeDefined()
    })

    it('renders the difference direction and unresolved count badge', () => {
        const cell = renderCell(
            'difference',
            makeRow({
                differenceAmountGross: '-12.00',
                differenceDirectionLabel: 'ERP 高于供应商账单',
                unresolvedDifferenceCount: 2,
            }),
        )
        expect(cell.getByText('ERP 高于供应商账单')).toBeDefined()
        expect(cell.getByText('未决 2')).toBeDefined()
    })

    it('opens preview via patchUrl and detail via onOpen', () => {
        const patchUrl = vi.fn()
        const onOpen = vi.fn()
        const cell = renderCell('actions', makeRow(), patchUrl, onOpen)

        fireEvent.click(cell.getByText('预览'))
        expect(patchUrl).toHaveBeenCalledWith({ preview: 'st1' })

        fireEvent.click(cell.getByText('打开'))
        expect(onOpen).toHaveBeenCalledWith('st1')
    })

    it('recomputes columns only when patchUrl or onOpen change', () => {
        const patchUrl = vi.fn()
        const onOpen = vi.fn()
        const { result, rerender } = renderHook(
            ({
                patchUrl: patch,
                onOpen: open,
            }: {
                patchUrl: (patch: Partial<SettlementsUrlState>) => void
                onOpen: (statementId: string) => void
            }) => useSettlementListColumns(patch, open),
            {
                initialProps: { patchUrl, onOpen },
            },
        )

        const first = result.current
        rerender({ patchUrl, onOpen })
        expect(result.current).toBe(first)

        const nextPatchUrl = vi.fn()
        rerender({ patchUrl: nextPatchUrl, onOpen })
        expect(result.current).not.toBe(first)

        const second = result.current
        const nextOnOpen = vi.fn()
        rerender({ patchUrl: nextPatchUrl, onOpen: nextOnOpen })
        expect(result.current).not.toBe(second)
    })
})
