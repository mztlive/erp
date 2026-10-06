"use client"

import { Button } from "@/components/ui/button"
import type { PortalDictionary } from "@/features/supplier-portal/types"

export function PortalCategoryHierarchyConfirmation({
    category,
    expectedVersion,
    value,
    disabled,
    onConfirm,
}: {
    category: PortalDictionary | undefined
    expectedVersion: number
    value: NonNullable<PortalDictionary["hierarchy"]>
    disabled: boolean
    onConfirm: (value: NonNullable<PortalDictionary["hierarchy"]>) => void
}) {
    const hierarchy = category?.hierarchy ?? []
    const valid = hierarchy.length > 0 && category?.version === expectedVersion
    const confirmed =
        valid && JSON.stringify(value) === JSON.stringify(hierarchy)
    return (
        <div className="space-y-2 rounded-lg border p-3">
            <p className="text-sm">
                本次公司完整分类：
                {hierarchy.map((node) => node.name).join(" / ") || "待选择"}
            </p>
            <p className="text-xs text-muted-foreground">
                核对全部父级和商品类型；确认后父级改名、移动或停用均需重新核对。
            </p>
            {!valid && category && (
                <p className="text-sm">
                    所选分类版本已变化，或路径未完整读取，请重新核对；供应商已选资料变化须退回确认。
                </p>
            )}
            <Button
                id="supplier-portal-review-category-path-confirm"
                type="button"
                variant="outline"
                disabled={disabled || !valid}
                onClick={() =>
                    onConfirm(hierarchy.map((node) => ({ ...node })))
                }
            >
                {confirmed
                    ? "已核对以上完整分类路径"
                    : "核对并确认以上完整分类路径"}
            </Button>
        </div>
    )
}
