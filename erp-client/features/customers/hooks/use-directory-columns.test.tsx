import { describe, it, expect, vi, afterEach } from 'vitest'
import { cleanup, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'

import { useCustomerDirectoryColumns } from '@/features/customers/hooks/use-directory-columns'
import type { CustomerDirectoryItem } from '@/features/customers/types'

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

const baseRow: CustomerDirectoryItem = {
    id: 'c1',
    partyId: 'p1',
    customerNo: 'C-2026-001',
    legalName: '客户甲有限公司',
    shortName: '客户甲',
    status: 'active',
    statusLabel: { label: '启用', tone: 'success' },
    ownerName: '张三',
    collaboratorCount: 2,
    scopeTags: ['mine'],
    metrics: {
        activeContractCount: null,
        inProgressSalesOrderCount: null,
        receivableBalance: null,
        overdueAmount: null,
    },
    attentionTags: ['重点跟进'],
    updatedAt: '2026-08-14T08:30:00.000Z',
}

function cellContext(
    row: CustomerDirectoryItem,
): CellContext<CustomerDirectoryItem, unknown> {
    return { row: { original: row } } as CellContext<CustomerDirectoryItem, unknown>
}

function renderCell(columnId: string, row: CustomerDirectoryItem) {
    const { result } = renderHook(() => useCustomerDirectoryColumns())
    const column = result.current.find((c) => c.id === columnId)
    if (typeof column?.cell !== 'function') {
        throw new Error(`cell renderer missing for ${columnId}`)
    }
    const element = column.cell(cellContext(row)) as React.ReactElement
    return render(element)
}

describe('useCustomerDirectoryColumns', () => {
    it('builds the directory columns with expected ids, headers and meta', () => {
        const { result } = renderHook(() => useCustomerDirectoryColumns())

        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'customer',
            'owner',
            'status',
            'business',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '客户',
            '负责销售',
            '状态',
            '资料更新',
        ])
        expect(columns.map((c) => c.meta?.label)).toEqual([
            '客户',
            '负责销售',
            '状态',
            '资料更新',
        ])
        expect(columns.every((c) => c.enableSorting !== true)).toBe(true)
        expect(columns[0].enableSorting).toBe(false)
        expect(columns[1].enableSorting).toBe(false)
        expect(columns[2].enableSorting).toBe(false)
    })

    it('keeps the same column array across renders (memoized)', () => {
        const { result, rerender } = renderHook(() =>
            useCustomerDirectoryColumns(),
        )
        const first = result.current
        rerender()
        expect(result.current).toBe(first)
    })

    it('renders the customer cell with detail link, number and attention tags', () => {
        const screen = renderCell('customer', baseRow)

        const link = screen.getByText('客户甲')
        expect(link.closest('a')?.getAttribute('href')).toBe(
            '/sales/customers/c1',
        )
        expect(screen.getByText('C-2026-001')).toBeTruthy()
        expect(screen.getByText('重点跟进')).toBeTruthy()
    })

    it('falls back to legalName in the customer cell when shortName is absent', () => {
        const row: CustomerDirectoryItem = {
            ...baseRow,
            shortName: undefined,
            attentionTags: undefined,
        }
        const screen = renderCell('customer', row)

        expect(screen.getByText('客户甲有限公司')).toBeTruthy()
    })

    it('renders the owner cell with collaborator count only when present', () => {
        const screen = renderCell('owner', baseRow)
        expect(screen.getByText('张三')).toBeTruthy()
        expect(screen.getByText('协作 2 人')).toBeTruthy()

        cleanup()
        const solo = renderCell('owner', {
            ...baseRow,
            collaboratorCount: 0,
        })
        expect(solo.getByText('张三')).toBeTruthy()
        expect(solo.queryByText('协作 0 人')).toBeNull()
    })

    it('renders the status cell through the business status badge', () => {
        const screen = renderCell('status', {
            ...baseRow,
            status: 'disabled',
            statusLabel: { label: '停用', tone: 'neutral' },
        })
        expect(screen.getByText('停用')).toBeTruthy()
    })

    it('renders the business cell with the date part of updatedAt', () => {
        const screen = renderCell('business', baseRow)
        expect(screen.getByText('2026-08-14')).toBeTruthy()
    })
})
