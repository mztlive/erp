"use client"

import * as React from "react"
import { ImageIcon } from "lucide-react"

import { TableIdentityCell } from "@/components/business"
import { Skeleton } from "@/components/ui/skeleton"
import { useFileAssetQuery } from "@/features/master-data/hooks/queries"
import type { MasterDataListItem } from "@/features/master-data/types"
import { cn } from "@/lib/utils"

export function BrandLogoImage({
    assetId,
    name,
    size = "preview",
}: {
    assetId?: string
    name: string
    size?: "preview" | "table"
}) {
    const query = useFileAssetQuery(assetId)
    const url = query.data?.public_url?.trim()
    const [failedUrl, setFailedUrl] = React.useState<string | null>(null)

    return (
        <span
            className={cn(
                "flex shrink-0 items-center justify-center overflow-hidden bg-surface-sunken/50",
                size === "table"
                    ? "size-control rounded-md p-0.5"
                    : "size-20 rounded-xl border border-border p-2",
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
                <Skeleton className="size-full rounded-md" />
            ) : size === "table" ? (
                <span
                    role="img"
                    aria-label={assetId ? "Logo 暂不可用" : "暂无 Logo"}
                    title={assetId ? "Logo 暂不可用" : "暂无 Logo"}
                >
                    <ImageIcon
                        className="size-4 text-muted-foreground"
                        aria-hidden
                    />
                </span>
            ) : (
                <span className="flex flex-col items-center gap-2 text-muted-foreground">
                    <ImageIcon className="size-6 opacity-50" aria-hidden />
                    <span className="text-tiny">
                        {assetId ? "Logo 暂不可用" : "暂无 Logo"}
                    </span>
                </span>
            )}
        </span>
    )
}

export function BrandListIdentity({ row }: { row: MasterDataListItem }) {
    const code = row.dictionaryCode ?? row.stableNo
    return (
        <div className="flex min-w-0 items-center gap-2.5">
            <BrandLogoImage
                assetId={row.brandLogoAssetId}
                name={row.name}
                size="table"
            />
            <TableIdentityCell
                primary={row.name}
                primaryTitle={row.name}
                secondary={`品牌代码：${code}`}
                secondaryTitle={code}
                numericSecondary
            />
        </div>
    )
}
