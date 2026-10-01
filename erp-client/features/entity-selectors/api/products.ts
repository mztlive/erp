import type { ProductComboboxItem } from "@/components/business/entity-comboboxes"

export type SellableSkuComboboxItem = ProductComboboxItem & {
    revisionId: string
    salesVisiblePriceGross?: string
}
