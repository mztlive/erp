"use client"

import Link from "next/link"
import {
    ArrowDownIcon,
    ArrowUpIcon,
    GripVerticalIcon,
    PlusIcon,
    XIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    HoverCard,
    HoverCardContent,
    HoverCardTrigger,
} from "@/components/ui/hover-card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { masterDataCopy } from "@/features/master-data/copy"
import {
    MoneyInput,
    moveListItem,
    SkuMainImageField,
} from "@/features/master-data/product-editor-media"
import type { ProductSpecDraft } from "@/features/master-data/product-editor-model"
import type { ProductInventoryPreviewSku } from "@/features/master-data/product-inventory-preview-sheet"
import type {
    ProductFields,
    ProductKind,
    ProductSkuFields,
    ProductSpecDimension,
} from "@/features/master-data/types"
import type { FixedSku } from "@/features/supplier-offerings/offering-dialogs"
import { getErrorMessage } from "@/lib/api/errors"
import { cn } from "@/lib/utils"

type ProductSkuSectionProps = {
    isCreate: boolean
    canRevise: boolean
    name: string
    fields: ProductFields
    specDrafts: readonly ProductSpecDraft[]
    activeSpecs: readonly ProductSpecDimension[]
    inventoryPreviewSkus: readonly ProductInventoryPreviewSku[]
    syncSpecDrafts: (next: readonly ProductSpecDraft[]) => void
    updateSku: (index: number, patch: Partial<ProductSkuFields>) => void
    batchSalePrice: string
    batchMarketPrice: string
    setBatchSalePrice: (next: string) => void
    setBatchMarketPrice: (next: string) => void
    onApplyBatchReferencePrices: () => void
    inventoryActionHint: string | undefined
    onOpenInventory: (
        skuId: string | undefined,
        trigger: HTMLButtonElement,
    ) => void
    rememberSkuFile: (index: number, file?: File) => void
    supplierCounts: Map<string, number> | undefined
    supplierCountsPending: boolean
    supplierCountsError: unknown
    onRegisterSupply: (sku: FixedSku) => void
    stableId: string
}

