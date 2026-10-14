"use client"

import Image from "next/image"
import { DetailPageHeader } from "@/components/business/detail-page-header"

import { BanIcon, ImagePlusIcon, SaveIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { productKindLabel } from "@/features/master-data/api/presentation"
import { masterDataCopy } from "@/features/master-data/lib/copy"
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
    /** 有未保存修改时提供；与保存并列放在页头，不另设底部保存条。 */
    onReset?: () => void
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
    onReset,
}: Props) {
    const image =
        fields.carouselPreviewUrls[fields.carouselImages[0]] ||
        fields.skus.find((sku) => sku.mainImagePreviewUrl)?.mainImagePreviewUrl
    return (
        <DetailPageHeader
            title={title}
            back={{
                id: "master-data-product-detail-back-list",
                label: "商品列表",
                onClick: onBack,
            }}
            secondaryActions={
                !isCreate && data ? (
                    <Button
                        id="master-data-product-detail-header-disable"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!canDisable || pending}
                        title={disableBlocker?.message}
                        onClick={() => setDisableOpen(true)}
                    >
                        <BanIcon data-icon="inline-start" aria-hidden />
                        {masterDataCopy.actionDisable}
                    </Button>
                ) : null
            }
            media={
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
            }
            titleExtra={
                <Badge
                    variant={
                        data?.lifecycleStatus === "ENABLED"
                            ? "success"
                            : "secondary"
                    }
                >
                    {isCreate ? "待创建" : data?.lifecycleStatusLabel}
                </Badge>
            }
            meta={
                <>
                    <span>
                        商品编号：{fields.productNo || "保存后自动生成"}
                    </span>
                    {fields.brand ? <span>{fields.brand}</span> : null}
                    {fields.category ? <span>{fields.category}</span> : null}
                    {fields.productKind ? (
                        <span>{productKindLabel(fields.productKind)}</span>
                    ) : null}
                    {fields.baseUnit ? (
                        <span>单位：{fields.baseUnit}</span>
                    ) : null}
                </>
            }
            primaryAction={
                <>
                    {onReset && canRevise && !isCreate ? (
                        <Button
                            id="master-data-product-detail-reset"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={pending}
                            onClick={onReset}
                        >
                            取消修改
                        </Button>
                    ) : null}
                    <LoadingButton
                        id="master-data-product-detail-header-submit"
                        loading={pending}
                        type="button"
                        size="sm"
                        disabled={!canRevise || pending}
                        onClick={onSave}
                    >
                        <SaveIcon data-icon="inline-start" aria-hidden />
                        {pending
                            ? "提交中…"
                            : isCreate
                              ? "创建商品"
                              : "保存更新"}
                    </LoadingButton>
                </>
            }
        />
    )
}
