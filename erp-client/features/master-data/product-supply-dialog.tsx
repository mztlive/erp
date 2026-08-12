"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type {
    MasterDataListItem,
    ProductListSkuSummary,
} from "@/features/master-data/types"
import {
    AVAILABILITY_STATUS_LABELS,
    type SupplierOfferingView,
} from "@/features/supplier-offerings/types"

function money(value?: string | null): string {
    return value ? `¥${value}` : "未提供"
}

function isCurrentSupply(offering: SupplierOfferingView): boolean {
    return offering.status === "ACTIVE" && Boolean(offering.current_revision_id)
}

export function ProductSupplyDialog({
    product,
    skus,
    skuLoading,
    skuError,
    offerings,
    offeringLoading,
    offeringError,
    onRetrySkus,
    onRetryOfferings,
    onAddSupply,
    onOpenChange,
}: {
    product: MasterDataListItem | null
    skus: readonly ProductListSkuSummary[]
    skuLoading: boolean
    skuError: unknown
    offerings: readonly SupplierOfferingView[]
    offeringLoading: boolean
    offeringError: unknown
    onRetrySkus: () => void
    onRetryOfferings: () => void
    onAddSupply: (sku: ProductListSkuSummary) => void
    onOpenChange: (open: boolean) => void
}) {
    const currentOfferingsBySku = React.useMemo(() => {
        const grouped = new Map<string, SupplierOfferingView[]>()
        for (const offering of offerings) {
            if (!isCurrentSupply(offering)) continue
            const rows = grouped.get(offering.sku_id) ?? []
            rows.push(offering)
            grouped.set(offering.sku_id, rows)
        }
        return grouped
    }, [offerings])

    return (
        <Dialog open={product != null} onOpenChange={onOpenChange}>
            <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-5xl">
                <DialogHeader>
                    <DialogTitle>
                        {product ? `${product.name} · 供给` : "商品供给"}
                    </DialogTitle>
                    <DialogDescription>
                        {product
                            ? `${product.stableNo} · 按 SKU 查看当前启用的供应商与含税供给价。`
                            : "按 SKU 查看当前启用的供应商与含税供给价。"}
                    </DialogDescription>
                </DialogHeader>

                {skuError ? (
                    <BusinessFailureState
                        title="SKU 信息加载失败"
                        error={skuError}
                        onRetry={onRetrySkus}
                    />
                ) : skuLoading ? (
                    <div className="rounded-lg border bg-muted/30 px-4 py-8 text-center text-sm text-muted-foreground">
                        正在读取 SKU…
                    </div>
                ) : skus.length === 0 ? (
                    <div className="rounded-lg border bg-muted/30 px-4 py-8 text-center">
                        <p className="text-sm font-medium">
                            该商品没有启用中的 SKU
                        </p>
                        <p className="mt-1 text-sm text-muted-foreground">
                            请先进入商品详情新增或启用 SKU，再添加供给。
                        </p>
                    </div>
                ) : (
                    <div className="space-y-4">
                        {offeringError ? (
                            <BusinessFailureState
                                title="供给信息加载失败"
                                error={offeringError}
                                onRetry={onRetryOfferings}
                            />
                        ) : null}

                        {skus.map((sku) => {
                            const skuOfferings =
                                currentOfferingsBySku.get(sku.skuId) ?? []
                            return (
                                <section
                                    key={sku.skuId}
                                    className="overflow-hidden rounded-lg border"
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-3 border-b bg-muted/30 px-4 py-3">
                                        <div className="min-w-0">
                                            <div className="flex flex-wrap items-center gap-2">
                                                <span className="font-medium">
                                                    {sku.skuName || sku.skuNo}
                                                </span>
                                                <Badge variant="secondary">
                                                    {sku.specification}
                                                </Badge>
                                                {!offeringLoading &&
                                                !offeringError ? (
                                                    <Badge
                                                        variant={
                                                            skuOfferings.length >
                                                            0
                                                                ? "success"
                                                                : "outline"
                                                        }
                                                    >
                                                        {skuOfferings.length > 0
                                                            ? `${skuOfferings.length} 条供给`
                                                            : "无供给"}
                                                    </Badge>
                                                ) : null}
                                            </div>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                {sku.skuNo} · 销售价{" "}
                                                {money(
                                                    sku.salesVisiblePriceGross,
                                                )}
                                            </p>
                                        </div>
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            onClick={() => onAddSupply(sku)}
                                        >
                                            <PlusIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            添加供给
                                        </Button>
                                    </div>

                                    {offeringLoading ? (
                                        <div className="px-4 py-6 text-sm text-muted-foreground">
                                            正在读取供给…
                                        </div>
                                    ) : offeringError ? (
                                        <div className="px-4 py-6 text-sm text-muted-foreground">
                                            当前无法判断该 SKU 是否存在供给。
                                        </div>
                                    ) : skuOfferings.length === 0 ? (
                                        <div className="px-4 py-6 text-sm text-muted-foreground">
                                            该 SKU
                                            暂无启用中的供给关系，可点击“添加供给”登记。
                                        </div>
                                    ) : (
                                        <Table>
                                            <TableHeader>
                                                <TableRow>
                                                    <TableHead>
                                                        供应商
                                                    </TableHead>
                                                    <TableHead>
                                                        供应商 SKU
                                                    </TableHead>
                                                    <TableHead>
                                                        一件代发价
                                                    </TableHead>
                                                    <TableHead>
                                                        集采价
                                                    </TableHead>
                                                    <TableHead>
                                                        当前可供
                                                    </TableHead>
                                                </TableRow>
                                            </TableHeader>
                                            <TableBody>
                                                {skuOfferings.map(
                                                    (offering) => (
                                                        <TableRow
                                                            key={offering.id}
                                                        >
                                                            <TableCell>
                                                                <div className="font-medium">
                                                                    {offering.supplier_name ??
                                                                        offering.supplier_no ??
                                                                        "供应商名称未返回"}
                                                                </div>
                                                                {offering.supplier_no &&
                                                                offering.supplier_name ? (
                                                                    <div className="mt-1 text-xs text-muted-foreground">
                                                                        {
                                                                            offering.supplier_no
                                                                        }
                                                                    </div>
                                                                ) : null}
                                                            </TableCell>
                                                            <TableCell>
                                                                {
                                                                    offering.supplier_sku_code
                                                                }
                                                            </TableCell>
                                                            <TableCell>
                                                                {money(
                                                                    offering.dropship_supply_price_gross,
                                                                )}
                                                            </TableCell>
                                                            <TableCell>
                                                                {money(
                                                                    offering.bulk_supply_price_gross,
                                                                )}
                                                            </TableCell>
                                                            <TableCell>
                                                                <Badge
                                                                    variant={
                                                                        offering.availability_status ===
                                                                        "AVAILABLE"
                                                                            ? "success"
                                                                            : "outline"
                                                                    }
                                                                >
                                                                    {offering.availability_status
                                                                        ? AVAILABILITY_STATUS_LABELS[
                                                                              offering
                                                                                  .availability_status
                                                                          ]
                                                                        : "未更新"}
                                                                </Badge>
                                                                <div className="mt-1 text-xs text-muted-foreground">
                                                                    数量{" "}
                                                                    {offering.available_quantity ??
                                                                        "未提供"}
                                                                </div>
                                                            </TableCell>
                                                        </TableRow>
                                                    ),
                                                )}
                                            </TableBody>
                                        </Table>
                                    )}
                                </section>
                            )
                        })}
                    </div>
                )}

                <DialogFooter>
                    <DialogClose
                        render={<Button type="button" variant="outline" />}
                    >
                        关闭
                    </DialogClose>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
