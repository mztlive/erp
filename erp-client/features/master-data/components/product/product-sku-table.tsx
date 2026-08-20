"use client"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
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
    onOpenInventory: (skuId: string | undefined, trigger: HTMLButtonElement) => void
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
    return (
        <tr
            key={`${sku.skuNo}-${index}`}
            className="border-b border-grid align-top last:border-b-0"
        >
            {activeSpecs.length > 0 ? (
                activeSpecs.map((spec, specIndex) => (
                    <td
                        key={`${spec.name}-${specIndex}`}
                        className="px-3 py-3"
                    >
                        <Badge variant="secondary">
                            {sku.attributeValues[specIndex] || "—"}
                        </Badge>
                    </td>
                ))
            ) : (
                <td className="px-3 py-3">
                    <Badge variant="secondary">
                        {masterDataCopy.productDefaultSpec}
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
                            skuNo: event.target.value,
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
                            name: event.target.value,
                        })
                    }
                    placeholder={name.trim() || "请输入 SKU 名称"}
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
                            barcode: event.target.value || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 条形码`}
                />
            </td>
            <td className="px-3 py-3">
                <SkuMainImageField
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
            </td>
            <td className="px-3 py-3">
                <MoneyInput
                    value={sku.salePrice ?? ""}
                    disabled={!canRevise}
                    onChange={(next) =>
                        updateSku(index, {
                            salePrice: next || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 销售价`}
                />
            </td>
            <td className="px-3 py-3">
                <MoneyInput
                    value={sku.marketPrice ?? ""}
                    disabled={!canRevise}
                    onChange={(next) =>
                        updateSku(index, {
                            marketPrice: next || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 市场价`}
                />
            </td>
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
            <td className="px-3 py-3">
                {fields.productKind && fields.productKind !== "PHYSICAL" ? (
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
            </td>
            <td className="px-3 py-3">
                <Badge
                    variant={
                        sku.listingStatus === "LISTED" ? "success" : "secondary"
                    }
                >
                    {sku.listingStatus === "LISTED" ? "已上架" : "已下架"}
                </Badge>
            </td>
            <td className="px-3 py-3">
                <div className="flex items-center gap-2">
                    <Switch
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
            </td>
        </tr>
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
    onOpenInventory: (skuId: string | undefined, trigger: HTMLButtonElement) => void
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
                            <th className="px-3 py-2 font-medium">规格</th>
                        )}
                        <th colSpan={4} className="px-3 py-2 font-medium">
                            身份
                        </th>
                        <th colSpan={2} className="px-3 py-2 font-medium">
                            公司商品池价格
                        </th>
                        <th colSpan={4} className="px-3 py-2 font-medium">
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
                </tbody>
            </table>
        </div>
    )
}

export { ProductSkuTable }
