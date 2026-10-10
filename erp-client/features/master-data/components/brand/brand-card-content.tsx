"use client"

import * as React from "react"
import { ChevronRightIcon, ImageIcon } from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"
import { useFileAssetQuery } from "@/features/master-data/hooks/queries"
import type { MasterDataListItem } from "@/features/master-data/types"
import { cn } from "@/lib/utils"

export function BrandLogoImage({
    assetId,
    name,
    compact = false,
}: {
    assetId?: string
    name: string
    compact?: boolean
}) {
    const query = useFileAssetQuery(assetId)
    const url = query.data?.public_url?.trim()
    const [failedUrl, setFailedUrl] = React.useState<string | null>(null)

    return (
        <span
            className={cn(
                "flex shrink-0 items-center justify-center overflow-hidden bg-surface-sunken/50",
                compact
                    ? "size-20 rounded-xl border border-border p-2"
                    : "h-40 w-full border-b border-border p-6",
            )}
        >
            {url && url !== failedUrl ? (
                // eslint-disable-next-line @next/next/no-img-element -- 品牌 Logo 使用文件资产的公开地址。
                <img
                    src={url}
                    alt={`${name} Logo`}
                    loading="lazy"
                    className="size-full object-contain"
                    onError={() => setFailedUrl(url)}
                />
            ) : query.isFetching ? (
                <Skeleton
                    className={
                        compact ? "size-full rounded-lg" : "size-20 rounded-xl"
                    }
                />
            ) : (
                <span className="flex flex-col items-center gap-2 text-muted-foreground">
                    <ImageIcon
                        className={cn(
                            "opacity-50",
                            compact ? "size-6" : "size-8",
                        )}
                        aria-hidden
                    />
                    <span className={compact ? "text-2xs" : "text-xs"}>
                        {assetId ? "Logo 暂不可用" : "暂无 Logo"}
                    </span>
                </span>
            )}
        </span>
    )
}

export function BrandCardContent({ row }: { row: MasterDataListItem }) {
    return (
        <>
            <BrandLogoImage assetId={row.brandLogoAssetId} name={row.name} />
            <span className="flex w-full min-w-0 flex-1 flex-col gap-4 p-4">
                <span className="min-w-0 space-y-1.5">
                    <span
                        className="line-clamp-2 break-words text-base font-medium text-foreground"
                        title={row.name}
                    >
                        {row.name}
                    </span>
                    <span className="block break-all text-xs text-muted-foreground">
                        品牌代码：
                        <span className="num">
                            {row.dictionaryCode ?? row.stableNo}
                        </span>
                    </span>
                </span>
                <span className="mt-auto flex items-center justify-between gap-3">
                    <BusinessStatusBadge
                        label={row.lifecycleStatusLabel}
                        tone={row.lifecycleTone}
                    />
                    <ChevronRightIcon
                        className="size-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5"
                        aria-hidden
                    />
                </span>
            </span>
        </>
    )
}
