"use client"

import Link from "next/link"

import {
    HoverCard,
    HoverCardContent,
    HoverCardTrigger,
} from "@/components/ui/hover-card"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
    return (
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
                                : supplierCountsError != null
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
                                    {supplierCountsError != null
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
                                    disabled={!canRevise}
                                    onClick={() =>
                                        onRegisterSupply(
                                            toFixedSku(fields, sku, name),
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
                            : supplierCountsError != null
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
    )
}

export { SkuSupplierCell }
