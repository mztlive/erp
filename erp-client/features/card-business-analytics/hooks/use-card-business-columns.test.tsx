import { render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import { createElement, type ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import type { CardBusinessRow } from '../types'
import { makeStubRow, makeStubView } from './test-data'
import { useCardBusinessColumns } from './use-card-business-columns'

vi.mock('next/link', () => ({
    default: ({ href, children }: { href: string; children?: ReactNode }) =>
        createElement('a', { href }, children),
}))

function renderCell(
    columnId: string,
    row: CardBusinessRow,
    data = makeStubView(),
) {
    const { result } = renderHook(() => useCardBusinessColumns(data))
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = { row: { original: row } } as CellContext<
        CardBusinessRow,
        unknown
    >
    const cell = column!.cell
    return render(
        (typeof cell === "function" ? cell(ctx) : cell ?? null) as ReactNode,
    ).container
}

function accessorOf(
    columns: ReturnType<typeof useCardBusinessColumns>,
    columnId: string,
) {
    const column = columns.find((c) => c.id === columnId)
    if (!column || !("accessorFn" in column) || !column.accessorFn) {
        throw new Error(`column ${columnId} has no accessorFn`)
    }
    return column.accessorFn
}

describe('useCardBusinessColumns', () => {
    it('returns the expected column ids, headers and accessors in order', () => {
        const data = makeStubView()
        const { result } = renderHook(() => useCardBusinessColumns(data))
        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'customer',
            'salesOrder',
            'category',
            'cardRef',
            'consumption',
            'refund',
            'costBasis',
            'cost',
            'coverage',
            'balance',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            '客户',
            '销售单',
            '卡券类目',
            '卡实例引用',
            '消费(含税)',
            '退款(含税)',
            '成本口径',
            '成本(不含税)',
            '覆盖',
            '未履约余额(含税)',
            '下钻',
        ])
        const row = makeStubRow()
        expect(accessorOf(columns, "customer")(row, 0)).toBe("示例客户")
        expect(accessorOf(columns, "salesOrder")(row, 0)).toBe("SO-2026-001")
        expect(accessorOf(columns, "cardRef")(row, 0)).toBe("ref-abc")
        expect(accessorOf(columns, "costBasis")(row, 0)).toBe("ACTUAL")
    })

    it('memoizes the column array per data reference', () => {
        const data = makeStubView()
        const { result, rerender } = renderHook(
            ({ view }: { view: ReturnType<typeof makeStubView> }) =>
                useCardBusinessColumns(view),
            { initialProps: { view: data } },
        )
        const first = result.current
        rerender({ view: data })
        expect(result.current).toBe(first)
        rerender({ view: makeStubView() })
        expect(result.current).not.toBe(first)
    })

    it('links the customer cell when a customer id is present', () => {
        const container = renderCell(
            'customer',
            makeStubRow({ customerId: 'cu1' }),
        )
        expect(container.textContent).toContain('示例客户')
        expect(container.querySelector('a')?.getAttribute('href')).toBe(
            '/sales/customers/cu1',
        )
    })

    it('renders plain text without a link when the customer id is missing', () => {
        const container = renderCell(
            'customer',
            makeStubRow({ customerId: undefined }),
        )
        expect(container.querySelector('a')).toBeNull()
        expect(container.textContent).toContain('示例客户')
    })

    it('shows an unavailable reason for NONE cost rows and a value otherwise', () => {
        const none = renderCell(
            'cost',
            makeStubRow({ costBasis: 'NONE', costNet: undefined }),
        )
        expect(none.textContent).toContain('无可用成本 · 不显示金额')
        const actual = renderCell(
            'cost',
            makeStubRow({ costBasis: 'ACTUAL', costNet: '60.00' }),
        )
        expect(actual.textContent).not.toContain('无可用成本')
    })

    it('maps coverage status to its badge label', () => {
        expect(
            renderCell('coverage', makeStubRow({ coverageStatus: 'covered' }))
                .textContent,
        ).toContain('已覆盖')
        expect(
            renderCell('coverage', makeStubRow({ coverageStatus: 'partial' }))
                .textContent,
        ).toContain('部分')
        expect(
            renderCell('coverage', makeStubRow({ coverageStatus: 'none' }))
                .textContent,
        ).toContain('未覆盖')
    })

    it('renders drill-down actions including the NONE coverage link', () => {
        const data = makeStubView()
        const container = renderCell(
            'actions',
            makeStubRow({ costBasis: 'NONE', costNet: undefined }),
            data,
        )
        expect(container.textContent).toContain('商城消费订单')
        expect(container.textContent).toContain('供应商订单')
        expect(container.textContent).toContain('接口错误与对账中心')
        const hrefs = Array.from(container.querySelectorAll('a')).map((a) =>
            a.getAttribute('href'),
        )
        expect(hrefs).toContain('/mall/orders/m1')
        expect(hrefs).toContain('/suppliers/orders/s1')
        expect(hrefs).toContain(data.governanceLinks.noneCoverageHref)
    })

    it('renders an em dash for rows without a card instance ref', () => {
        expect(
            renderCell(
                'cardRef',
                makeStubRow({ cardInstanceRef: undefined }),
            ).textContent,
        ).toContain('—')
    })
})
