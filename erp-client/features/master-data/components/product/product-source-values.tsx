"use client"

import * as React from "react"
import Image from "next/image"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useFileAssetQuery } from "../../hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    ProductSourceImage,
    ProductSourceValue,
} from "../../api/types-product-source"

export function sourceImageKey(image: ProductSourceImage): string {
    return (
        image.file_asset_id ??
        `${image.url ?? "missing"}:${image.content_fingerprint ?? "unknown"}`
    )
}

export function canAdoptSource(value: ProductSourceValue): boolean {
    return (
        value.type === "TEXT" ||
        value.value.every((image) => Boolean(image.file_asset_id))
    )
}

function changedText(current: string, other: string) {
    let start = 0
    while (
        start < current.length &&
        start < other.length &&
        current[start] === other[start]
    )
        start += 1
    let end = 0
    while (
        end < current.length - start &&
        end < other.length - start &&
        current[current.length - end - 1] === other[other.length - end - 1]
    )
        end += 1
    return {
        before: current.slice(0, start),
        changed: current.slice(start, current.length - end),
        after: end ? current.slice(current.length - end) : "",
    }
}

/** 文本差异只作为可读提示，提交始终使用接口原值。 */
export function SourceText({
    value,
    other,
    incoming,
}: {
    value: string | null
    other: string | null
    incoming: boolean
}) {
    if (!value) return <p className="text-sm text-muted-foreground">（空）</p>
    const parts = changedText(value, other ?? "")
    return (
        <p className="whitespace-pre-wrap break-words text-sm leading-relaxed">
            {parts.before}
            {parts.changed ? (
                incoming ? (
                    <ins className="rounded bg-success-soft px-0.5 text-success-soft-foreground no-underline">
                        {parts.changed}
                    </ins>
                ) : (
                    <del className="rounded bg-destructive-soft px-0.5 text-destructive-soft-foreground">
                        {parts.changed}
                    </del>
                )
            ) : null}
            {parts.after}
        </p>
    )
}

export function SourceImagePreview({
    image,
    alt,
    thumbnail = false,
    expanded = false,
}: {
    image: ProductSourceImage
    alt: string
    thumbnail?: boolean
    expanded?: boolean
}) {
    const asset = useFileAssetQuery(image.file_asset_id ?? undefined)
    const [failed, setFailed] = React.useState(false)
    const url = image.file_asset_id ? asset.data?.public_url : image.url
    return url && !failed ? (
        <Image
            src={url}
            width={thumbnail ? 48 : 400}
            height={thumbnail ? 48 : 400}
            unoptimized
            alt={alt}
            className={
                thumbnail
                    ? "size-12 rounded object-contain"
                    : expanded
                      ? "h-auto w-full object-contain"
                      : "max-h-full w-full object-contain"
            }
            loading="lazy"
            referrerPolicy="no-referrer"
            onError={() => setFailed(true)}
        />
    ) : (
        <span className="px-3 text-center text-xs text-muted-foreground">
            {asset.isFetching
                ? "图片读取中…"
                : asset.isError
                  ? "图片读取失败"
                  : "图片暂不可预览"}
        </span>
    )
}

function SourceImage({
    image,
    position,
    idPrefix,
}: {
    image: ProductSourceImage
    position: number
    idPrefix: string
}) {
    const [expanded, setExpanded] = React.useState(false)
    return (
        <div className="space-y-2">
            <div className="flex aspect-square items-center justify-center overflow-hidden rounded-md border bg-muted/20">
                <SourceImagePreview
                    image={image}
                    alt={`第 ${position} 张商品图片`}
                />
            </div>
            <p className="text-xs text-muted-foreground">第 {position} 张</p>
            <Button
                id={`${idPrefix}-preview`}
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setExpanded(true)}
            >
                放大查看
            </Button>
            <Dialog open={expanded} onOpenChange={setExpanded}>
                <DialogContent
                    closeButtonId={`${idPrefix}-preview-close`}
                    className="max-h-[90dvh] sm:max-w-4xl"
                >
                    <DialogHeader>
                        <DialogTitle>第 {position} 张商品图片</DialogTitle>
                        <DialogDescription>
                            按原图内容核对，长图可向下滚动查看。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="min-h-0 overflow-y-auto">
                        <SourceImagePreview
                            image={image}
                            alt={`第 ${position} 张商品图片原图`}
                            expanded
                        />
                    </div>
                </DialogContent>
            </Dialog>
        </div>
    )
}

