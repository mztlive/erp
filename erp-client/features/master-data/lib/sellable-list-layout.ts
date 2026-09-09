export const SELLABLE_LIST_LAYOUTS = ["table", "gallery"] as const

export type SellableListLayout = (typeof SELLABLE_LIST_LAYOUTS)[number]

export const SELLABLE_GALLERY_BATCH_SIZE = 24

export function parseSellableListLayout(
    value: string | null | undefined,
): SellableListLayout {
    return value === "gallery" ? "gallery" : "table"
}
