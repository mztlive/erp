import type { CatalogImage, CatalogItem } from "../api/catalog"

export function hasVerifiedCatalogImage(image: CatalogImage): boolean {
    return Boolean(
        image.content_fingerprint &&
        /^[0-9a-f]{64}$/i.test(image.content_fingerprint) &&
        image.file_asset_id?.trim() &&
        !image.file_asset_id.startsWith("pending-file:"),
    )
}

/** 未核验素材不能被选品操作当作已经看过的固定图片。 */
export function catalogMediaBlocker(item: CatalogItem): string | undefined {
    const images = [...(item.images ?? []), ...(item.detail_images ?? [])]
    return images.some((image) => !hasVerifiedCatalogImage(image))
        ? "商品图片待核验，请先重新同步商品"
        : undefined
}
