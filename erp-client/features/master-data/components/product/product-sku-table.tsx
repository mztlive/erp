"use client"

import { useChangeConfirmation } from "../../hooks/use-change-confirmation"

import * as React from "react"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    MoneyInput,
    SkuMainImageField,
} from "@/features/master-data/components/product/product-editor-media"
import { SkuSupplierCell } from "@/features/master-data/components/product/product-sku-supplier-cell"
import type {
    ProductFields,
    ProductSkuFields,
    ProductSpecDimension,
} from "@/features/master-data/types"
import type { FixedSku } from "@/features/supplier-offerings/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

type SkuRowProps = {
    sku: ProductSkuFields
    index: number
    isCreate: boolean
    canRevise: boolean
    name: string
    fields: ProductFields
    activeSpecs: readonly ProductSpecDimension[]
    updateSku: (index: number, patch: Partial<ProductSkuFields>) => void
    rememberSkuFile: (previewUrl: string, file: File) => void
    onOpenInventory: (
        skuId: string | undefined,
        trigger: HTMLButtonElement,
    ) => void
    supplierCount: number
    supplierCountsPending: boolean
    supplierCountsError: unknown
    onRegisterSupply: (sku: FixedSku) => void
    stableId: string
}

function SkuRow({
    sku,
    index,
    isCreate,
    canRevise,
    name,
    fields,
    activeSpecs,
    updateSku,
    rememberSkuFile,
    onOpenInventory,
    supplierCount,
    supplierCountsPending,
    supplierCountsError,
    onRegisterSupply,
    stableId,
}: SkuRowProps) {
    const skuSegment = toAutomationIdSegment(
        sku.skuId || sku.specificationSignature || sku.skuNo || `sku-${index}`,
    )
    const disableConfirmation = useChangeConfirmation(
        `master-data-product-sku-${skuSegment}-disable`,
    )
    const [detailsOpen, setDetailsOpen] = React.useState(false)
    const cellPad = "whitespace-normal align-middle"
    const skuName = sku.name || name || "未填写 SKU 名称"
    const specText = activeSpecs.length
        ? activeSpecs
              .map(
                  (spec, i) =>
                      `${spec.name}：${sku.attributeValues[i] || "未填写"}`,
              )
              .join(" · ")
        : "默认规格"
    const metaText = `${sku.skuNo || "待填写编码"} · ${specText}`
    return (
        <TableRow>
            <TableCell className="sticky left-0 z-10 min-w-64 max-w-80 whitespace-normal bg-card align-middle">
                <div className="flex items-center gap-3">
                    <div
                        className="size-14 shrink-0"
                        role="group"
                        aria-label={`${sku.name || sku.specLabel} 主图`}
                    >
                        <SkuMainImageField
                            idPrefix={`master-data-product-sku-${skuSegment}-main-image`}
                            value={sku.mainImage}
                            previewUrl={sku.mainImagePreviewUrl}
                            disabled={!canRevise}
                            onChange={(mainImage) =>
                                updateSku(
                                    index,
                                    mainImage
                                        ? {
                                              mainImage,
                                          }
                                        : {
                                              mainImage: "",
                                              mainImagePreviewUrl: undefined,
                                              mainImageAssetId: undefined,
                                          },
                                )
                            }
                            onFilesSelected={(files) => {
                                const file = files[0]
                                if (file) {
                                    const previewUrl = URL.createObjectURL(file)
                                    rememberSkuFile(previewUrl, file)
                                    updateSku(index, {
                                        mainImage: file.name,
                                        mainImagePreviewUrl: previewUrl,
                                        mainImageAssetId: undefined,
                                    })
                                }
                            }}
                        />
                    </div>
                    <div className="flex min-w-0 flex-1 items-center gap-2">
                        <div className="min-w-0">
                            <p
                                className="truncate text-sm font-medium leading-5"
                                title={skuName}
                            >
                                {skuName}
                            </p>
                            <p
                                className="mt-0.5 truncate text-xs leading-4 text-muted-foreground"
                                title={metaText}
                            >
                                {metaText}
                            </p>
                        </div>
                        <Button
                            id={`master-data-product-sku-${skuSegment}-edit`}
                            type="button"
                            variant="outline"
                            size="xs"
                            className="shrink-0"
                            onClick={() => setDetailsOpen(true)}
                        >
                            {canRevise ? "编辑资料" : "查看资料"}
                        </Button>
                    </div>
                </div>
                <Dialog open={detailsOpen} onOpenChange={setDetailsOpen}>
                    <DialogContent
                        className="max-h-[90dvh] overflow-y-auto sm:max-w-lg"
                        closeButtonId={`master-data-product-sku-${skuSegment}-close`}
                    >
                        <DialogHeader>
                            <DialogTitle>SKU 资料</DialogTitle>
                            <DialogDescription>
                                {canRevise
                                    ? "修改将在保存商品时生效。"
                                    : "SKU 基础资料。"}
                            </DialogDescription>
                        </DialogHeader>
                        <div className="space-y-4">
                            <div className="space-y-2">
                                <Label
                                    htmlFor={`master-data-product-sku-${skuSegment}-code`}
                                >
                                    SKU 编码
                                </Label>
                                <Input
                                    id={`master-data-product-sku-${skuSegment}-code`}
                                    className="h-8"
                                    value={sku.skuNo}
                                    disabled={!canRevise}
                                    onChange={(event) =>
                                        updateSku(index, {
                                            skuNo: event.target.value,
                                        })
                                    }
                                    aria-label={`${sku.specLabel} SKU 编码`}
                                    title={
                                        canRevise
                                            ? "系统默认生成，可手动覆盖"
                                            : undefined
                                    }
                                />
                            </div>
                            <div className="space-y-2">
                                <Label
                                    htmlFor={`master-data-product-sku-${skuSegment}-name`}
                                >
                                    SKU 名称
                                </Label>
                                <Input
                                    id={`master-data-product-sku-${skuSegment}-name`}
                                    className="h-8"
                                    value={sku.name}
                                    disabled={!canRevise}
                                    onChange={(event) =>
                                        updateSku(index, {
                                            name: event.target.value,
                                        })
                                    }
                                    placeholder={
                                        name.trim() || "请输入 SKU 名称"
                                    }
                                    aria-label={`${sku.specLabel} SKU 名称`}
                                    title={
                                        canRevise
                                            ? "可与商品名称不同"
                                            : undefined
                                    }
                                />
                            </div>
                            <div className="space-y-2">
                                <Label
                                    htmlFor={`master-data-product-sku-${skuSegment}-barcode`}
                                >
                                    条码
                                </Label>
                                <Input
                                    id={`master-data-product-sku-${skuSegment}-barcode`}
                                    className="h-8"
                                    value={sku.barcode ?? ""}
                                    disabled={!canRevise}
                                    onChange={(event) =>
                                        updateSku(index, {
                                            barcode:
                                                event.target.value || undefined,
                                        })
                                    }
                                    aria-label={`${sku.specLabel} 条形码`}
                                />
                            </div>
                        </div>
                        <DialogFooter>
                            <Button
                                id={`master-data-product-sku-${skuSegment}-done`}
                                type="button"
                                onClick={() => setDetailsOpen(false)}
                            >
                                {canRevise ? "完成编辑" : "关闭"}
                            </Button>
                        </DialogFooter>
                    </DialogContent>
                </Dialog>
            </TableCell>
            <TableCell className={`${cellPad} px-2 [&_input]:min-w-24`}>
                <MoneyInput
                    id={`master-data-product-sku-${skuSegment}-sale-price`}
                    value={sku.salePrice ?? ""}
                    disabled={!canRevise}
                    onChange={(next) =>
                        updateSku(index, {
                            salePrice: next || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 销售价`}
                />
            </TableCell>
            <TableCell className={`${cellPad} px-2 [&_input]:min-w-24`}>
                <MoneyInput
                    id={`master-data-product-sku-${skuSegment}-market-price`}
                    value={sku.marketPrice ?? ""}
                    disabled={!canRevise}
                    onChange={(next) =>
                        updateSku(index, {
                            marketPrice: next || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 市场价`}
                />
            </TableCell>
            <SkuSupplierCell
                sku={sku}
                name={name}
                fields={fields}
                isCreate={isCreate}
                canRevise={canRevise}
                stableId={stableId}
                supplierCount={supplierCount}
                supplierCountsPending={supplierCountsPending}
                supplierCountsError={supplierCountsError}
                onRegisterSupply={onRegisterSupply}
            />
            {fields.productKind === "PHYSICAL" ? (
                <TableCell className={cellPad}>
                    {sku.skuId ? (
                        <Button
                            id={`master-data-product-sku-${skuSegment}-inventory`}
                            type="button"
                            variant="link"
                            size="xs"
                            className="h-auto px-0 text-xs"
                            onClick={(event) =>
                                onOpenInventory(sku.skuId, event.currentTarget)
                            }
                        >
                            查看库存
                        </Button>
                    ) : (
                        <span className="block text-xs text-muted-foreground">
                            保存后可查看
                        </span>
                    )}
                </TableCell>
            ) : null}
            <TableCell className={cellPad}>
                <div className="flex flex-col items-start gap-1.5">
                    <div className="flex items-center gap-2">
                        <span className="whitespace-nowrap text-xs text-muted-foreground">
                            上架状态
                        </span>
                        <Badge
                            variant={
                                sku.listingStatus === "LISTED"
                                    ? "success"
                                    : "secondary"
                            }
                        >
                            {sku.listingStatus === "LISTED"
                                ? "已上架"
                                : "已下架"}
                        </Badge>
                    </div>
                    <div className="flex items-center gap-2">
                        <Label
                            htmlFor={`master-data-product-sku-${skuSegment}-enable`}
                            className="whitespace-nowrap text-xs font-normal text-muted-foreground"
                        >
                            SKU 启用
                        </Label>
                        <Switch
                            id={`master-data-product-sku-${skuSegment}-enable`}
                            size="sm"
                            disabled={!canRevise}
                            checked={sku.lifecycleStatus === "ENABLED"}
                            onCheckedChange={(checked) => {
                                if (!checked) {
                                    disableConfirmation.confirm({
                                        title: "停用 SKU",
                                        description:
                                            "保存商品后，新的业务单据将选不到此 SKU，历史单据不受影响。",
                                        details: [skuName, metaText],
                                        confirmLabel: "停用 SKU",
                                        destructive: true,
                                        onConfirm: () =>
                                            updateSku(index, {
                                                lifecycleStatus: "DISABLED",
                                            }),
                                    })
                                } else
                                    updateSku(index, {
                                        lifecycleStatus: "ENABLED",
                                    })
                            }}
                            aria-label={`${skuName} SKU 启用`}
                        />
                        <span className="text-xs text-muted-foreground">
                            {sku.lifecycleStatus === "ENABLED"
                                ? "启用"
                                : "停用"}
                        </span>
                    </div>
                </div>
                {disableConfirmation.dialog}
            </TableCell>
        </TableRow>
    )
}

type ProductSkuTableProps = {
    fields: ProductFields
    activeSpecs: readonly ProductSpecDimension[]
    isCreate: boolean
    canRevise: boolean
    name: string
    updateSku: (index: number, patch: Partial<ProductSkuFields>) => void
    rememberSkuFile: (previewUrl: string, file: File) => void
    onOpenInventory: (
        skuId: string | undefined,
        trigger: HTMLButtonElement,
    ) => void
    supplierCounts: Map<string, number> | undefined
    supplierCountsPending: boolean
    supplierCountsError: unknown
    onRegisterSupply: (sku: FixedSku) => void
    stableId: string
}

function ProductSkuTable({
    fields,
    activeSpecs,
    isCreate,
    canRevise,
    name,
    updateSku,
    rememberSkuFile,
    onOpenInventory,
    supplierCounts,
    supplierCountsPending,
    supplierCountsError,
    onRegisterSupply,
    stableId,
}: ProductSkuTableProps) {
    return (
        <div className="w-full max-w-full overflow-hidden rounded-lg border">
            <Table
                data-density="comfortable"
                className={
                    fields.productKind === "PHYSICAL"
                        ? "min-w-[52rem] [&_thead_th]:!static"
                        : "min-w-[44rem] [&_thead_th]:!static"
                }
            >
                <TableHeader>
                    <TableRow>
                        <TableHead className="min-w-64">SKU 信息</TableHead>
                        <TableHead className="w-32 min-w-28">销售价</TableHead>
                        <TableHead className="w-32 min-w-28">市场价</TableHead>
                        <TableHead className="w-36 min-w-28">供给</TableHead>
                        {fields.productKind === "PHYSICAL" ? (
                            <TableHead className="w-28 min-w-24">
                                库存
                            </TableHead>
                        ) : null}
                        <TableHead className="w-44 min-w-40">状态</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {fields.skus.map((sku, index) => {
                        const supplierCount = sku.skuId
                            ? supplierCounts?.get(sku.skuId)
                            : 0
                        return (
                            <SkuRow
                                key={
                                    sku.skuId ||
                                    sku.specificationSignature ||
                                    "default"
                                }
                                sku={sku}
                                index={index}
                                isCreate={isCreate}
                                canRevise={canRevise}
                                name={name}
                                fields={fields}
                                activeSpecs={activeSpecs}
                                updateSku={updateSku}
                                rememberSkuFile={rememberSkuFile}
                                onOpenInventory={onOpenInventory}
                                supplierCount={supplierCount ?? 0}
                                supplierCountsPending={supplierCountsPending}
                                supplierCountsError={supplierCountsError}
                                onRegisterSupply={onRegisterSupply}
                                stableId={stableId}
                            />
                        )
                    })}
                </TableBody>
            </Table>
        </div>
    )
}

export { ProductSkuTable }
