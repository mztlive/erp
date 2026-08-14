import { describe, it, expect, vi, afterEach } from 'vitest'
import { cleanup, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'

import { useContractListColumns } from '@/features/contracts/hooks/use-contract-list-columns'
import type { ContractListRow } from '@/features/contracts/types'

afterEach(cleanup)

vi.mock('next/link', async () => {
    const React = await import('react')
    return {
        default: ({
            href,
            children,
        }: {
            href: string
            children?: React.ReactNode
        }) => React.createElement('a', { href }, children),
    }
})

const baseRow: ContractListRow = {
    contractId: 'ct-1',
    contractNo: 'CT-2026-001',
    customer: {
        customerId: 'c1',
        customerNo: 'C-001',
        displayName: '客户甲',
    },
    settlementParty: { partyId: 'p1', displayName: '主体乙' },
    status: 'EFFECTIVE',
    statusLabel: '生效',
    statusTone: 'success',
    revisionNo: 2,
    signedAt: '2026-01-01',
    validFrom: '2026-01-01',
    validTo: '2027-01-01',
    expiringWithin30Days: false,
    salesOrderCount: 3,
    activeSalesOrderCount: 1,
    ownerLabel: '张三',
    ownerKind: 'current_customer_owner',
    allowedActions: ['PRINT'],
    actionBlockers: [],
}

function cellContext(
    row: ContractListRow,
): CellContext<ContractListRow, unknown> {
    return { row: { original: row } } as CellContext<ContractListRow, unknown>
}

describe('useContractListColumns', () => {
    it('builds the contract columns with expected ids, headers and meta', () => {
        const onPreview = vi.fn()
        const onPaper = vi.fn()
        const { result } = renderHook(() =>
            useContractListColumns({ onPreview, onPaper }),
        )

        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'contractNo',
            'settlement',
            'validity',
            'status',
            'revision',
            'sales',
            'owner',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '合同编号',
            '结算主体',
            '有效期',
            '状态',
            '版本',
            '销售单',
            '负责人',
            '操作',
        ])
        expect(columns[0].meta).toEqual({
            label: '合同编号',
            width: 'reference',
        })
        expect(columns.find((c) => c.id === 'status')?.enableSorting).toBe(
            false,
        )
        expect(columns.find((c) => c.id === 'actions')?.enableSorting).toBe(
            false,
        )
    })

    it('keeps the same column array for stable callbacks (memoized)', () => {
        const onPreview = vi.fn()
        const onPaper = vi.fn()
        const { result, rerender } = renderHook(
            ({ onPreview, onPaper }) =>
                useContractListColumns({ onPreview, onPaper }),
            { initialProps: { onPreview, onPaper } },
        )

        const first = result.current
        rerender({ onPreview, onPaper })
        expect(result.current).toBe(first)

        rerender({ onPreview: vi.fn(), onPaper })
        expect(result.current).not.toBe(first)
    })

    it('wires the actions cell to onPreview and onPaper', () => {
        const onPreview = vi.fn()
        const onPaper = vi.fn()
        const { result } = renderHook(() =>
            useContractListColumns({ onPreview, onPaper }),
        )

        const actions = result.current.find((c) => c.id === 'actions')
        const cell = actions?.cell
        if (typeof cell !== 'function') throw new Error('actions cell missing')
        const element = cell(cellContext(baseRow)) as React.ReactElement
        const screen = render(element)

        screen.getByText('预览').click()
        expect(onPreview).toHaveBeenCalledWith('ct-1')

        screen.getByText('打印').click()
        expect(onPaper).toHaveBeenCalledWith('ct-1')
    })

    it('disables 打印 and does not call onPaper when PRINT is not allowed', () => {
        const onPreview = vi.fn()
        const onPaper = vi.fn()
        const { result } = renderHook(() =>
            useContractListColumns({ onPreview, onPaper }),
        )

        const actions = result.current.find((c) => c.id === 'actions')
        const cell = actions?.cell
        if (typeof cell !== 'function') throw new Error('actions cell missing')
        const blockedRow: ContractListRow = {
            ...baseRow,
            allowedActions: [],
            actionBlockers: [
                {
                    action: 'PRINT',
                    code: 'CONTRACT_EXPIRED',
                    message: '合同已到期',
                },
            ],
        }
        const element = cell(cellContext(blockedRow)) as React.ReactElement
        const screen = render(element)

        const printButton = screen.getByText('打印')
        expect(printButton.closest('button')?.disabled).toBe(true)
        expect(printButton.closest('button')?.title).toBe('合同已到期')

        printButton.click()
        expect(onPaper).not.toHaveBeenCalled()
    })
})