function ProductSkuSection({
    isCreate,
    canRevise,
    name,
    fields,
    specDrafts,
    activeSpecs,
    inventoryPreviewSkus,
    syncSpecDrafts,
    updateSku,
    batchSalePrice,
    batchMarketPrice,
    setBatchSalePrice,
    setBatchMarketPrice,
    onApplyBatchReferencePrices,
    inventoryActionHint,
    onOpenInventory,
    rememberSkuFile,
    supplierCounts,
    supplierCountsPending,
    supplierCountsError,
    onRegisterSupply,
    stableId,
}: ProductSkuSectionProps) {
    return (
        <>
            <fieldset
                id="product-section-sku"
                className={cn(
                    "scroll-mt-[var(--product-section-scroll-margin)] space-y-4 border-b border-border/70 p-5 last:border-b-0",
                )}
                disabled={!canRevise}
            >
                <legend className="sr-only">商品规格</legend>
                <div className="text-base font-semibold">商品规格</div>
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <p className="text-xs text-muted-foreground">
                        规格值会自动组合成 SKU；调整规格顺序时保留可匹配的原 SKU
                        数据。
                    </p>
                    <Badge variant="secondary">
                        {specDrafts.length} 个规格项 · {fields.skus.length} 个
                        SKU
                    </Badge>
                </div>
                <div className="space-y-3">
                    {specDrafts.map((draft, index) => (
                        <div
                            key={index}
                            className="rounded-xl border border-border bg-surface-sunken"
                        >
                            <div className="flex flex-wrap items-end gap-3 border-b border-border px-3 py-3">
                                <div className="flex items-center gap-2 self-center">
                                    <GripVerticalIcon
                                        className="size-4 text-muted-foreground"
                                        aria-hidden
                                    />
                                    <Badge variant="outline">
                                        规格项 {index + 1}
                                    </Badge>
                                </div>
                                <div className="min-w-48 flex-1 space-y-1.5 sm:max-w-sm">
                                    <Label
                                        htmlFor={`product-spec-name-${index}`}
                                        className="text-sm font-medium text-foreground"
                                    >
                                        规格名称
                                    </Label>
                                    <Input
                                        id={`product-spec-name-${index}`}
                                        className="bg-card font-medium shadow-sm"
                                        value={draft.name}
                                        onChange={(event) => {
                                            const next = [...specDrafts]
                                            next[index] = {
                                                ...draft,
                                                name: event.target.value,
                                            }
                                            syncSpecDrafts(next)
                                        }}
                                        placeholder="规格名称，如：颜色"
                                    />
                                </div>
                                <div className="ml-auto flex items-center gap-1">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        disabled={index === 0}
                                        aria-label={`规格项 ${index + 1} 上移`}
                                        onClick={() =>
                                            syncSpecDrafts(
                                                moveListItem(
                                                    specDrafts,
                                                    index,
                                                    index - 1,
                                                ),
                                            )
                                        }
                                    >
                                        <ArrowUpIcon />
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        disabled={
                                            index === specDrafts.length - 1
                                        }
                                        aria-label={`规格项 ${index + 1} 下移`}
                                        onClick={() =>
                                            syncSpecDrafts(
                                                moveListItem(
                                                    specDrafts,
                                                    index,
                                                    index + 1,
                                                ),
                                            )
                                        }
                                    >
                                        <ArrowDownIcon />
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        aria-label={`删除规格项 ${index + 1}`}
                                        onClick={() => {
                                            if (
                                                !window.confirm(
                                                    "删除规格项会移除对应组合生成的 SKU 行（含价格、主图、条码）。确定删除？",
                                                )
                                            ) {
                                                return
                                            }
                                            syncSpecDrafts(
                                                specDrafts.filter(
                                                    (_, i) => i !== index,
                                                ),
                                            )
                                        }}
                                    >
                                        <XIcon />
                                    </Button>
                                </div>
                            </div>
                            <div className="space-y-2 p-3">
                                <Label className="text-xs text-muted-foreground">
                                    规格值
                                </Label>
                                <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                                    {draft.values.map(
                                        (specValue, valueIndex) => (
                                            <div
                                                key={valueIndex}
                                                className="flex items-center gap-1"
                                            >
                                                <Input
                                                    className="h-8 bg-background"
                                                    value={specValue}
                                                    onChange={(event) => {
                                                        const nextValues = [
                                                            ...draft.values,
                                                        ]
                                                        nextValues[valueIndex] =
                                                            event.target.value
                                                        const next = [
                                                            ...specDrafts,
                                                        ]
                                                        next[index] = {
                                                            ...draft,
                                                            values: nextValues,
                                                        }
                                                        syncSpecDrafts(next)
                                                    }}
                                                    placeholder={`请输入${draft.name || "规格"}`}
                                                    aria-label={`${draft.name || `规格项 ${index + 1}`}的第 ${valueIndex + 1} 个值`}
                                                />
                                                <Button
                                                    type="button"
                                                    variant="ghost"
                                                    size="icon-xs"
                                                    aria-label={`删除规格值 ${specValue || valueIndex + 1}`}
                                                    onClick={() => {
                                                        if (
                                                            !window.confirm(
                                                                "删除规格取值会移除对应组合生成的 SKU 行（含价格、主图、条码）。确定删除？",
                                                            )
                                                        ) {
                                                            return
                                                        }
                                                        const next = [
                                                            ...specDrafts,
                                                        ]
                                                        next[index] = {
                                                            ...draft,
                                                            values: draft.values.filter(
                                                                (_, i) =>
                                                                    i !==
                                                                    valueIndex,
                                                            ),
                                                        }
                                                        syncSpecDrafts(next)
                                                    }}
                                                >
                                                    <XIcon />
                                                </Button>
                                            </div>
                                        ),
                                    )}
                                </div>
                                <Button
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    onClick={() => {
                                        const next = [...specDrafts]
                                        next[index] = {
                                            ...draft,
                                            values: [...draft.values, ""],
                                        }
                                        syncSpecDrafts(next)
                                    }}
                                >
                                    <PlusIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                    添加规格值
                                </Button>
                            </div>
                        </div>
                    ))}
                </div>
                <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() =>
                        syncSpecDrafts([
                            ...specDrafts,
                            { name: "", values: [""] },
                        ])
                    }
                >
                    <PlusIcon data-icon="inline-start" aria-hidden />
                    添加规格项
                </Button>
            </fieldset>

            <fieldset
                className={cn(
                    "min-w-0 max-w-full space-y-4 overflow-hidden border-b border-border/70 p-5 last:border-b-0",
                )}
            >
                <legend className="sr-only">SKU</legend>
                <div className="text-base font-semibold">SKU</div>
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="min-w-0 space-y-1">
                        <p className="text-xs text-muted-foreground">
                            {masterDataCopy.productSkuHint}
                        </p>
                    </div>
                    <Badge variant="success">
                        共 {fields.skus.length} 个 SKU
                    </Badge>
                </div>
                <div className="grid gap-2 rounded-xl border border-border bg-surface-sunken p-3 sm:grid-cols-2 lg:grid-cols-[repeat(2,minmax(0,1fr))_auto_auto]">
                    <div className="space-y-1">
                        <Label htmlFor="bulk-sale-price" className="text-xs">
                            批量销售价
                        </Label>
                        <Input
                            id="bulk-sale-price"
                            className="h-8 bg-background"
                            value={batchSalePrice}
                            disabled={!canRevise}
                            onChange={(event) =>
                                setBatchSalePrice(event.target.value)
                            }
                            placeholder="可选"
                        />
                    </div>
                    <div className="space-y-1">
                        <Label htmlFor="bulk-market-price" className="text-xs">
                            批量市场价
                        </Label>
                        <Input
                            id="bulk-market-price"
                            className="h-8 bg-background"
                            value={batchMarketPrice}
                            disabled={!canRevise}
                            onChange={(event) =>
                                setBatchMarketPrice(event.target.value)
                            }
                            placeholder="可选"
                        />
                    </div>
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        className="self-end"
                        disabled={
                            !canRevise ||
                            (!batchSalePrice.trim() && !batchMarketPrice.trim())
                        }
                        onClick={onApplyBatchReferencePrices}
                    >
                        批量设置
                    </Button>
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="self-end"
                        disabled={Boolean(inventoryActionHint)}
                        title={inventoryActionHint}
                        onClick={(event) =>
                            onOpenInventory(
                                inventoryPreviewSkus[0]?.skuId,
                                event.currentTarget,
                            )
                        }
                    >
                        查看商品库存
                    </Button>
                </div>
                {fields.skus.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        {masterDataCopy.productNoSkus}
                    </p>
                ) : (
                    <div className="w-full max-w-full overflow-x-auto overscroll-x-contain rounded-xl border border-border">
                        <table className="w-full min-w-[64rem] border-collapse text-sm">
                            <thead>
                                <tr className="border-b border-border text-left text-xs text-muted-foreground">
                                    {activeSpecs.length > 0 ? (
                                        <th
                                            colSpan={activeSpecs.length}
                                            className="px-3 py-2 font-medium"
                                        >
                                            规格
                                        </th>
                                    ) : (
                                        <th className="px-3 py-2 font-medium">
                                            规格
                                        </th>
                                    )}
                                    <th
                                        colSpan={4}
                                        className="px-3 py-2 font-medium"
                                    >
                                        身份
                                    </th>
                                    <th
                                        colSpan={2}
                                        className="px-3 py-2 font-medium"
                                    >
                                        公司商品池价格
                                    </th>
                                    <th
                                        colSpan={4}
                                        className="px-3 py-2 font-medium"
                                    >
                                        关联与状态
                                    </th>
                                </tr>
                                <tr className="border-b border-border text-left text-xs text-muted-foreground">
                                    {activeSpecs.length > 0 ? (
                                        activeSpecs.map((spec) => (
                                            <th
                                                key={spec.name}
                                                className="min-w-24 px-3 py-2 font-medium"
                                            >
                                                {spec.name}
                                            </th>
                                        ))
                                    ) : (
                                        <th className="min-w-24 px-3 py-2 font-medium">
                                            —
                                        </th>
                                    )}
                                    <th className="min-w-32 px-3 py-2 font-medium">
                                        {masterDataCopy.fProductCode}
                                    </th>
                                    <th className="min-w-40 px-3 py-2 font-medium">
                                        {masterDataCopy.fSkuName}
                                    </th>
                                    <th className="min-w-32 px-3 py-2 font-medium">
                                        {masterDataCopy.fBarcode}
                                    </th>
                                    <th className="min-w-36 px-3 py-2 font-medium">
                                        {masterDataCopy.fMainImage}
                                    </th>
                                    <th className="min-w-28 px-3 py-2 font-medium">
                                        {masterDataCopy.fSalePrice}
                                    </th>
                                    <th className="min-w-28 px-3 py-2 font-medium">
                                        {masterDataCopy.fMarketPrice}
                                    </th>
                                    <th className="min-w-32 px-3 py-2 font-medium">
                                        供给
                                    </th>
                                    <th className="min-w-28 px-3 py-2 font-medium">
                                        库存
                                    </th>
                                    <th className="min-w-24 px-3 py-2 font-medium">
                                        上架
                                    </th>
                                    <th className="min-w-24 px-3 py-2 font-medium">
                                        启用
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                {fields.skus.map((sku, index) => {
                                    const supplierCount = sku.skuId
                                        ? supplierCounts?.get(sku.skuId)
                                        : 0
                                    return (
                                        <tr
                                            key={`${sku.skuNo}-${index}`}
                                            className="border-b border-border/70 align-top last:border-b-0"
                                        >
                                            {activeSpecs.length > 0 ? (
                                                activeSpecs.map(
                                                    (spec, specIndex) => (
                                                        <td
                                                            key={`${spec.name}-${specIndex}`}
                                                            className="px-3 py-3"
                                                        >
                                                            <Badge variant="secondary">
                                                                {sku
                                                                    .attributeValues[
                                                                    specIndex
                                                                ] || "—"}
                                                            </Badge>
                                                        </td>
                                                    ),
                                                )
                                            ) : (
                                                <td className="px-3 py-3">
                                                    <Badge variant="secondary">
                                                        {
                                                            masterDataCopy.productDefaultSpec
                                                        }
                                                    </Badge>
                                                </td>
                                            )}
                                            <td className="px-3 py-3">
                                                <Input
                                                    className="h-8"
                                                    value={sku.skuNo}
                                                    disabled={!canRevise}
                                                    onChange={(event) =>
                                                        updateSku(index, {
                                                            skuNo: event.target
                                                                .value,
                                                        })
                                                    }
                                                    aria-label={`${sku.specLabel} 产品编码`}
                                                    title="系统默认生成，可手动覆盖"
                                                />
                                            </td>
                                            <td className="px-3 py-3">
                                                <Input
                                                    className="h-8"
                                                    value={sku.name}
                                                    disabled={!canRevise}
                                                    onChange={(event) =>
                                                        updateSku(index, {
                                                            name: event.target
                                                                .value,
                                                        })
                                                    }
                                                    placeholder={
                                                        name.trim() ||
                                                        "请输入 SKU 名称"
                                                    }
                                                    aria-label={`${sku.specLabel} SKU 名称`}
                                                    title="可与商品名称不同，保存后写入 SKU 修订"
                                                />
                                            </td>
                                            <td className="px-3 py-3">
                                                <Input
                                                    className="h-8"
                                                    value={sku.barcode ?? ""}
                                                    disabled={!canRevise}
                                                    onChange={(event) =>
                                                        updateSku(index, {
                                                            barcode:
                                                                event.target
                                                                    .value ||
                                                                undefined,
                                                        })
                                                    }
                                                    aria-label={`${sku.specLabel} 条形码`}
                                                />
                                            </td>
                                            <td className="px-3 py-3">
                                                <SkuMainImageField
                                                    value={sku.mainImage}
                                                    previewUrl={
                                                        sku.mainImagePreviewUrl
                                                    }
                                                    disabled={!canRevise}
                                                    onChange={(mainImage) =>
                                                        updateSku(
                                                            index,
                                                            mainImage
                                                                ? {
                                                                      mainImage,
                                                                  }
                                                                : {
                                                                      mainImage:
                                                                          "",
                                                                      mainImagePreviewUrl:
                                                                          undefined,
                                                                      mainImageAssetId:
                                                                          undefined,
                                                                  },
                                                        )
                                                    }
                                                    onFilesSelected={(
                                                        files,
                                                    ) => {
                                                        const file = files[0]
                                                        rememberSkuFile(
                                                            index,
                                                            file,
                                                        )
                                                        if (file) {
                                                            updateSku(index, {
                                                                mainImage:
                                                                    file.name,
                                                                mainImagePreviewUrl:
                                                                    URL.createObjectURL(
                                                                        file,
                                                                    ),
                                                                mainImageAssetId:
                                                                    undefined,
                                                            })
                                                        }
                                                    }}
                                                />
                                            </td>
                                            <td className="px-3 py-3">
                                                <MoneyInput
                                                    value={sku.salePrice ?? ""}
                                                    disabled={!canRevise}
                                                    onChange={(next) =>
                                                        updateSku(index, {
                                                            salePrice:
                                                                next ||
                                                                undefined,
                                                        })
                                                    }
                                                    aria-label={`${sku.specLabel} 销售价`}
                                                />
                                            </td>
                                            <td className="px-3 py-3">
                                                <MoneyInput
                                                    value={
                                                        sku.marketPrice ?? ""
                                                    }
                                                    disabled={!canRevise}
                                                    onChange={(next) =>
                                                        updateSku(index, {
                                                            marketPrice:
                                                                next ||
                                                                undefined,
                                                        })
                                                    }
                                                    aria-label={`${sku.specLabel} 市场价`}
                                                />
                                            </td>
                                            <td className="px-3 py-3">
                                                <div className="space-y-1.5">
                                                    {sku.skuId && !isCreate ? (
                                                        <HoverCard>
                                                            <HoverCardTrigger
                                                                render={
                                                                    <Badge
                                                                        variant="outline"
                                                                        className="cursor-pointer"
                                                                    />
                                                                }
                                                            >
                                                                {supplierCountsPending
                                                                    ? "…"
                                                                    : supplierCountsError !=
                                                                        null
                                                                      ? "供给暂不可查"
                                                                      : `${supplierCount ?? 0} 家供应商`}
                                                            </HoverCardTrigger>
                                                            <HoverCardContent
                                                                align="start"
                                                                className="w-64 space-y-3"
                                                            >
                                                                <div>
                                                                    <p className="text-sm font-medium">
                                                                        已启用供给关系
                                                                    </p>
                                                                    <p className="mt-2 text-sm text-muted-foreground">
                                                                        {supplierCountsError !=
                                                                        null
                                                                            ? getErrorMessage(
                                                                                  supplierCountsError,
                                                                                  "当前无法读取正式供给，请稍后重试。",
                                                                              )
                                                                            : `当前共有 ${supplierCount ?? 0} 家供应商具备已启用且已形成当前修订的供给关系；供应商及有效期明细以供给中心为准。`}
                                                                    </p>
                                                                </div>
                                                                <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
                                                                    <Button
                                                                        type="button"
                                                                        variant="outline"
                                                                        size="sm"
                                                                        disabled={
                                                                            !canRevise
                                                                        }
                                                                        onClick={() =>
                                                                            onRegisterSupply(
                                                                                {
                                                                                    skuId: sku.skuId!,
                                                                                    skuCode:
                                                                                        sku.skuNo,
                                                                                    skuName:
                                                                                        sku.name.trim() ||
                                                                                        name,
                                                                                    productKind:
                                                                                        fields.productKind as ProductKind,
                                                                                    specification:
                                                                                        sku.specLabel,
                                                                                    baseUnit:
                                                                                        sku.baseUnit ??
                                                                                        fields.baseUnit,
                                                                                    category:
                                                                                        fields.category ||
                                                                                        undefined,
                                                                                    brand:
                                                                                        fields.brand ||
                                                                                        undefined,
                                                                                    barcode:
                                                                                        sku.barcode,
                                                                                    description:
                                                                                        fields.description ||
                                                                                        undefined,
                                                                                    carouselImages:
                                                                                        fields.carouselImages,
                                                                                    detailImages:
                                                                                        fields.detailImages,
                                                                                    carouselFileAssetIds:
                                                                                        fields.carouselFileAssetIds,
                                                                                    detailFileAssetIds:
                                                                                        fields.detailFileAssetIds,
                                                                                    carouselPreviewUrls:
                                                                                        fields.carouselPreviewUrls,
                                                                                    detailPreviewUrls:
                                                                                        fields.detailPreviewUrls,
                                                                                    mainImage:
                                                                                        sku.mainImage ||
                                                                                        undefined,
                                                                                    mainImageAssetId:
                                                                                        sku.mainImageAssetId,
                                                                                    mainImagePreviewUrl:
                                                                                        sku.mainImagePreviewUrl,
                                                                                },
                                                                            )
                                                                        }
                                                                    >
                                                                        添加供给
                                                                    </Button>
                                                                    <Link
                                                                        className="text-xs text-primary hover:underline"
                                                                        href={`/procurement/supplier-offerings?skuId=${encodeURIComponent(sku.skuId)}&returnTo=${encodeURIComponent(`/master-data/products/${stableId}#product-section-sku`)}`}
                                                                    >
                                                                        查看全部供给
                                                                    </Link>
                                                                </div>
                                                            </HoverCardContent>
                                                        </HoverCard>
                                                    ) : (
                                                        <Badge variant="outline">
                                                            {supplierCountsPending
                                                                ? "…"
                                                                : supplierCountsError !=
                                                                    null
                                                                  ? "供给暂不可查"
                                                                  : `${supplierCount ?? 0} 家供应商`}
                                                        </Badge>
                                                    )}
                                                    {!sku.skuId || isCreate ? (
                                                        <span className="block text-xs text-muted-foreground">
                                                            保存商品后可添加多家供应商
                                                        </span>
                                                    ) : null}
                                                </div>
                                            </td>
                                            <td className="px-3 py-3">
                                                {fields.productKind &&
                                                fields.productKind !==
                                                    "PHYSICAL" ? (
                                                    <span className="block text-xs text-muted-foreground">
                                                        不适用
                                                    </span>
                                                ) : sku.skuId ? (
                                                    <Button
                                                        type="button"
                                                        variant="link"
                                                        size="xs"
                                                        className="h-auto px-0 text-xs"
                                                        onClick={(event) =>
                                                            onOpenInventory(
                                                                sku.skuId,
                                                                event.currentTarget,
                                                            )
                                                        }
                                                    >
                                                        查看库存
                                                    </Button>
                                                ) : (
                                                    <span className="block text-xs text-muted-foreground">
                                                        保存后可查看
                                                    </span>
                                                )}
                                            </td>
                                            <td className="px-3 py-3">
                                                <Badge
                                                    variant={
                                                        sku.listingStatus ===
                                                        "LISTED"
                                                            ? "success"
                                                            : "secondary"
                                                    }
                                                >
                                                    {sku.listingStatus ===
                                                    "LISTED"
                                                        ? "已上架"
                                                        : "已下架"}
                                                </Badge>
                                            </td>
                                            <td className="px-3 py-3">
                                                <div className="flex items-center gap-2">
                                                    <Switch
                                                        size="sm"
                                                        disabled={!canRevise}
                                                        checked={
                                                            sku.lifecycleStatus ===
                                                            "ENABLED"
                                                        }
                                                        onCheckedChange={(
                                                            checked,
                                                        ) => {
                                                            if (
                                                                !checked &&
                                                                !window.confirm(
                                                                    "停用该 SKU 后，新的业务单据将选不到它；历史单据不受影响。确定停用？",
                                                                )
                                                            ) {
                                                                return
                                                            }
                                                            updateSku(index, {
                                                                lifecycleStatus:
                                                                    checked
                                                                        ? "ENABLED"
                                                                        : "DISABLED",
                                                            })
                                                        }}
                                                        aria-label={`${sku.specLabel} SKU 状态`}
                                                    />
                                                    <span className="text-xs text-muted-foreground">
                                                        {sku.lifecycleStatus ===
                                                        "ENABLED"
                                                            ? "启用"
                                                            : "停用"}
                                                    </span>
                                                </div>
                                            </td>
                                        </tr>
                                    )
                                })}
                            </tbody>
                        </table>
                    </div>
                )}
            </fieldset>
        </>
    )
}

export { ProductSkuSection }
