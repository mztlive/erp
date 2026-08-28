import { z } from "zod"

import { compareDecimal } from "@/lib/fixed-decimal"
import type {
    ProductPublicationRevisionView,
    SaleStatus,
} from "@/features/product-publications/types"

export type SessionEdit = {
    baselineRevisionId: string
    name: string
    specification: string
    salesDescription: string
    minimumPurchaseQuantity: string
    salesPriceGross: string
    salesTaxRate: string
    saleStatus: SaleStatus
    baseUnitCode: string
    salesRegion: string[]
    categoryId: string
    skuRevisionId: string
    supplierOfferingRevisionId: string
    productCapabilities: string[]
    validFrom: string
    validTo?: string
    media: ProductPublicationRevisionView["media"]
}

const publishDecimal = (label: string, maxScale: number, positive = false) =>
    z
        .string()
        .trim()
        .regex(
            new RegExp(`^\\d+(?:\\.\\d{1,${maxScale}})?$`),
            `${label}最多保留 ${maxScale} 位小数`,
        )
        .refine(
            (value) => !positive || /[1-9]/.test(value),
            `${label}必须大于 0`,
        )

function decimalAtMost(value: string, maximum: string, maxScale: number) {
    try {
        return compareDecimal(value, maximum, maxScale) <= 0
    } catch {
        return false
    }
}

export const publishSchema = z.object({
    name: z.string().trim().min(1, "请填写展示名称"),
    specification: z.string().trim().min(1, "请填写规格"),
    salesDescription: z.string().trim().min(1, "请填写商城销售说明"),
    minimumPurchaseQuantity: publishDecimal("最小购买量", 6, true),
    salesPriceGross: publishDecimal("含税销售价", 4, true),
    salesTaxRate: publishDecimal("销项税率", 6).refine(
        (value) => decimalAtMost(value, "1", 6),
        "税率请填 0 到 1 之间的小数，如 0.13 表示 13%",
    ),
    categoryId: z.string().trim().min(1, "请填写商城类目编号"),
    skuRevisionId: z.string().trim().min(1, "请填写 SKU 修订编号"),
    supplierOfferingRevisionId: z.string().trim().min(1, "请选择固定供给修订编号"),
    baseUnitCode: z.string().trim().min(1, "请填写基础单位代码"),
    salesRegionText: z.string().trim().min(1, "请填写可销售区域"),
    productCapabilitiesText: z.string(),
    validFrom: z.string().min(1, "请选择生效时间"),
    validTo: z.string(),
    media: z
        .array(
            z.object({
                fileAssetId: z.string().min(1),
                mediaRole: z.enum(["MAIN", "CAROUSEL", "DETAIL"]),
                sortNo: z.number().int().nonnegative(),
                altText: z.string().trim().min(1, "请填写图片说明"),
            }),
        )
        .min(1, "至少需要一张图片"),
    saleStatus: z.enum(["ON_SALE", "OFF_SALE", "PAUSED"]),
})
