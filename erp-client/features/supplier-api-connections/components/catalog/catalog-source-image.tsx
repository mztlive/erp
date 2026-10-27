"use client"

import Image from "next/image"
import { ImageOffIcon } from "lucide-react"
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
    compact = false,
}: {
    image: CatalogImage
    alt: string
    width: number
    height: number
    className?: string
    compact?: boolean
}) {
    const verified = hasVerifiedCatalogImage(image)
    const asset = useFileAssetQuery(
        verified ? (image.file_asset_id ?? undefined) : undefined,
    )
    const placeholder = (message: string) =>
        compact ? (
            <span
                className="flex size-full items-center justify-center rounded-md bg-muted text-muted-foreground"
                aria-label={message}
            >
                <ImageOffIcon className="size-4" aria-hidden="true" />
            </span>
        ) : (
            <span className="block rounded-md bg-muted/40 p-2 text-xs text-muted-foreground">
                {message}
            </span>
        )
    if (!verified) return placeholder("图片尚未同步")
    if (!asset.data?.public_url)
        return placeholder(asset.isFetching ? "图片读取中…" : "图片暂不可预览")
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
