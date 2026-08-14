import { describe, it, expect, vi, afterEach } from 'vitest'
import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'

import { usePublicationListColumns } from '@/features/product-publications/hooks/use-publication-list-columns'
import type { ProductPublicationRow } from '@/features/product-publications/types'

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

const baseRow: ProductPublicationRow = {
    publicationId: 'pub-1',
    publicationCode: 'PUB-1',
    skuId: 'sku-1',
    skuCode: 'SKU-001',
    productName: '测试商品',
    specification: '500ml',
    targetMallId: 'mall-1',
    targetMallName: '测试商城',
    publicationStatus: 'MALL_LIVE',
    publicationStatusLabel: '商城已生效',
    publicationStatusTone: 'success',
    currentAckedRevisionNo: 1,
    latestRevisionNo: 2,
    hasPendingConfirmation: true,
    salesPriceGross: '9.90',
    fixedOffering: {
        offeringRevisionId: 'offer-1',
        supplierName: '供应商甲',
        availability: 'AVAILABLE',
        availabilityLabel: '可供',
        supplyPriceVisible: false,
    },
    latestDelivery: {
        deliveryId: 'del-1',
        status: 'ACKED',
        statusLabel: '已确认',
        statusTone: 'success',
        attemptCount: 1,
        errorSummary: undefined,
    },
    ownerLabel: '张三',
    updatedAt: '2026-01-01T00:00:00.000Z',
    allowedActions: [],
    actionBlockers: [],
}

function cellContext(
    row: ProductPublicationRow,
): CellContext<ProductPublicationRow, unknown> {
    return { row: { original: row } } as CellContext<ProductPublicationRow, unknown>
}

function renderCell(
    columnId: string,
    row: ProductPublicationRow,
    onPreview: (id: string) => void = () => undefined,
) {
    const { result } = renderHook(() => usePublicationListColumns(onPreview))
    const column = result.current.find((c) => c.id === columnId)
    if (typeof column?.cell !== 'function') {
        throw new Error(`cell renderer missing for ${columnId}`)
    }
    const element = column.cell(cellContext(row)) as React.ReactElement
    return render(element)
}

describe('usePublicationListColumns', () => {
    it('builds the publication columns with expected ids, headers and meta', () => {
        const { result } = renderHook(() => usePublicationListColumns(() => undefined))

        const columns = result.current
        expect(columns.map((c) => c.id)).toEqual([
            'sku',
            'mall',
            'acked',
            'latest',
            'offering',
            'price',
            'pubStatus',
            'delivery',
            'ackAt',
            'owner',
            'actions',
        ])
        expect(columns.map((c) => c.header)).toEqual([
            'SKU / 商品',
            '目标商城',
            '商城生效版',
            '最新发布版',
            '固定供给',
            '含税销售价',
            '发布状态',
            '商城接收',
            '商城确认时间',
            '负责人',
            '操作',
        ])
        expect(columns[0]?.meta?.label).toBe('SKU / 商品')
    })

    it('keeps the same column array across renders for a stable callback (memoized)', () => {
        const onPreview = () => undefined
        const { result, rerender } = renderHook(() =>
            usePublicationListColumns(onPreview),
        )
        const first = result.current
        rerender()
        expect(result.current).toBe(first)
    })

    it('renders the sku cell with code, name, specification and publication code', () => {
        const screen = renderCell('sku', baseRow)

        expect(screen.getByText('SKU-001')).toBeTruthy()
        expect(screen.getByText('测试商品')).toBeTruthy()
        expect(screen.getByText('500ml')).toBeTruthy()
        expect(screen.getByText('PUB-1')).toBeTruthy()
    })

    it('renders the acked cell placeholder when nothing is live yet', () => {
        const screen = renderCell('acked', {
            ...baseRow,
            currentAckedRevisionNo: undefined,
        })

        expect(screen.getByText('尚未生效')).toBeTruthy()
    })

    it('renders the latest cell with the pending confirmation badge', () => {
        const screen = renderCell('latest', baseRow)

        expect(screen.getByText('r2')).toBeTruthy()
        expect(screen.getByText('待确认')).toBeTruthy()
    })

    it('renders the price cell with the gross sales price', () => {
        const screen = renderCell('price', baseRow)

        expect(screen.getByText('¥9.90')).toBeTruthy()
    })

    it('renders the price cell placeholder when no price is present', () => {
        const screen = renderCell('price', {
            ...baseRow,
            salesPriceGross: undefined,
        })

        expect(screen.getByText('—')).toBeTruthy()
    })

    it('renders the publication status badge with its label', () => {
        const screen = renderCell('pubStatus', baseRow)

        expect(screen.getByText('商城已生效')).toBeTruthy()
    })

    it('renders the delivery cell with status and error summary when present', () => {
        const screen = renderCell('delivery', {
            ...baseRow,
            latestDelivery: {
                ...baseRow.latestDelivery!,
                status: 'FAILED',
                statusLabel: '失败',
                statusTone: 'destructive',
                errorSummary: 'E-404',
            },
        })

        expect(screen.getByText('失败')).toBeTruthy()
        expect(screen.getByText('E-404')).toBeTruthy()
    })

    it('renders the delivery cell placeholder when no delivery exists', () => {
        const screen = renderCell('delivery', {
            ...baseRow,
            latestDelivery: undefined,
        })

        expect(screen.getByText('—')).toBeTruthy()
    })

    it('opens the preview and links to the center from the actions cell', () => {
        const onPreview = vi.fn()
        const screen = renderCell('actions', baseRow, onPreview)

        fireEvent.click(screen.getByRole('button', { name: '预览' }))
        expect(onPreview).toHaveBeenCalledWith('pub-1')

        const link = screen.getByRole('link', { name: '打开' })
        expect(link.getAttribute('href')).toBe(
            '/commerce/publications/pub-1',
        )
    })
})
