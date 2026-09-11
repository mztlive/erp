"use client"

import { useState } from "react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { TierEditor } from "./tier-editor"
import { useBookOperations } from "../hooks/queries"
import { createIdempotencyKey, tierRuleSchema } from "../lib/validation"
import type { SelectionBookDetail, PoolFilterSnapshot } from "../types"

/** 在固定来源类型内编辑来源和档位；成功准备后才替换原规则。 */
export const ReprepareDialog = ({
    detail,
    onClose,
}: {
    detail: SelectionBookDetail
    onClose: () => void
}) => {
    const operations = useBookOperations()
    const [error, setError] = useState("")
    const form = useAppForm({
        defaultValues: {
            q: detail.pool_filter?.q ?? "",
            skuText: detail.sku_ids?.join("\n") ?? "",
            tiers: detail.tiers,
        },
        onSubmit: async ({ value }) => {
            setError("")
            const tiers = value.tiers.map((tier) =>
                tierRuleSchema.safeParse(tier),
            )
            const invalid = tiers.find((result) => !result.success)
            if (invalid && !invalid.success) {
                setError(invalid.error.issues[0].message)
                return
            }
            if (
                detail.form === "PACKAGE" &&
                (tiers.length < 1 || tiers.length > 10)
            ) {
                setError("请设置 1 到 10 个档位")
                return
            }
            const skuIds = [
                ...new Set(value.skuText.split(/[\s,，、]+/).filter(Boolean)),
            ]
            if (
                detail.source_kind === "SELECTION" &&
                (skuIds.length < 1 || skuIds.length > 500)
            ) {
                setError("请选择 1 到 500 个商品")
                return
            }
            try {
                await operations.prepare.mutateAsync({
                    bookId: detail.id,
                    kind:
                        detail.status === "DRAFT"
                            ? "FIRST_PREPARE"
                            : "RE_PREPARE",
                    filter:
                        detail.source_kind === "FILTER"
                            ? {
                                  ...detail.pool_filter,
                                  q: value.q.trim() || undefined,
                              }
                            : undefined,
                    sku_ids:
                        detail.source_kind === "SELECTION" ? skuIds : undefined,
                    tiers: value.tiers,
                    expected_version: detail.version,
                    idempotency_key: createIdempotencyKey(),
                })
                onClose()
            } catch {
                setError("准备任务未确认启动，请关闭弹窗刷新选品册状态后重试。")
            }
        },
    })
    const filters = Object.entries(detail.pool_filter ?? {}).filter(
        ([key, value]) => key !== "q" && value != null && value !== false,
    )
    const labels: Record<keyof PoolFilterSnapshot, string> = {
        q: "关键字",
        product_kind: "商品类型",
        category_id: "分类",
        brand_id: "品牌",
        supplier_id: "供应商",
        supply_region: "供货区域",
        max_supplier_count: "供应商数量上限",
        sales_price_min: "最低售价",
        sales_price_max: "最高售价",
        nationwide_only: "全国可供",
    }
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !operations.prepare.isPending) onClose()
            }}
        >
            <DialogContent
                closeButtonId="selection-reprepare-close"
                className="max-h-[90svh] overflow-y-auto sm:max-w-2xl"
            >
                <DialogHeader>
                    <DialogTitle>调整来源与档位并准备</DialogTitle>
                    <DialogDescription>
                        成功后整体替换当前陈列；失败时保留原商品池、规则和陈列。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="grid gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <fieldset
                        disabled={operations.prepare.isPending}
                        className="grid gap-4"
                    >
                        {detail.source_kind === "FILTER" ? (
                            <>
                                <form.Field name="q">
                                    {(field) => (
                                        <div className="grid gap-2">
                                            <Label htmlFor="selection-reprepare-q">
                                                筛选关键字
                                            </Label>
                                            <Input
                                                id="selection-reprepare-q"
                                                value={field.state.value}
                                                onChange={(e) =>
                                                    field.handleChange(
                                                        e.target.value,
                                                    )
                                                }
                                            />
                                            <p className="text-sm text-muted-foreground">
                                                留空表示不限制关键字，其他来源条件继续生效。
                                            </p>
                                        </div>
                                    )}
                                </form.Field>
                                {filters.length > 0 && (
                                    <p className="text-sm text-muted-foreground">
                                        保留的来源条件：
                                        {filters
                                            .map(
                                                ([key, value]) =>
                                                    `${labels[key as keyof PoolFilterSnapshot]}：${value === true ? "是" : value}`,
                                            )
                                            .join("；")}
                                    </p>
                                )}
                            </>
                        ) : (
                            <form.Field name="skuText">
                                {(field) => (
                                    <div className="grid gap-2">
                                        <Label htmlFor="selection-reprepare-skus">
                                            已选商品编号（每行一个）
                                        </Label>
                                        <textarea
                                            id="selection-reprepare-skus"
                                            className="min-h-28 rounded-md border p-2"
                                            value={field.state.value}
                                            onChange={(e) =>
                                                field.handleChange(
                                                    e.target.value,
                                                )
                                            }
                                        />
                                    </div>
                                )}
                            </form.Field>
                        )}
                        {detail.form === "PACKAGE" && (
                            <form.Field name="tiers">
                                {(field) => (
                                    <TierEditor
                                        value={field.state.value}
                                        onChange={(tiers) =>
                                            field.handleChange(
                                                tiers.map((tier) => ({
                                                    ...tier,
                                                    tier_id: tier.tier_id ?? "",
                                                })),
                                            )
                                        }
                                    />
                                )}
                            </form.Field>
                        )}
                    </fieldset>
                    {error && (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    )}
                    <Button
                        id="selection-reprepare-submit"
                        type="submit"
                        disabled={operations.prepare.isPending}
                    >
                        {operations.prepare.isPending
                            ? "正在创建准备任务…"
                            : "按以上规则准备"}
                    </Button>
                </form>
            </DialogContent>
        </Dialog>
    )
}