export function SourceImages({
    images,
    otherImages,
    incoming,
    idPrefix,
}: {
    images: readonly ProductSourceImage[]
    otherImages: readonly ProductSourceImage[]
    incoming: boolean
    idPrefix: string
}) {
    if (!images.length)
        return <p className="text-sm text-muted-foreground">（无图片）</p>
    return (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            {images.map((image, index) => {
                const key = sourceImageKey(image)
                const otherIndex = otherImages.findIndex(
                    (other) => sourceImageKey(other) === key,
                )
                const other = otherImages[otherIndex]
                const uncertain =
                    (!image.content_fingerprint && !image.file_asset_id) ||
                    Boolean(
                        other &&
                        !other.content_fingerprint &&
                        !other.file_asset_id,
                    )
                const contentChanged = Boolean(
                    other?.content_fingerprint &&
                    image.content_fingerprint &&
                    other.content_fingerprint !== image.content_fingerprint,
                )
                return (
                    <div key={key} className="space-y-2">
                        <SourceImage
                            image={image}
                            position={index + 1}
                            idPrefix={`${idPrefix}-${toAutomationIdSegment(key)}`}
                        />
                        <div className="flex flex-wrap gap-1">
                            {otherIndex < 0 ? (
                                <Badge variant="outline">
                                    {incoming ? "新增或替换" : "将被移除或替换"}
                                </Badge>
                            ) : contentChanged ? (
                                <Badge variant="outline">图片内容变化</Badge>
                            ) : otherIndex !== index ? (
                                <Badge variant="outline">排序变化</Badge>
                            ) : null}
                            {uncertain ? (
                                <Badge variant="outline">内容待核验</Badge>
                            ) : null}
                            {!image.file_asset_id && incoming ? (
                                <Badge variant="outline">暂不可采用</Badge>
                            ) : null}
                        </div>
                    </div>
                )
            })}
        </div>
    )
}

export function SourceValueComparison({
    company,
    supplier,
    idPrefix,
}: {
    company: ProductSourceValue
    supplier: ProductSourceValue
    idPrefix: string
}) {
    return (
        <div className="grid min-w-0 gap-4 md:grid-cols-2">
            <div className="min-w-0 space-y-3 rounded-lg border p-4">
                <p className="text-sm font-medium">公司当前内容</p>
                {company.type === "TEXT" ? (
                    <SourceText
                        value={company.value}
                        other={supplier.type === "TEXT" ? supplier.value : null}
                        incoming={false}
                    />
                ) : (
                    <SourceImages
                        idPrefix={`${idPrefix}-company`}
                        images={company.value}
                        otherImages={
                            supplier.type === "IMAGES" ? supplier.value : []
                        }
                        incoming={false}
                    />
                )}
            </div>
            <div className="min-w-0 space-y-3 rounded-lg border p-4">
                <p className="text-sm font-medium">供应商最新内容</p>
                {supplier.type === "TEXT" ? (
                    <SourceText
                        value={supplier.value}
                        other={company.type === "TEXT" ? company.value : null}
                        incoming
                    />
                ) : (
                    <SourceImages
                        idPrefix={`${idPrefix}-supplier`}
                        images={supplier.value}
                        otherImages={
                            company.type === "IMAGES" ? company.value : []
                        }
                        incoming
                    />
                )}
            </div>
        </div>
    )
}
