"use client"

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
import { masterDataCopy } from "@/features/master-data/lib/copy"
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
        sku.skuId || sku.skuNo || sku.specificationSignature || `sku-${index}`,
    )
    const cellPad = "h-auto whitespace-normal align-top"

    return (
        <TableRow className="align-top">
            {activeSpecs.length > 0 ? (
                activeSpecs.map((spec, specIndex) => (
                    <TableCell
                        key={`${spec.name}-${specIndex}`}
                        className={cellPad}
                    >
                        <Badge variant="secondary">
                            {sku.attributeValues[specIndex] || "—"}
                        </Badge>
                    </TableCell>
                ))
            ) : (
                <TableCell className={cellPad}>
                    <Badge variant="secondary">
                        {masterDataCopy.productDefaultSpec}
                    </Badge>
                </TableCell>
            )}
            <TableCell className={cellPad}>
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
            </TableCell>
            <TableCell className={cellPad}>
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
                    placeholder={name.trim() || "请输入 SKU 名称"}
                    aria-label={`${sku.specLabel} SKU 名称`}
                    title="可与商品名称不同，保存后写入 SKU 修订"
                />
            </TableCell>
            <TableCell className={cellPad}>
                <Input
                    id={`master-data-product-sku-${skuSegment}-barcode`}
                    className="h-8"
                    value={sku.barcode ?? ""}
                    disabled={!canRevise}
                    onChange={(event) =>
                        updateSku(index, {
                            barcode: event.target.value || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 条形码`}
                />
            </TableCell>
            <TableCell className={cellPad}>
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
                                mainImagePreviewUrl: URL.createObjectURL(file),
                                mainImageAssetId: undefined,
                            })
                        }
                    }}
                />
            </TableCell>
            <TableCell className={cellPad}>
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
            <TableCell className={cellPad}>
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
            </TableCell>
            <TableCell className={cellPad}>
                <div className="flex items-center gap-2">
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
                className="min-w-[64rem] [&_thead_th]:!static"
            >
                <TableHeader>
                    <TableRow>
                        {activeSpecs.length > 0 ? (
                            <TableHead colSpan={activeSpecs.length}>
                                规格
                            </TableHead>
                        ) : (
                            <TableHead>规格</TableHead>
                        )}
                        <TableHead colSpan={4}>身份</TableHead>
                        <TableHead colSpan={2}>公司商品池价格</TableHead>
                        <TableHead colSpan={4}>关联与状态</TableHead>
                    </TableRow>
                    <TableRow>
                        {activeSpecs.length > 0 ? (
                            activeSpecs.map((spec) => (
                                <TableHead key={spec.name} className="min-w-24">
                                    {spec.name}
                                </TableHead>
                            ))
                        ) : (
                            <TableHead className="min-w-24">—</TableHead>
                        )}
                        <TableHead className="min-w-32">
                            {masterDataCopy.fProductCode}
                        </TableHead>
                        <TableHead className="min-w-40">
                            {masterDataCopy.fSkuName}
                        </TableHead>
                        <TableHead className="min-w-32">
                            {masterDataCopy.fBarcode}
                        </TableHead>
                        <TableHead className="min-w-36">
                            {masterDataCopy.fMainImage}
                        </TableHead>
                        <TableHead className="min-w-28">
                            {masterDataCopy.fSalePrice}
                        </TableHead>
                        <TableHead className="min-w-28">
                            {masterDataCopy.fMarketPrice}
                        </TableHead>
                        <TableHead className="min-w-32">供给</TableHead>
                        <TableHead className="min-w-28">库存</TableHead>
                        <TableHead className="min-w-24">上架</TableHead>
                        <TableHead className="min-w-24">启用</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {fields.skus.map((sku, index) => {
                        const supplierCount = sku.skuId
                            ? supplierCounts?.get(sku.skuId)
                            : 0
                        return (
                            <SkuRow
                                key={`${sku.skuNo}-${index}`}
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
