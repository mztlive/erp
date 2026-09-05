"use client"

import Image from "next/image"

import {
    ArrowLeftIcon,
    BanIcon,
    ImagePlusIcon,
    MoreHorizontalIcon,
    SaveIcon,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { productKindLabel } from "@/features/master-data/api/presentation"
import type {
    MasterDataCenterView,
    ProductFields,
} from "@/features/master-data/types"

type Props = {
    isCreate: boolean
    data: MasterDataCenterView | null | undefined
    title: string
    fields: ProductFields
    canDisable: boolean
    disableBlocker: { message: string } | undefined
    setDisableOpen: (open: boolean) => void
    canRevise: boolean
    pending: boolean
    onBack: () => void
    onMedia: () => void
    onSave: () => void
}

export function ProductDetailHeader({
    isCreate,
    data,
    title,
    fields,
    canDisable,
    disableBlocker,
    setDisableOpen,
    canRevise,
    pending,
    onBack,
    onMedia,
    onSave,
}: Props) {
    const image =
        fields.carouselPreviewUrls[fields.carouselImages[0]] ||
        fields.skus.find((sku) => sku.mainImagePreviewUrl)?.mainImagePreviewUrl
    return (
        <header className="space-y-4">
            <div className="flex items-center justify-between gap-3">
                <Button
                    id="master-data-product-detail-back-list"
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="-ml-2 text-muted-foreground"
                    onClick={onBack}
                >
                    <ArrowLeftIcon aria-hidden />
                    商品列表
                </Button>
                {!isCreate && data ? (
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id="master-data-product-detail-more"
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                />
                            }
                        >
                            <MoreHorizontalIcon aria-hidden />
                            更多操作
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                            <DropdownMenuItem
                                id="master-data-product-detail-header-disable"
                                variant="destructive"
                                disabled={!canDisable || pending}
                                title={disableBlocker?.message}
                                onClick={() => setDisableOpen(true)}
                            >
                                <BanIcon aria-hidden />
                                停用商品
                            </DropdownMenuItem>
                        </DropdownMenuContent>
                    </DropdownMenu>
                ) : null}
            </div>
            <div className="flex flex-wrap items-center gap-4 pb-2">
                <button
                    id="master-data-product-detail-media-shortcut"
                    type="button"
                    onClick={onMedia}
                    className="flex size-16 shrink-0 flex-col items-center justify-center gap-1 overflow-hidden rounded-xl border border-border bg-muted/40 text-muted-foreground transition-colors hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring"
                    aria-label={image ? "查看商品图片" : "添加商品图片"}
                >
                    {image ? (
                        <Image
                            src={image}
                            alt={title}
                            width={64}
                            height={64}
                            unoptimized
                            className="size-full object-cover"
                        />
                    ) : (
                        <>
                            <ImagePlusIcon className="size-5" aria-hidden />
                            <span className="text-xs">商品图片</span>
                        </>
                    )}
                </button>
                <div className="min-w-0 flex-1 basis-56 space-y-2">
                    <div className="flex flex-wrap items-center gap-2">
                        <h1 className="break-words text-xl font-semibold tracking-tight md:text-2xl">
                            {title}
                        </h1>
                        <Badge
                            variant={
                                data?.lifecycleStatus === "ENABLED"
                                    ? "success"
                                    : "secondary"
                            }
                        >
                            {isCreate ? "待创建" : data?.lifecycleStatusLabel}
                        </Badge>
                    </div>
                    <div className="flex flex-wrap gap-x-3 gap-y-1 text-sm text-muted-foreground">
                        <span>商品编号：{fields.productNo || "待填写"}</span>
                        {fields.brand ? <span>{fields.brand}</span> : null}
                        {fields.category ? (
                            <span>{fields.category}</span>
                        ) : null}
                        {fields.productKind ? (
                            <span>{productKindLabel(fields.productKind)}</span>
                        ) : null}
                        {fields.baseUnit ? (
                            <span>单位：{fields.baseUnit}</span>
                        ) : null}
                    </div>
                </div>
                <Button
                    id="master-data-product-detail-header-submit"
                    type="button"
                    disabled={!canRevise || pending}
                    onClick={onSave}
                >
                    <SaveIcon aria-hidden />
                    {pending ? "提交中…" : isCreate ? "创建商品" : "保存更新"}
                </Button>
            </div>
        </header>
    )
}
