"use client"

import { CircleHelpIcon } from "lucide-react"

import { DetailHint } from "@/components/business/detail-presentation"
import { detailWorkspaceStyles } from "@/components/business/detail-workspace"
import { MoneyValue } from "@/components/business/values"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
import { StatusBadge } from "@/components/ui/status-badge"
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
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type {
    ProductFields,
    ProductSkuFields,
    ProductSpecDimension,
} from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useChangeConfirmation } from "../../hooks/use-change-confirmation"

export type SkuSupplySummary = Readonly<{
    activeSuppliers: number
    lowestDropshipPrice?: string
}>

type SkuRowProps = {
    sku: ProductSkuFields
    index: number
    showRowInventory: boolean
    showSupplyColumn: boolean
    supplySummary?: SkuSupplySummary
    supplyLoading?: boolean
    supplyError?: boolean
    canViewCosts?: boolean
    onOpenSupply?: (skuId: string) => void
    canRevise: boolean
    canEditLifecycle: boolean
    name: string
    fields: ProductFields
    activeSpecs: readonly ProductSpecDimension[]
    updateSku: (index: number, patch: Partial<ProductSkuFields>) => void
    rememberSkuFile: (previewUrl: string, file: File) => void
    onOpenInventory?: (
        skuId: string | undefined,
        trigger: HTMLButtonElement,
    ) => void
}

