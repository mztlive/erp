import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { CellContext } from '@tanstack/react-table'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { useProductListColumns } from './use-product-list-columns'
import type {
    MasterDataListItem,
    ProductListSkuSummary,
} from '@/features/master-data/types'

afterEach(cleanup)

function makeRow(overrides: Partial<MasterDataListItem> = {}): MasterDataListItem {
    return {
        objectType: 'products',
        stableId: 'p1',
        stableNo: 'P-001',
        name: '示例商品',
        lifecycleStatus: 'ENABLED',
        lifecycleStatusLabel: '启用',
        lifecycleTone: 'success',
        listingStatus: 'LISTED',
        listedSkuCount: 2,
        skuCount: 2,
        revisionTiming: 'CURRENT',
        revisionTimingLabel: '当前生效',
        currentRevisionId: 'r1',
        displayedRevisionId: 'r1',
        revisionNo: 1,
        effectiveFrom: '2026-01-01',
        keyFacts: [{ label: '基础单位', value: '件' }],
        selectorEligibility: [],
        allowedActions: ['CREATE_REVISION', 'DISABLE'],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: [],
        ...overrides,
    }
}

function makeSku(overrides: Partial<ProductListSkuSummary> = {}): ProductListSkuSummary {
    return {
        productId: 'p1',
        skuId: 'sk1',
        skuNo: 'SKU-01',
        skuName: '红色',
        specification: '颜色：红',
        baseUnit: '件',
        salesVisiblePriceGross: '10',
        ...overrides,
    }
}

type ColumnsInput = Parameters<typeof useProductListColumns>[0]

function makeColumnsInput(overrides: Partial<ColumnsInput> = {}): ColumnsInput {
    return {
        canUpdateProductListing: true,
        currentSupplySkuIds: new Set(['sk1']),
        lastFocusedRowId: { current: null },
        productSkusByProduct: new Map([['p1', [makeSku()]]]),
        productSkusPending: false,
        productSkusError: false,
        productListingPending: false,
        productListingProductId: undefined,
        rows: [makeRow()],
        supplierOfferingsPending: false,
        supplierOfferingsError: false,
        onUpdateProductListing: vi.fn(),
        onSupplyProduct: vi.fn(),
        onDisableTarget: vi.fn(),
        ...overrides,
    }
}

function renderCell(
    columnId: string,
    row: MasterDataListItem,
    input: ColumnsInput = makeColumnsInput(),
) {
    const { result } = renderHook(() => useProductListColumns(input))
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

describe('useProductListColumns', () => {
    it('returns the expected column ids in order', () => {
        const { result } = renderHook(() =>
            useProductListColumns(makeColumnsInput()),
        )

        expect(result.current.map((c) => c.id)).toEqual([
            'stableNo',
            'name',
            'revisionNo',
            'lifecycle',
            'skuNames',
            'skuPriceRange',
            'skuCount',
            'supply',
            'listing',
            'revisionTiming',
            'actions',
        ])
    })

    it('adds a blocker column only when some row has a primary blocker', () => {
        const withBlocker = makeColumnsInput({
            rows: [makeRow({ primaryBlocker: '缺少供给' })],
        })
        const { result } = renderHook(() => useProductListColumns(withBlocker))

        expect(result.current.map((c) => c.id)).toContain('blocker')
        const clean = renderHook(() =>
            useProductListColumns(makeColumnsInput()),
        )
        expect(clean.result.current.map((c) => c.id)).not.toContain('blocker')
    })

    it('joins sku names and shows pending / error / empty states', () => {
        const row = makeRow()
        const pending = renderCell('skuNames', row, makeColumnsInput({ productSkusPending: true }))
        expect(pending.getByText('读取中…')).toBeDefined()

        const failed = renderCell('skuNames', row, makeColumnsInput({ productSkusError: true }))
        expect(failed.getByText('暂不可查')).toBeDefined()

        const empty = renderCell('skuNames', row, makeColumnsInput({ productSkusByProduct: new Map() }))
        expect(empty.getByText('—')).toBeDefined()

        const loaded = renderCell(
            'skuNames',
            row,
            makeColumnsInput({
                productSkusByProduct: new Map([
                    ['p1', [makeSku({ skuName: '红色' }), makeSku({ skuId: 'sk2', skuNo: 'SKU-02', skuName: '蓝色' })]],
                ]),
            }),
        )
        expect(loaded.getByText('红色、蓝色')).toBeDefined()
    })

    it('renders the sku price range and sku count', () => {
        const row = makeRow()
        const price = renderCell('skuPriceRange', row)
        expect(price.container.textContent).toBe('¥10.00')

        const failed = renderCell('skuPriceRange', row, makeColumnsInput({ productSkusError: true }))
        expect(failed.getByText('暂不可查')).toBeDefined()

        const count = renderCell('skuCount', row)
        expect(count.getByText('2 个')).toBeDefined()
    })

    it('opens the supply dialog with the clicked row', () => {
        const input = makeColumnsInput()
        const row = makeRow()
        const cell = renderCell('supply', row, input)

        expect(cell.getByText('有供给')).toBeDefined()
        expect(cell.getByText('1/1 SKU')).toBeDefined()

        fireEvent.click(cell.getByLabelText('示例商品供给详情：有供给'))
        expect(input.onSupplyProduct).toHaveBeenCalledWith(row)
        expect(input.lastFocusedRowId.current).toBe('p1')
    })

    it('shows 无供给 and stays accurate with multiple skus', () => {
        const cell = renderCell(
            'supply',
            makeRow({ skuCount: 3 }),
            makeColumnsInput({
                currentSupplySkuIds: new Set(),
                productSkusByProduct: new Map([
                    ['p1', [makeSku(), makeSku({ skuId: 'sk2', skuNo: 'SKU-02' }), makeSku({ skuId: 'sk3', skuNo: 'SKU-03' })]],
                ]),
            }),
        )

        expect(cell.getByText('无供给')).toBeDefined()
        expect(cell.getByText('0/3 SKU')).toBeDefined()
    })

    it('toggles the listing switch and disables it without permission', () => {
        const input = makeColumnsInput()
        const row = makeRow()
        const cell = renderCell('listing', row, input)

        expect(cell.getByText('已上架 2/2')).toBeDefined()
        fireEvent.click(cell.getByLabelText('示例商品整组上架状态'))
        expect(input.onUpdateProductListing).toHaveBeenCalledWith(row, false)
    })

    it('disables the listing switch without update permission', () => {
        const noPermission = renderCell(
            'listing',
            makeRow(),
            makeColumnsInput({ canUpdateProductListing: false }),
        )
        expect(
            noPermission
                .getByLabelText('示例商品整组上架状态')
                .hasAttribute('data-disabled'),
        ).toBe(true)
    })

    it('shows partial listing labels', () => {
        const cell = renderCell(
            'listing',
            makeRow({ listingStatus: 'PARTIALLY_LISTED', listedSkuCount: 1 }),
        )
        expect(cell.getByText('部分上架 1/2')).toBeDefined()
    })

    it('disables the row via the actions column', () => {
        const input = makeColumnsInput()
        const row = makeRow()
        const cell = renderCell('actions', row, input)

        fireEvent.click(cell.getByText('停用'))
        expect(input.onDisableTarget).toHaveBeenCalledWith(row)
        expect(input.lastFocusedRowId.current).toBe('p1')
    })

    it('uses the disabled button when the row cannot be disabled', () => {
        const cell = renderCell(
            'actions',
            makeRow({ allowedActions: [] }),
        )
        expect(cell.getByText('停用').closest('button')!.disabled).toBe(true)
    })
})
