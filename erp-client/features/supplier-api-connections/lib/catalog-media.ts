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
    const missing = [
        ...(!item.images?.length ? ["暂无商品主图和轮播图"] : []),
        ...(item.images?.length && !hasVerifiedCatalogImage(item.images[0])
            ? ["商品主图未完成"]
            : []),
        ...(item.images ?? []).flatMap((image, index) =>
            hasVerifiedCatalogImage(image) ? [] : [`轮播图第 ${index + 1} 张`],
        ),
        ...(!item.detail_images?.length ? ["暂无详情图"] : []),
        ...(item.detail_images ?? []).flatMap((image, index) =>
            hasVerifiedCatalogImage(image) ? [] : [`详情图第 ${index + 1} 张`],
        ),
    ]
    if (!missing.length) return undefined
    const reason = item.media_errors?.length
        ? `；${item.media_errors.join("；")}`
        : ""
    return `${missing.join("、")}${reason}，不影响选品`
}