function SkuRow({
    sku,
    index,
    showRowInventory,
    showSupplyColumn,
    supplySummary,
    supplyLoading = false,
    supplyError = false,
    canViewCosts = false,
    onOpenSupply,
    canRevise,
    canEditLifecycle,
    name,
    fields,
    activeSpecs,
    updateSku,
    rememberSkuFile,
    onOpenInventory,
}: SkuRowProps) {
    const skuSegment = toAutomationIdSegment(
        sku.skuId || sku.specificationSignature || sku.skuNo || "default",
    )
    const disableConfirmation = useChangeConfirmation(
        `master-data-product-sku-${skuSegment}-disable`,
    )
    const cellPad = "whitespace-normal py-3 align-top"
    const skuName = sku.name || name || "未填写 SKU 名称"
    const specText = activeSpecs.length
        ? activeSpecs
              .map(
                  (spec, i) =>
                      `${spec.name}：${sku.attributeValues[i] || "未填写"}`,
              )
              .join(" · ")
        : "默认规格"
    return (
        <TableRow>
            <TableCell className="sticky left-0 z-10 whitespace-normal bg-card py-3 align-top transition-colors group-hover/row:bg-row-hover">
                <div className="flex items-start gap-2.5">
                    <div
                        className="w-12 shrink-0"
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
                    <div className="min-w-0 flex-1 space-y-1">
                        {canRevise ? (
                            <Input
                                id={`master-data-product-sku-${skuSegment}-name`}
                                className="h-control-sm"
                                value={sku.name}
                                onChange={(event) =>
                                    updateSku(index, {
                                        name: event.target.value,
                                    })
                                }
                                placeholder={name.trim() || "请输入 SKU 名称"}
                                aria-label={`${sku.specLabel} SKU 名称`}
                                title="可与商品名称不同"
                            />
                        ) : (
                            <p
                                className="truncate pt-1 font-medium leading-5 text-foreground"
                                title={skuName}
                            >
                                {skuName}
                            </p>
                        )}
                        <p
                            className="truncate pl-2.5 text-xs leading-4 text-muted-foreground"
                            title={specText}
                        >
                            {specText}
                        </p>
                    </div>
                </div>
            </TableCell>
            <TableCell className={cellPad}>
                <div className="flex min-w-0 flex-col gap-1">
                    {sku.skuNo ? (
                        <p
                            id={`master-data-product-sku-${skuSegment}-code`}
                            className="num truncate pl-2.5 text-xs text-muted-foreground"
                            title="SKU 编码：系统生成，不可修改"
                        >
                            {sku.skuNo}
                        </p>
                    ) : null}
                    <Input
                        id={`master-data-product-sku-${skuSegment}-barcode`}
                        className="h-control-sm num"
                        value={sku.barcode ?? ""}
                        disabled={!canRevise}
                        onChange={(event) =>
                            updateSku(index, {
                                barcode: event.target.value || undefined,
                            })
                        }
                        placeholder={canRevise ? "条码（可选）" : "无条码"}
                        aria-label={`${sku.specLabel} 条形码`}
                        title="条码"
                    />
                </div>
            </TableCell>
            <TableCell data-align="end" className={cellPad}>
                <MoneyInput
                    id={`master-data-product-sku-${skuSegment}-sale-price`}
                    value={sku.salePrice ?? ""}
                    disabled={!canRevise}
                    align="end"
                    onChange={(next) =>
                        updateSku(index, {
                            salePrice: next || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 一件代发价`}
                />
            </TableCell>
            <TableCell data-align="end" className={cellPad}>
                <MoneyInput
                    id={`master-data-product-sku-${skuSegment}-bulk-price`}
                    value={sku.bulkPriceGross ?? ""}
                    disabled={!canRevise}
                    align="end"
                    onChange={(next) =>
                        updateSku(index, { bulkPriceGross: next || undefined })
                    }
                    aria-label={`${sku.specLabel} 集采价`}
                />
            </TableCell>
            <TableCell data-align="end" className={cellPad}>
                <Input
                    id={`master-data-product-sku-${skuSegment}-bulk-min-quantity`}
                    className="h-control-sm num text-right"
                    inputMode="decimal"
                    value={sku.bulkMinQuantity ?? ""}
                    disabled={!canRevise}
                    onChange={(event) =>
                        updateSku(index, {
                            bulkMinQuantity: event.target.value || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 集采起订量`}
                />
            </TableCell>
            <TableCell data-align="end" className={cellPad}>
                <MoneyInput
                    id={`master-data-product-sku-${skuSegment}-market-price`}
                    value={sku.marketPrice ?? ""}
                    disabled={!canRevise}
                    align="end"
                    onChange={(next) =>
                        updateSku(index, {
                            marketPrice: next || undefined,
                        })
                    }
                    aria-label={`${sku.specLabel} 市场价`}
                />
            </TableCell>
            {showSupplyColumn ? (
                <TableCell className={cellPad}>
                    <div className="flex flex-col items-start gap-1 pt-0.5">
                        {!sku.skuId ? (
                            <span className="text-xs text-muted-foreground/75">
                                保存后可添
                            </span>
                        ) : supplyLoading ? (
                            <span className="text-xs text-muted-foreground">
                                读取中…
                            </span>
                        ) : supplyError ? (
                            <span className="text-xs text-muted-foreground">
                                暂不可查
                            </span>
                        ) : (
                            <>
                                <button
                                    id={`master-data-product-sku-${skuSegment}-supply-link`}
                                    type="button"
                                    onClick={() =>
                                        sku.skuId && onOpenSupply?.(sku.skuId)
                                    }
                                    className="inline-flex cursor-pointer items-center rounded-sm transition-opacity hover:opacity-80 focus-visible:outline-2 focus-visible:outline-primary"
                                    title="点击查看或添加该 SKU 的供应商供给"
                                >
                                    <StatusBadge
                                        tone={
                                            (supplySummary?.activeSuppliers ??
                                                0) > 0
                                                ? "success"
                                                : "warning"
                                        }
                                        appearance={
                                            (supplySummary?.activeSuppliers ??
                                                0) > 0
                                                ? "quiet"
                                                : "badge"
                                        }
                                        label={
                                            (supplySummary?.activeSuppliers ??
                                                0) > 0
                                                ? `${supplySummary?.activeSuppliers} 家供给`
                                                : "缺供给"
                                        }
                                    />
                                </button>
                                {canViewCosts &&
                                supplySummary?.lowestDropshipPrice ? (
                                    <span className="num text-xs text-muted-foreground">
                                        供货{" "}
                                        <MoneyValue
                                            value={
                                                supplySummary.lowestDropshipPrice
                                            }
                                            display="compact"
                                            className="text-muted-foreground"
                                        />
                                        起
                                    </span>
                                ) : null}
                            </>
                        )}
                    </div>
                </TableCell>
            ) : null}
            <TableCell className={cellPad}>
                <div className="flex flex-col items-start gap-2 pt-0.5">
                    <StatusBadge
                        tone={
                            sku.listingStatus === "LISTED"
                                ? "success"
                                : "neutral"
                        }
                        label={
                            sku.listingStatus === "LISTED" ? "已上架" : "已下架"
                        }
                    />
                    {canEditLifecycle ? (
                        <div className="flex items-center gap-1.5">
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
                                            details: [
                                                skuName,
                                                `${sku.skuNo || "新 SKU"} · ${specText}`,
                                            ],
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
                            <Label
                                htmlFor={`master-data-product-sku-${skuSegment}-enable`}
                                className="whitespace-nowrap text-xs font-normal text-muted-foreground"
                            >
                                {sku.lifecycleStatus === "ENABLED"
                                    ? "SKU 已启用"
                                    : "SKU 已停用"}
                            </Label>
                        </div>
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            {sku.lifecycleStatus === "ENABLED"
                                ? "SKU 已启用"
                                : "SKU 已停用"}
                        </span>
                    )}
                    {showRowInventory &&
                    fields.productKind === "PHYSICAL" &&
                    onOpenInventory ? (
                        sku.skuId ? (
                            <Button
                                id={`master-data-product-sku-${skuSegment}-inventory`}
                                type="button"
                                variant="link"
                                size="xs"
                                className="h-auto px-0 text-xs font-normal"
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
                            <span className="text-xs text-muted-foreground/75">
                                保存后可查看库存
                            </span>
                        )
                    ) : null}
                </div>
                {disableConfirmation.dialog}
            </TableCell>
        </TableRow>
    )
}

type ProductSkuTableProps = {
    fields: ProductFields
    activeSpecs: readonly ProductSpecDimension[]
    canRevise: boolean
    canEditLifecycle: boolean
    name: string
    showSupplyColumn?: boolean
    supplyBySku?: ReadonlyMap<string, SkuSupplySummary>
    supplyLoading?: boolean
    supplyError?: boolean
    canViewCosts?: boolean
    onOpenSupply?: (skuId: string) => void
    updateSku: (index: number, patch: Partial<ProductSkuFields>) => void
    rememberSkuFile: (previewUrl: string, file: File) => void
    onOpenInventory?: (
        skuId: string | undefined,
        trigger: HTMLButtonElement,
    ) => void
}

function ProductSkuTable({
    fields,
    activeSpecs,
    canRevise,
    canEditLifecycle,
    name,
    showSupplyColumn = false,
    supplyBySku,
    supplyLoading = false,
    supplyError = false,
    canViewCosts = false,
    onOpenSupply,
    updateSku,
    rememberSkuFile,
    onOpenInventory,
}: ProductSkuTableProps) {
    const showRowInventory = fields.skus.length > 1
    return (
        <div
            data-slot="detail-inline-table"
            className={detailWorkspaceStyles.inlineTableWrapper}
        >
            <Table
                data-density="comfortable"
                className="min-w-[62rem] table-fixed [--table-cell-inline:0.625rem] [&_thead_th]:!static"
            >
                <colgroup>
                    {showSupplyColumn ? (
                        <>
                            <col className="w-[27%]" />
                            <col className="w-[14.5%]" />
                            <col className="w-[9.5%]" />
                            <col className="w-[9.5%]" />
                            <col className="w-[8.5%]" />
                            <col className="w-[9.5%]" />
                            <col className="w-[10%]" />
                            <col className="w-[11.5%]" />
                        </>
                    ) : (
                        <>
                            <col className="w-[31%]" />
                            <col className="w-[16%]" />
                            <col className="w-[10.5%]" />
                            <col className="w-[10%]" />
                            <col className="w-[9.5%]" />
                            <col className="w-[10%]" />
                            <col className="w-[13%]" />
                        </>
                    )}
                </colgroup>
                <TableHeader>
                    <TableRow>
                        <TableHead>
                            SKU 名称与规格
                            <RequiredMark />
                        </TableHead>
                        <TableHead>
                            <span className="inline-flex items-center gap-1">
                                SKU 编码 / 条码
                                <DetailHint
                                    id="master-data-product-sku-code-hint"
                                    label="SKU 编码 / 条码"
                                    icon={
                                        <CircleHelpIcon
                                            className="size-3.5"
                                            aria-hidden="true"
                                        />
                                    }
                                >
                                    {masterDataCopy.productSkuCodeHint}
                                </DetailHint>
                            </span>
                        </TableHead>
                        <TableHead data-align="end">
                            <span className="inline-flex items-center justify-end gap-1">
                                代发价（含税）
                                <DetailHint
                                    id="master-data-product-sku-price-hint"
                                    label="价格"
                                    icon={
                                        <CircleHelpIcon
                                            className="size-3.5"
                                            aria-hidden="true"
                                        />
                                    }
                                >
                                    {masterDataCopy.productSkuPriceAudience}
                                </DetailHint>
                            </span>
                        </TableHead>
                        <TableHead data-align="end">集采价（含税）</TableHead>
                        <TableHead data-align="end">集采起订量</TableHead>
                        <TableHead data-align="end">市场价（含税）</TableHead>
                        {showSupplyColumn ? <TableHead>供给</TableHead> : null}
                        <TableHead>状态</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {fields.skus.map((sku, index) => (
                        <SkuRow
                            key={
                                sku.skuId ||
                                sku.specificationSignature ||
                                "default"
                            }
                            sku={sku}
                            index={index}
                            showRowInventory={showRowInventory}
                            showSupplyColumn={showSupplyColumn}
                            supplySummary={
                                sku.skuId
                                    ? supplyBySku?.get(sku.skuId)
                                    : undefined
                            }
                            supplyLoading={supplyLoading}
                            supplyError={supplyError}
                            canViewCosts={canViewCosts}
                            onOpenSupply={onOpenSupply}
                            canRevise={canRevise}
                            canEditLifecycle={canEditLifecycle}
                            name={name}
                            fields={fields}
                            activeSpecs={activeSpecs}
                            updateSku={updateSku}
                            rememberSkuFile={rememberSkuFile}
                            onOpenInventory={onOpenInventory}
                        />
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}

export { ProductSkuTable }
