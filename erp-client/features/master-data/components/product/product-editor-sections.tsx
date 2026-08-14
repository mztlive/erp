"use client"

/**
 * 商品编辑页分区组件的统一出口。
 * 实现拆分在 product-{basic,media,effective,history}-section.tsx，保持原有导出名。
 */

import * as React from "react"

import { ProductBasicSection } from "@/features/master-data/components/product/product-basic-section"
import { ProductEffectiveSection } from "@/features/master-data/components/product/product-effective-section"
import { ProductHistorySection } from "@/features/master-data/components/product/product-history-section"
import { ProductMediaSection } from "@/features/master-data/components/product/product-media-section"
import type { ProductFields } from "@/features/master-data/types"

type SetProductFields = (next: React.SetStateAction<ProductFields>) => void

export {
    ProductBasicSection,
    ProductEffectiveSection,
    ProductHistorySection,
    ProductMediaSection,
}
export type { SetProductFields }
