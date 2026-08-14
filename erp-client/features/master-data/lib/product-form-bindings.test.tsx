import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { useAppForm } from '@/components/form'
import { createProductDefaults } from '@/features/master-data/lib/product-editor-model'
import { createProductFormBindings } from './product-form-bindings'
import type { ProductFields } from '@/features/master-data/types'

function makeValidFields(): ProductFields {
    const base = createProductDefaults(true)
    return {
        ...base.fields,
        productNo: 'P-009',
        productKind: 'PHYSICAL',
        baseUnitId: 'u1',
        baseUnitCode: 'pc',
        baseUnit: '件',
        categoryId: 'c1',
        category: '办公用品',
        brandId: 'b1',
        brand: '得力',
        skus: [
            {
                ...base.fields.skus[0],
                skuId: 'sk9',
                skuNo: 'SKU-01',
                name: '红色',
                mainImage: 'a.png',
            },
        ],
    }
}

function renderHarness(
    isCreate = true,
    fallbackName: string | undefined = undefined,
) {
    const onSubmit = vi.fn()
    const hook = renderHook(() => {
        const form = useAppForm({
            defaultValues: createProductDefaults(isCreate),
            onSubmit: async () => {
                onSubmit()
            },
        })
        const bindings = createProductFormBindings(
            form,
            form.state.values,
            isCreate,
            fallbackName,
        )
        return { form, bindings }
    })
    return { ...hook, onSubmit }
}

function rebind(harness: ReturnType<typeof renderHarness>) {
    return createProductFormBindings(
        harness.result.current.form,
        harness.result.current.form.state.values,
        true,
        undefined,
    )
}

describe('createProductFormBindings', () => {
    beforeEach(() => {
        vi.spyOn(window, 'confirm').mockReturnValue(true)
    })

    afterEach(() => {
        vi.restoreAllMocks()
    })

    it('derives the page title for create and edit modes', () => {
        const creating = renderHarness(true)
        expect(creating.result.current.bindings.title).toBe('新建商品')

        const { result: editing } = renderHook(() => {
            const form = useAppForm({
                defaultValues: createProductDefaults(false),
            })
            return createProductFormBindings(form, form.state.values, false, '示例商品')
        })
        expect(editing.current.title).toBe('示例商品')

        const { result: named } = renderHook(() => {
            const form = useAppForm({
                defaultValues: createProductDefaults(false),
            })
            return createProductFormBindings(
                form,
                { ...form.state.values, name: '改名商品' },
                false,
                '示例商品',
            )
        })
        expect(named.current.title).toBe('改名商品')

        const { result: unnamed } = renderHook(() => {
            const form = useAppForm({
                defaultValues: createProductDefaults(false),
            })
            return createProductFormBindings(form, form.state.values, false, undefined)
        })
        expect(unnamed.current.title).toBe('商品详情')
    })

    it('writes scalar setters through to the form values', () => {
        const harness = renderHarness()

        act(() => {
            harness.result.current.bindings.setName('新品')
            harness.result.current.bindings.setEffectiveFrom('2026-02-01')
            harness.result.current.bindings.setEffectiveTo('2026-12-31')
            harness.result.current.bindings.setChangeReason('上新')
        })

        const values = harness.result.current.form.state.values
        expect(values.name).toBe('新品')
        expect(values.effectiveFrom).toBe('2026-02-01')
        expect(values.effectiveTo).toBe('2026-12-31')
        expect(values.changeReason).toBe('上新')
    })

    it('supports functional field updates and patching a single sku row', () => {
        const harness = renderHarness()

        act(() => {
            harness.result.current.bindings.setFields((previous) => ({
                ...previous,
                productNo: 'P-001',
            }))
        })
        expect(
            harness.result.current.form.state.values.fields.productNo,
        ).toBe('P-001')

        act(() => {
            harness.result.current.bindings.updateSku(0, { name: '蓝色' })
        })
        expect(
            harness.result.current.form.state.values.fields.skus[0].name,
        ).toBe('蓝色')
    })

    it('syncs spec drafts into trimmed specs and rebuilt skus', () => {
        const harness = renderHarness()

        act(() => {
            harness.result.current.bindings.syncSpecDrafts([
                { name: ' 颜色 ', values: [' 红 ', ''] },
            ])
        })

        const fields = harness.result.current.form.state.values.fields
        expect(fields.specs).toEqual([{ name: '颜色', values: ['红'] }])
        expect(fields.skus).toHaveLength(1)
        expect(fields.skus[0].attributeValues).toEqual(['红'])
    })

    it('applies batch reference prices to every sku after confirmation', () => {
        const harness = renderHarness()
        act(() => {
            harness.result.current.bindings.setFields(makeValidFields())
        })
        const form = harness.result.current.form

        const withPrices = createProductFormBindings(
            form,
            {
                ...form.state.values,
                batchSalePrice: '5',
                batchMarketPrice: '8',
            },
            true,
            undefined,
        )
        act(() => withPrices.applyBatchReferencePrices())

        const fields = form.state.values.fields
        expect(fields.skus[0].salePrice).toBe('5')
        expect(fields.skus[0].marketPrice).toBe('8')
    })

    it('skips batch prices when the user declines the confirmation', () => {
        vi.mocked(window.confirm).mockReturnValue(false)
        const harness = renderHarness()
        act(() => {
            harness.result.current.bindings.setFields(makeValidFields())
        })
        const form = harness.result.current.form

        const withPrices = createProductFormBindings(
            form,
            {
                ...form.state.values,
                batchSalePrice: '5',
                batchMarketPrice: '8',
            },
            true,
            undefined,
        )
        act(() => withPrices.applyBatchReferencePrices())

        expect(form.state.values.fields.skus[0].salePrice).toBeUndefined()
        expect(form.state.values.fields.skus[0].marketPrice).toBeUndefined()
    })

    it('leaves skus untouched when no batch price is entered', () => {
        const harness = renderHarness()
        act(() => {
            harness.result.current.bindings.setFields(makeValidFields())
        })
        const before = harness.result.current.form.state.values.fields.skus

        act(() => rebind(harness).applyBatchReferencePrices())

        expect(harness.result.current.form.state.values.fields.skus).toEqual(
            before,
        )
    })

    it('derives inventory preview skus and the action hint', () => {
        const harness = renderHarness()
        expect(harness.result.current.bindings.inventoryPreviewSkus).toEqual([])
        expect(harness.result.current.bindings.inventoryActionHint).toBe(
            '选择实物商品类型并保存 SKU 后可查看正式库存',
        )

        act(() => {
            harness.result.current.bindings.setFields(makeValidFields())
        })
        const bound = rebind(harness)
        expect(bound.inventoryPreviewSkus).toEqual([
            {
                skuId: 'sk9',
                skuNo: 'SKU-01',
                specLabel: '默认规格',
                baseUnit: '件',
            },
        ])
        expect(bound.inventoryActionHint).toBeUndefined()
    })

    it('marks non-physical products as ineligible for company inventory', () => {
        const harness = renderHarness()
        act(() => {
            harness.result.current.bindings.setFields({
                ...makeValidFields(),
                productKind: 'VOUCHER',
            })
        })

        const bound = rebind(harness)
        expect(bound.inventoryPreviewSkus).toEqual([])
        expect(bound.inventoryActionHint).toBe(
            '仅实物商品适用公司自有库存台账',
        )
    })

    it('submits the form through handleSubmit', async () => {
        const harness = renderHarness()

        await act(async () => {
            harness.result.current.bindings.handleSubmit()
        })

        expect(harness.onSubmit).toHaveBeenCalledTimes(1)
    })
})
