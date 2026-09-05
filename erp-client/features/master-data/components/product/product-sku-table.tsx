"use client"

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
    rememberSkuFile: (index: number, file?: File) => void
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
    const [detailsOpen, setDetailsOpen] = React.useState(false)
    const cellPad = "h-auto whitespace-normal align-top py-4"
    return (
        <TableRow className="align-top">
            <TableCell className="sticky left-0 z-10 min-w-64 max-w-80 whitespace-normal bg-card py-4 align-top">
                <div className="flex items-start gap-3">
                    <div
                        className="shrink-0"
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
                                rememberSkuFile(index, file)
                                if (file) {
                                    updateSku(index, {
                                        mainImage: file.name,
                                        mainImagePreviewUrl:
                                            URL.createObjectURL(file),
                                        mainImageAssetId: undefined,
                                    })
                                }
                            }}
                        />
                    </div>
                    <div className="min-w-0 flex-1">
                        <p className="break-words text-sm font-medium">
                            {sku.name || name || "未填写 SKU 名称"}
                        </p>
                        <p className="mt-1 break-all text-xs text-muted-foreground">
                            {sku.skuNo || "待填写编码"}
                        </p>
                        <p className="mt-1 break-words text-xs text-muted-foreground">
                            {activeSpecs.length
                                ? activeSpecs
                                      .map(
                                          (spec, i) =>
                                              `${spec.name}：${sku.attributeValues[i] || "未填写"}`,
                                      )
                                      .join(" · ")
                                : "默认规格"}
                        </p>
                        <Button
                            id={`master-data-product-sku-${skuSegment}-edit`}
                            type="button"
                            variant="link"
                            size="xs"
                            className="mt-2 h-auto px-0"
                            onClick={() => setDetailsOpen(true)}
                        >
                            {canRevise ? "编辑详情" : "查看详情"}
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
                                修改后返回商品页统一保存；关闭此窗口会保留本次编辑内容。
                            </DialogDescription>
                        </DialogHeader>
                        <div className="space-y-4">
                            <div className="space-y-2">
                                <Label
                                    htmlFor={`master-data-product-sku-${skuSegment}-code`}
                                >
                                    产品编码
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
                                    aria-label={`${sku.specLabel} 产品编码`}
                                    title="系统默认生成，可手动覆盖"
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
                                    title="可与商品名称不同，保存后写入 SKU 修订"
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
                                完成编辑
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
            <TableCell className={cellPad}>
                {fields.productKind && fields.productKind !== "PHYSICAL" ? (
                    <span className="block text-xs text-muted-foreground">
                        不适用
                    </span>
                ) : sku.skuId ? (
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
            <TableCell className={cellPad}>
                <Badge
                    variant={
                        sku.listingStatus === "LISTED" ? "success" : "secondary"
                    }
                >
                    {sku.listingStatus === "LISTED" ? "已上架" : "已下架"}
                </Badge>

                <div className="mt-2 flex items-center gap-2">
                    <Switch
                        id={`master-data-product-sku-${skuSegment}-enable`}
                        size="sm"
                        disabled={!canRevise}
                        checked={sku.lifecycleStatus === "ENABLED"}
                        onCheckedChange={(checked) => {
                            if (
                                !checked &&
                                !window.confirm(
                                    "停用该 SKU 后，新的业务单据将选不到它；历史单据不受影响。确定停用？",
                                )
                            ) {
                                return
                            }
                            updateSku(index, {
                                lifecycleStatus: checked
                                    ? "ENABLED"
                                    : "DISABLED",
                            })
                        }}
                        aria-label={`${sku.specLabel} SKU 状态`}
                    />
                    <span className="text-xs text-muted-foreground">
                        {sku.lifecycleStatus === "ENABLED" ? "启用" : "停用"}
                    </span>
                </div>
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
    rememberSkuFile: (index: number, file?: File) => void
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
                className="min-w-[48rem] [&_thead_th]:!static"
            >
                <TableHeader>
                    <TableRow>
                        <TableHead className="min-w-64">商品规格</TableHead>
                        <TableHead className="w-32 min-w-28">销售价</TableHead>
                        <TableHead className="w-32 min-w-28">市场价</TableHead>
                        <TableHead className="w-36 min-w-28">供给</TableHead>
                        <TableHead className="w-28 min-w-24">库存</TableHead>
                        <TableHead className="w-32 min-w-28">状态</TableHead>
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
