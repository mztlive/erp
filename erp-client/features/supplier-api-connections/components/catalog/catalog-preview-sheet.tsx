"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import { useState } from "react"
import Link from "next/link"
import { ArrowUpRightIcon, ImageOffIcon } from "lucide-react"
import { MoneyValue, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    catalogUnitLabel,
    type CatalogItem,
    type CatalogAvailability,
} from "../../api/catalog"
import type { useCatalogBatchSelection } from "../../hooks/use-catalog-batch-selection"
import {
    hasVerifiedCatalogImage,
    catalogMediaBlocker,
} from "../../lib/catalog-media"
import { CatalogSourceImage } from "./catalog-source-image"
import { SourcePurchaseTerms } from "./source-purchase-terms"
import {
    CatalogPreviewGallery,
    CatalogPreviewThumbnails,
    previewImages,
} from "./catalog-preview-gallery"

const availabilityLabels: Record<CatalogAvailability, string> = {
    AVAILABLE: "可供",
    UNAVAILABLE: "暂不可供",
    UNKNOWN: "待核验",
    WITHDRAWN: "已下架",
}

export function CatalogPreviewSheet({
    item,
    onClose,
    canReadCost,
    canConfigure,
    selectionBlocker,
    choice,
}: {
    item: CatalogItem
    onClose: () => void
    canReadCost: boolean
    canConfigure: boolean
    selectionBlocker?: string
    choice: ReturnType<typeof useCatalogBatchSelection>
}) {
    const [open, setOpen] = useState(true)
    const [expanded, setExpanded] = useState(false)
    const [detailsExpanded, setDetailsExpanded] = useState(false)
    const [activeImage, setActiveImage] = useState<string | null>(null)
    const images = previewImages(item)
    const mainImages = images.filter((entry) => !entry.detail)
    const details = images.filter((entry) => entry.detail)
    const cover = mainImages[0] ?? details[0]
    const selected = Boolean(choice.rowSelection[item.id])
    const blocker =
        selectionBlocker ??
        (choice.preparing
            ? "正在准备本次选择，请稍后继续。"
            : !choice.isFrozenMember(item.id)
              ? "该商品不在本次全选范围内，请清空选择后重新选择。"
              : choice.needsReload
                ? "请重新读取已保存的选择后继续。"
                : !canConfigure
                  ? "商品库读取完成后可选择商品。"
                  : undefined)
    const mediaWarning = catalogMediaBlocker(item)
    return (
        <QuickPreviewSheet
            idPrefix="supplier-catalog-preview"
            open={open}
            onOpenChange={setOpen}
            onOpenChangeComplete={(next) => {
                if (!next) onClose()
            }}
            size="preview"
            title={item.name}
            description={item.specification || undefined}
            headerMedia={
                cover ? (
                    <Button
                        id="supplier-catalog-preview-cover"
                        variant="outline"
                        className="size-full overflow-hidden p-0"
                        aria-label="放大查看商品主图"
                        disabled={!hasVerifiedCatalogImage(cover.image)}
                        onClick={() => setActiveImage(cover.key)}
                    >
                        <CatalogSourceImage
                            image={cover.image}
                            alt={item.name}
                            width={80}
                            height={80}
                            compact
                            className="size-full object-contain"
                        />
                    </Button>
                ) : (
                    <div
                        className="flex size-full items-center justify-center rounded-md bg-muted text-muted-foreground"
                        aria-label="暂无商品图片"
                    >
                        <ImageOffIcon aria-hidden="true" />
                    </div>
                )
            }
            summary={
                <div className="flex flex-wrap items-end justify-between gap-2">
                    <div>
                        <p className="text-xs text-muted-foreground">
                            接口采购报价
                        </p>
                        <MoneyValue
                            value={item.purchase_price}
                            unavailableReason={
                                !canReadCost
                                    ? "无采购报价查看权限"
                                    : item.purchase_price == null
                                      ? "暂无接口报价"
                                      : undefined
                            }
                            size="section"
                        />
                    </div>
                    <Badge
                        variant={
                            item.availability === "AVAILABLE"
                                ? "success"
                                : item.availability === "UNKNOWN"
                                  ? "warning"
                                  : "secondary"
                        }
                    >
                        {availabilityLabels[item.availability] ?? "待核验"}
                    </Badge>
                </div>
            }
            footer={
                <>
                    {blocker ? (
                        <PermissionGuard
                            permission={[
                                "supplier_api_connection:catalog_sync",
                                "supplier_offering:create",
                                "supplier_offering_cost:detail",
                            ]}
                        >
                            <p
                                id="supplier-catalog-preview-selection-blocker"
                                className="w-full text-xs text-muted-foreground"
                            >
                                {blocker}
                            </p>
                        </PermissionGuard>
                    ) : null}
                    <Button
                        id="supplier-catalog-preview-footer-close"
                        variant="outline"
                        onClick={() => setOpen(false)}
                    >
                        关闭
                    </Button>
                    {item.company_product_id ? (
                        <Button
                            id="supplier-catalog-preview-open-company"
                            variant="outline"
                            render={
                                <Link
                                    href={`/master-data/products/${encodeURIComponent(item.company_product_id)}`}
                                />
                            }
                        >
                            打开公司商品
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    ) : null}
                    <PermissionGuard
                        permission={[
                            "supplier_api_connection:catalog_sync",
                            "supplier_offering:create",
                            "supplier_offering_cost:detail",
                        ]}
                    >
                        <LoadingButton
                            id="supplier-catalog-preview-select"
                            variant={selected ? "outline" : "default"}
                            disabled={Boolean(blocker) || choice.busy}
                            loading={choice.saving}
                            aria-describedby={
                                blocker
                                    ? "supplier-catalog-preview-selection-blocker"
                                    : undefined
                            }
                            onClick={() =>
                                choice.change(
                                    {
                                        ...choice.rowSelection,
                                        [item.id]: !selected,
                                    },
                                    [item],
                                )
                            }
                        >
                            {selected ? "取消选择" : "加入本次选择"}
                        </LoadingButton>
                    </PermissionGuard>
                </>
            }
        >
            <div className="space-y-5">
                <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
                    {[
                        ["分类", item.category_name || "未提供"],
                        ["品牌", item.brand_name || "未提供"],
                        ["商品单位", catalogUnitLabel(item)],
                        [
                            "可供数量",
                            item.quantity_unbounded
                                ? "供应商未设数量上限"
                                : (item.available_quantity ?? "未提供"),
                        ],
                    ].map(([label, value]) => (
                        <div key={label} className="min-w-0 space-y-1">
                            <dt className="text-xs text-muted-foreground">
                                {label}
                            </dt>
                            <dd className="break-words">{value}</dd>
                        </div>
                    ))}
                    <div className="col-span-2 flex flex-wrap gap-x-2 gap-y-1 text-xs text-muted-foreground">
                        <dt>供应商订货编码</dt>
                        <dd className="num break-all">{item.spec_id}</dd>
                    </div>
                </dl>
                <SourcePurchaseTerms
                    terms={item.source_purchase_terms}
                    unit={item.unit}
                    canReadCost={canReadCost}
                />
                {choice.error ? (
                    <div role="alert" className="space-y-2 text-sm">
                        <p className="text-destructive">
                            选择更新失败：{choice.error.message}
                        </p>
                        {choice.frozen ? (
                            <Button
                                id="supplier-catalog-preview-selection-reload"
                                variant="outline"
                                size="sm"
                                disabled={choice.busy}
                                onClick={() => void choice.refreshFrozen()}
                            >
                                重新读取选择
                            </Button>
                        ) : null}
                    </div>
                ) : null}
                {mainImages.length ? (
                    <section className="space-y-2">
                        <h3 className="text-sm font-medium">
                            商品图片（{mainImages.length}）
                        </h3>
                        <CatalogPreviewThumbnails
                            images={mainImages}
                            onOpen={setActiveImage}
                        />
                    </section>
                ) : null}
                {details.length ? (
                    <details
                        className="space-y-2"
                        onToggle={(event) =>
                            setDetailsExpanded(event.currentTarget.open)
                        }
                    >
                        <summary
                            id="supplier-catalog-preview-details-toggle"
                            className="cursor-pointer text-sm font-medium"
                        >
                            商品详情图（{details.length}）
                        </summary>
                        {detailsExpanded ? (
                            <CatalogPreviewThumbnails
                                images={details}
                                onOpen={setActiveImage}
                            />
                        ) : null}
                    </details>
                ) : null}
                {mediaWarning ? (
                    <p className="text-xs text-muted-foreground">
                        {mediaWarning}
                    </p>
                ) : null}
                {item.description ? (
                    <section className="space-y-2">
                        <h3 className="text-sm font-medium">商品介绍</h3>
                        <p
                            id="supplier-catalog-preview-description"
                            className={
                                expanded
                                    ? "whitespace-pre-wrap break-words text-sm leading-6 text-muted-foreground"
                                    : "line-clamp-3 whitespace-pre-wrap break-words text-sm leading-6 text-muted-foreground"
                            }
                        >
                            {item.description}
                        </p>
                        <Button
                            id="supplier-catalog-preview-description-toggle"
                            variant="link"
                            size="sm"
                            className="h-auto px-0"
                            aria-expanded={expanded}
                            aria-controls="supplier-catalog-preview-description"
                            onClick={() => setExpanded(!expanded)}
                        >
                            {expanded ? "收起介绍" : "展开全文"}
                        </Button>
                    </section>
                ) : null}
            </div>
            <CatalogPreviewGallery
                images={images}
                activeKey={activeImage}
                onChange={setActiveImage}
                name={item.name}
            />
        </QuickPreviewSheet>
    )
}
