"use client"

import Link from "next/link"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { TableCell } from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { toFixedSku } from "@/features/master-data/lib/product-fixed-sku"
import type {
    ProductFields,
    ProductSkuFields,
} from "@/features/master-data/types"
import type { FixedSku } from "@/features/supplier-offerings/types"
import { getErrorMessage } from "@/lib/api/errors"

type SkuSupplierCellProps = {
    sku: ProductSkuFields
    name: string
    fields: ProductFields
    isCreate: boolean
    canRevise: boolean
    stableId: string
    supplierCount: number
    supplierCountsPending: boolean
    supplierCountsError: unknown
    onRegisterSupply: (sku: FixedSku) => void
}

function SkuSupplierCell({
    sku,
    name,
    fields,
    isCreate,
    canRevise,
    stableId,
    supplierCount,
    supplierCountsPending,
    supplierCountsError,
    onRegisterSupply,
}: SkuSupplierCellProps) {
    const skuSegment = toAutomationIdSegment(sku.skuId || sku.skuNo || name)
    return (
        <TableCell className="whitespace-normal align-middle">
            <div className="space-y-1.5">
                <Badge
                    variant="outline"
                    title={
                        supplierCountsError
                            ? getErrorMessage(
                                  supplierCountsError,
                                  "供给读取失败，请稍后重试。",
                              )
                            : "当前有效的已启用供给关系"
                    }
                >
                    {supplierCountsPending
                        ? "读取中…"
                        : supplierCountsError != null
                          ? "供给暂不可查"
                          : `${supplierCount ?? 0} 家供应商`}
                </Badge>
                {sku.skuId && !isCreate ? (
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        <Button
                            id={`master-data-product-sku-${skuSegment}-add-supply`}
                            type="button"
                            variant="link"
                            size="xs"
                            className="h-7 px-0"
                            disabled={!canRevise}
                            onClick={() =>
                                onRegisterSupply(toFixedSku(fields, sku, name))
                            }
                        >
                            添加供给
                        </Button>
                        <Link
                            id={`master-data-product-sku-${skuSegment}-view-supplies`}
                            className="text-xs text-muted-foreground hover:text-foreground hover:underline"
                            target="_blank"
                            rel="noopener noreferrer"
                            aria-label={`${name}查看全部供给（新窗口）`}
                            href={`/procurement/supplier-offerings?skuId=${encodeURIComponent(sku.skuId)}&returnTo=${encodeURIComponent(`/master-data/products/${stableId}#product-section-sku`)}`}
                        >
                            查看供给
                        </Link>
                    </div>
                ) : (
                    <span className="block text-xs text-muted-foreground">
                        保存商品后可添加多家供应商
                    </span>
                )}
            </div>
        </TableCell>
    )
}

export { SkuSupplierCell }
