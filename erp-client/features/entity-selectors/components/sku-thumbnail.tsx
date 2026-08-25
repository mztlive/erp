"use client"

import { PackageIcon } from "lucide-react"

import { useFileAssetQuery } from "@/features/entity-selectors/hooks/queries"
import { cn } from "@/lib/utils"

export function SkuThumbnail({
    assetId,
    label,
    className,
}: {
    assetId?: string
    label: string
    className?: string
}) {
    const query = useFileAssetQuery(assetId)
    const url = query.data?.public_url?.trim()
    return (
        <div
            className={cn(
                "flex size-12 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-muted",
                className,
            )}
        >
            {url ? (
                // eslint-disable-next-line @next/next/no-img-element -- 商品主图来自对象存储公开 URL，且可能跨域。
                <img src={url} alt="" className="size-full object-cover" />
            ) : (
                <PackageIcon
                    className="size-5 text-muted-foreground"
                    aria-hidden="true"
                />
            )}
            <span className="sr-only">{label}主图</span>
        </div>
    )
}
