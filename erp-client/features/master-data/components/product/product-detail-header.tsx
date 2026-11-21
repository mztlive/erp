"use client"

import Image from "next/image"
import { BanIcon, ImagePlusIcon, SaveIcon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
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
    hasUnsavedChanges?: boolean
    tabs?: React.ReactNode
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
    hasUnsavedChanges = false,
    tabs,
    onBack,
    onMedia,
    onSave,
}: Props) {
    const image =
        fields.carouselPreviewUrls[fields.carouselImages[0]] ||
        fields.skus.find((sku) => sku.mainImagePreviewUrl)?.mainImagePreviewUrl
    return (
        <DetailWorkspaceChrome
            title={title}
            back={{
                id: "master-data-product-detail-back-list",
                label: "商品列表",
                onClick: onBack,
            }}
            media={
                image || canRevise ? (
                    <button
                        id="master-data-product-detail-media-shortcut"
                        type="button"
                        onClick={onMedia}
                        className="flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-border/75 bg-muted/40 text-muted-foreground transition-colors hover:border-primary/45 hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring"
                        aria-label={image ? "查看商品图片" : "添加商品图片"}
                        title={image ? "查看图片与详情" : "添加商品图片"}
                    >
                        {image ? (
                            <Image
                                src={image}
                                alt={title}
                                width={44}
                                height={44}
                                unoptimized
                                className="size-full object-cover"
                            />
                        ) : (
                            <ImagePlusIcon className="size-4" aria-hidden />
                        )}
                    </button>
                ) : null
            }
            primaryStatus={{
                label: isCreate
                    ? "待创建"
                    : (data?.lifecycleStatusLabel ?? "当前停用"),
                tone: isCreate
                    ? "info"
                    : data?.lifecycleStatus === "ENABLED"
                      ? "success"
                      : "neutral",
            }}
            titleExtra={
                hasUnsavedChanges && canRevise && !isCreate ? (
                    <DetailWorkspaceDirtyBadge label="有未保存修改" />
                ) : null
            }
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="product-no" className="num">
                            商品编号：{fields.productNo || "保存后自动生成"}
                        </span>,
                        fields.brand,
                        fields.category,
                        fields.productKind
                            ? productKindLabel(fields.productKind)
                            : null,
                        fields.baseUnit ? `单位：${fields.baseUnit}` : null,
                    ]}
                />
            }
            secondaryActions={
                !isCreate && data ? (
                    <PermissionGuard permission="product:update">
                        <Button
                            id="master-data-product-detail-header-disable"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietDangerButton}
                            disabled={!canDisable || pending}
                            title={disableBlocker?.message}
                            onClick={() => setDisableOpen(true)}
                        >
                            <BanIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.actionDisable}
                        </Button>
                    </PermissionGuard>
                ) : null
            }
            primaryAction={
                isCreate ? (
                    <PermissionGuard permission="product:create">
                        <LoadingButton
                            id="master-data-product-detail-header-submit"
                            loading={pending}
                            type="button"
                            size="sm"
                            className="px-3.5"
                            disabled={!canRevise || pending}
                            onClick={onSave}
                        >
                            <SaveIcon data-icon="inline-start" aria-hidden />
                            {pending ? "提交中…" : "创建商品"}
                        </LoadingButton>
                    </PermissionGuard>
                ) : null
            }
            tabs={tabs}
        />
    )
}
