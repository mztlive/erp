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
        const { result } = renderHook(() => useContractListColumns())

        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'contractNo',
            'settlement',
            'validity',
            'status',
            'revision',
            'sales',
            'owner',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '合同编号',
            '结算主体',
            '有效期',
            '状态',
            '版本',
            '销售单',
            '负责人',
        ])
        expect(columns[0].meta).toEqual({
            label: '合同编号',
            width: 'reference',
        })
        expect(columns.find((c) => c.id === 'status')?.enableSorting).toBe(
            false,
        )
        expect(columns.find((c) => c.id === 'actions')).toBeUndefined()
    })

    it('keeps a stable column array across rerenders', () => {
        const { result, rerender } = renderHook(() => useContractListColumns())

        const first = result.current
        rerender()
        expect(result.current).toBe(first)
    })

    it('renders owner name and omits the actions column', () => {
        const { result } = renderHook(() => useContractListColumns())
        const owner = result.current.find((c) => c.id === 'owner')
        const cell = owner?.cell
        if (typeof cell !== 'function') {
            throw new Error('owner cell missing')
        }
        const element = cell(cellContext(baseRow)) as React.ReactElement
        const screen = render(element)

        expect(screen.getByText('张三')).toBeTruthy()
        expect(result.current.some((c) => c.id === 'actions')).toBe(false)
    })
})


