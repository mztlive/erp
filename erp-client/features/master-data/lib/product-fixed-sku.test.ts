import { describe, it, expect } from 'vitest'

import { toFixedSku } from './product-fixed-sku'
import { emptyProductFields } from '@/features/master-data/lib/product-model'

describe('toFixedSku', () => {
    it('maps product and sku fields onto the supply dialog fixed sku', () => {
        const base = emptyProductFields()
        const sku = base.skus[0]
        const fields = {
            ...base,
            productKind: 'PHYSICAL' as const,
            baseUnit: '件',
            category: '办公用品',
            brand: '得力',
            description: '一支好笔',
            carouselImages: ['a.png'],
            detailImages: ['b.png'],
            carouselPreviewUrls: { 'a.png': 'https://cdn/a.png' },
            detailPreviewUrls: { 'b.png': 'https://cdn/b.png' },
            carouselFileAssetIds: { 'a.png': 'fa-1' },
            detailFileAssetIds: { 'b.png': 'fa-2' },
            skus: [
                {
                    ...sku,
                    skuId: 'sk1',
                    skuNo: 'SKU-01',
                    name: '红色',
                    specLabel: '颜色：红',
                    barcode: '6901234567890',
                    mainImage: 'main.png',
                    mainImageAssetId: 'fa-3',
                    mainImagePreviewUrl: 'https://cdn/main.png',
                },
            ],
        }

        expect(toFixedSku(fields, fields.skus[0], '示例商品')).toEqual({
            skuId: 'sk1',
            skuCode: 'SKU-01',
            skuName: '红色',
            productKind: 'PHYSICAL',
            specification: '颜色：红',
            baseUnit: '件',
            category: '办公用品',
            brand: '得力',
            barcode: '6901234567890',
            description: '一支好笔',
            carouselImages: ['a.png'],
            detailImages: ['b.png'],
            carouselFileAssetIds: { 'a.png': 'fa-1' },
            detailFileAssetIds: { 'b.png': 'fa-2' },
            carouselPreviewUrls: { 'a.png': 'https://cdn/a.png' },
            detailPreviewUrls: { 'b.png': 'https://cdn/b.png' },
            mainImage: 'main.png',
            mainImageAssetId: 'fa-3',
            mainImagePreviewUrl: 'https://cdn/main.png',
        })
    })

    it('falls back to the product name for an empty sku name', () => {
        const fields = emptyProductFields()

        expect(toFixedSku(fields, fields.skus[0], '示例商品').skuName).toBe(
            '示例商品',
        )
    })

    it('omits empty optional fields and falls back to the product base unit', () => {
        const fields = { ...emptyProductFields(), baseUnit: '件' }

        const fixed = toFixedSku(fields, fields.skus[0], '示例商品')

        expect(fixed.category).toBeUndefined()
        expect(fixed.brand).toBeUndefined()
        expect(fixed.barcode).toBeUndefined()
        expect(fixed.mainImage).toBeUndefined()
        expect(fixed.baseUnit).toBe('件')
    })
})
