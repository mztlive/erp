"use client"

import * as React from "react"
import { ImageOffIcon, PackageIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { useFileAssetQuery } from "@/features/master-data/hooks/queries"
import { getErrorPresentation } from "@/lib/api/errors"
import { cn } from "@/lib/utils"

/** 列表与详情共用主图缓存；区分无图、资源失效与请求失败。 */
export function InquiryProductImage({
    assetId,
    label,
    pending,
    lookupFailed,
    idPrefix,
    detail = false,
}: {
    assetId?: string
    label: string
    pending: boolean
    lookupFailed: boolean
    idPrefix: string
    detail?: boolean
}) {
    const query = useFileAssetQuery(assetId)
    const [failedUrl, setFailedUrl] = React.useState<string | null>(null)
    const [attempt, setAttempt] = React.useState(0)
    const url = query.data?.public_url?.trim()
    const imageFailed = Boolean(url && failedUrl === url)
    const loading = pending || Boolean(assetId && query.isPending)
    const failure = query.isError ? getErrorPresentation(query.error) : null
    const status =
        lookupFailed || query.isError || imageFailed
            ? "图片加载失败"
            : assetId
              ? "图片已失效"
              : "暂无图片"
    const description =
        failure?.description ??
        (imageFailed ? "图片资源无法加载，请重试。" : status)
    const showImage = url && !imageFailed
    const PlaceholderIcon = status === "暂无图片" ? PackageIcon : ImageOffIcon

    return (
        <span
            className={cn(
                "block min-w-0",
                detail ? "space-y-2" : "size-12 shrink-0",
            )}
        >
            <span
                className={cn(
                    "flex items-center justify-center overflow-hidden rounded-lg border bg-card",
                    detail ? "h-48 w-full" : "size-12",
                )}
                title={
                    showImage
                        ? `${label}主图`
                        : loading
                          ? "图片加载中"
                          : description
                }
            >
                {showImage ? (
                    // eslint-disable-next-line @next/next/no-img-element -- 商品主图使用对象存储公开 URL，沿用商品池的跨域图片展示方式。
                    <img
                        key={`${url}-${attempt}`}
                        src={url}
                        alt={`${label}主图`}
                        loading="lazy"
                        className="size-full object-contain p-1"
                        onError={() => setFailedUrl(url)}
                    />
                ) : loading ? (
                    <Skeleton
                        className="size-full rounded-none"
                        aria-label="图片加载中"
                    />
                ) : (
                    <span
                        className="flex flex-col items-center gap-2 p-1 text-center text-xs text-muted-foreground"
                        role="img"
                        aria-label={`${label}：${status}`}
                    >
                        <PlaceholderIcon
                            className={detail ? "size-8" : "size-5"}
                            aria-hidden="true"
                        />
                        {detail && <span>{status}</span>}
                    </span>
                )}
            </span>
            {detail && (query.isError || imageFailed) && (
                <span
                    className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
                    role="status"
                >
                    <span className="min-w-0 wrap-anywhere">
                        {description}
                        {failure?.requestId &&
                            `（请求编号：${failure.requestId}）`}
                    </span>
                    <Button
                        id={`${idPrefix}-retry`}
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={query.isFetching}
                        onClick={() => {
                            setFailedUrl(null)
                            setAttempt((value) => value + 1)
                            void query.refetch()
                        }}
                    >
                        重试加载图片
                    </Button>
                </span>
            )}
        </span>
    )
}
