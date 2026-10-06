"use client"

import { Badge } from "@/components/ui/badge"
import type { PortalCategoryMappingSuggestion } from "@/features/supplier-portal/types"

const productKindLabels: Record<string, string> = {
    PHYSICAL: "实物",
    VIRTUAL: "虚拟商品",
    OFFLINE_SERVICE: "线下服务",
    VOUCHER: "卡券",
}

/** 先前确认记录只作为分类核对证据，不替代当前申请的人工匹配。 */
export function PortalCategoryMappingEvidence({
    suggestion,
}: {
    suggestion: PortalCategoryMappingSuggestion | null | undefined
}) {
    if (!suggestion)
        return (
            <section className="space-y-2 rounded-lg border p-4">
                <h3 className="text-sm font-medium">此前分类核对记录</h3>
                <p className="text-sm text-muted-foreground">
                    当前未读取到此前确认的分类记录，请按本次原始资料核对分类。
                </p>
            </section>
        )

    const requiresRecheck = suggestion.status === "recheck_required"
    const category = suggestion.category
    return (
        <section className="space-y-3 rounded-lg border p-4">
            <div className="flex flex-wrap items-center gap-2">
                <h3 className="text-sm font-medium">此前分类核对记录</h3>
                <Badge variant={requiresRecheck ? "warning" : "info"}>
                    {requiresRecheck ? "需重新核对" : "待本次人工确认"}
                </Badge>
            </div>
            <dl className="grid gap-x-4 gap-y-2 text-sm sm:grid-cols-[auto_1fr]">
                <dt className="text-muted-foreground">商品类型</dt>
                <dd>
                    {productKindLabels[suggestion.product_kind] ?? "待核对"}
                </dd>
                <dt className="text-muted-foreground">供应商原始完整路径</dt>
                <dd className="min-w-0 break-words">
                    {suggestion.original_category_path}
                </dd>
                <dt className="text-muted-foreground">先前确认分类路径</dt>
                <dd className="min-w-0 break-words">
                    {suggestion.confirmed_category_path}
                </dd>
                <dt className="text-muted-foreground">当前分类路径</dt>
                <dd className="min-w-0 break-words">
                    {category?.path || category?.name || "当前已无可选分类"}
                </dd>
            </dl>
            <p className="text-sm text-muted-foreground">
                {!category
                    ? "先前确认的分类当前不可选，需要重新匹配。本次审核仍须保留供应商原始路径。"
                    : requiresRecheck
                      ? "分类资料已需要重新核对，请比较先前确认路径与当前路径，并核对商品类型后重新匹配。"
                      : "此记录仅供本次核对参考；当前分类仍须由本次申请明确确认，供应商原始路径保持原值。"}
            </p>
        </section>
    )
}
