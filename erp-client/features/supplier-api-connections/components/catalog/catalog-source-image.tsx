"use client"

import Image from "next/image"
import { useFileAssetQuery } from "@/features/master-data/hooks/queries"
import type { CatalogImage } from "../../api/catalog"
import { hasVerifiedCatalogImage } from "../../lib/catalog-media"

/** 使用已归档文件展示固定内容，供应商原 URL 的变化不能替换已预览的字节。 */
export function CatalogSourceImage({
    image,
    alt,
    width,
    height,
    className,
}: {
    image: CatalogImage
    alt: string
    width: number
    height: number
    className?: string
}) {
    const asset = useFileAssetQuery(image.file_asset_id ?? undefined)
    if (!hasVerifiedCatalogImage(image))
        return <span className="text-xs text-muted-foreground">图片待核验</span>
    if (!asset.data?.public_url)
        return (
            <span className="text-xs text-muted-foreground">
                {asset.isFetching ? "图片读取中…" : "图片暂不可预览"}
            </span>
        )
    return (
        <Image
            src={asset.data.public_url}
            alt={alt}
            width={width}
            height={height}
            unoptimized
            loading="lazy"
            className={className}
        />
    )
}
