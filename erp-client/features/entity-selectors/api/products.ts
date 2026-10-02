import type { ProductComboboxItem } from "@/components/business/entity-comboboxes"

export type SellableSkuComboboxItem = ProductComboboxItem & {
    revisionId: string
    factoryPriceGross?: string
    salesVisiblePriceGross?: string
    bulkPriceGross?: string
    bulkMinQuantity?: string
    marketPrice?: string
    supplierCodes?: readonly string[]
}
