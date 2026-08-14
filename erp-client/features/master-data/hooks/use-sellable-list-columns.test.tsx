import { cleanup, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

import { useSellableListColumns } from './use-sellable-list-columns'
import type { MasterDataListItem } from '@/features/master-data/types'

afterEach(cleanup)

function makeRow(
    sellable: NonNullable<MasterDataListItem['sellableItem']> | null,
): MasterDataListItem {
    return {
        objectType: 'sellable-items',
        stableId: 'sk1',
        stableNo: 'SKU-001',
        name: '签字笔',
        lifecycleStatus: 'ENABLED',
        lifecycleStatusLabel: '启用',
        lifecycleTone: 'success',
        revisionTiming: 'CURRENT',
        revisionTimingLabel: '当前生效',
        currentRevisionId: 'r1',
        displayedRevisionId: 'r1',
        revisionNo: 1,
        effectiveFrom: '2026-01-01',
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: [],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: [],
        sellableItem: sellable ?? undefined,
    }
}

function makeSellable(
    overrides: Partial<
        NonNullable<MasterDataListItem['sellableItem']>
    > = {},
): NonNullable<MasterDataListItem['sellableItem']> {
    return {
        productId: 'p1',
        productNo: 'P-001',
        specificationAttributes: [{ name: '颜色', value: '红' }],
        specificationLabel: '颜色：红',
        baseUnit: '件',
        productKindLabel: '实物',
        salesVisiblePriceGross: '12.50',
        supplierCount: 3,
        supplyRegions: ['华东', '华南'],
        eligibilityAsOf: '2026-08-14',
        ...overrides,
    }
}

function renderCell(columnId: string, row: MasterDataListItem) {
    const { result } = renderHook(() => useSellableListColumns())
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = {
        row: { original: row },
    } as CellContext<MasterDataListItem, unknown>
    const cell = column!.cell as
        | ((props: CellContext<MasterDataListItem, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

describe('useSellableListColumns', () => {
    it('returns the expected column ids in order', () => {
        const { result } = renderHook(() => useSellableListColumns())

        expect(result.current.map((c) => c.id)).toEqual([
            'name',
            'productNo',
            'price',
            'marketPrice',
            'supplyRegions',
            'supplierCount',
        ])
    })

    it('renders sku name with specification and sku number', () => {
        const cell = renderCell('name', makeRow(makeSellable()))

        expect(cell.getByText('签字笔')).toBeDefined()
        expect(cell.getByText(/颜色：红/)).toBeDefined()
        expect(cell.getByText('SKU-001')).toBeDefined()
    })

    it('renders the product number and sales price with tax hint', () => {
        const productNo = renderCell('productNo', makeRow(makeSellable()))
        expect(productNo.getByText('P-001')).toBeDefined()

        const price = renderCell('price', makeRow(makeSellable()))
        expect(price.getByText('¥12.50')).toBeDefined()
        expect(price.getByText('含税')).toBeDefined()
    })

    it('renders a dash for a missing product number', () => {
        const noProduct = renderCell('productNo', makeRow(null))
        expect(noProduct.getByText('—')).toBeDefined()
    })

    it('renders a dash for a missing market price', () => {
        const noMarket = renderCell(
            'marketPrice',
            makeRow(makeSellable({ marketPrice: undefined })),
        )
        expect(noMarket.getByText('—')).toBeDefined()
    })

    it('renders market price when present', () => {
        const cell = renderCell('marketPrice', makeRow(makeSellable({ marketPrice: '9.90' })))
        expect(cell.getByText('¥9.90')).toBeDefined()
    })

    it('joins supply regions and shows 未标注 when empty', () => {
        const joined = renderCell('supplyRegions', makeRow(makeSellable()))
        expect(joined.getByText('华东、华南')).toBeDefined()

        const empty = renderCell('supplyRegions', makeRow(makeSellable({ supplyRegions: [] })))
        expect(empty.getByText('未标注')).toBeDefined()
    })

    it('renders the supplier count badge', () => {
        const cell = renderCell('supplierCount', makeRow(makeSellable()))

        expect(cell.getByText('3')).toBeDefined()
        expect(cell.getByText('家')).toBeDefined()
    })
})
