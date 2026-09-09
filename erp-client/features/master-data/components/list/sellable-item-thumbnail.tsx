"use client"

import { PackageIcon } from "lucide-react"

import { Skeleton } from "@/components/ui/skeleton"
import { useFileAssetQuery } from "@/features/master-data/hooks/queries"
import { cn } from "@/lib/utils"

export function SellableItemThumbnail({
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
                "flex aspect-square items-center justify-center overflow-hidden bg-muted",
                className,
            )}
        >
            {url ? (
                // eslint-disable-next-line @next/next/no-img-element -- 商品主图来自对象存储公开 URL，且可能跨域。
                <img
                    src={url}
                    alt=""
                    className="size-full object-cover"
                    loading="lazy"
                />
            ) : query.isFetching ? (
                <Skeleton className="size-full rounded-none" />
            ) : (
                <PackageIcon
                    className="size-8 text-muted-foreground"
                    aria-hidden="true"
                />
            )}
            <span className="sr-only">{label}主图</span>
        </div>
    )
}
