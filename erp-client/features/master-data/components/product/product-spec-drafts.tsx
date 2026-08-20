"use client"

import {
    ArrowDownIcon,
    ArrowUpIcon,
    GripVerticalIcon,
    PlusIcon,
    XIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { moveListItem } from "@/features/master-data/lib/move-list-item"
import type { ProductSpecDraft } from "@/features/master-data/lib/product-editor-model"
import { cn } from "@/lib/utils"

type ProductSpecDraftsEditorProps = {
    canRevise: boolean
    specDrafts: readonly ProductSpecDraft[]
    skuCount: number
    syncSpecDrafts: (next: readonly ProductSpecDraft[]) => void
}

function ProductSpecDraftsEditor({
    canRevise,
    specDrafts,
    skuCount,
    syncSpecDrafts,
}: ProductSpecDraftsEditorProps) {
    return (
        <fieldset
            id="product-section-sku"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] space-y-4 border-b border-grid p-5 last:border-b-0",
            )}
            disabled={!canRevise}
        >
            <legend className="sr-only">商品规格</legend>
            <div className="text-base font-semibold">商品规格</div>
            <div className="flex flex-wrap items-center justify-between gap-3">
                <p className="text-xs text-muted-foreground">
                    规格值会自动组合成 SKU；调整规格顺序时保留可匹配的原 SKU
                    数据。
                </p>
                <Badge variant="secondary">
                    {specDrafts.length} 个规格项 · {skuCount} 个 SKU
                </Badge>
            </div>
            <div className="space-y-3">
                {specDrafts.map((draft, index) => (
                    <div
                        key={index}
                        className="rounded-xl border border-border bg-surface-sunken"
                    >
                        <div className="flex flex-wrap items-end gap-3 border-b border-border px-3 py-3">
                            <div className="flex items-center gap-2 self-center">
                                <GripVerticalIcon
                                    className="size-4 text-muted-foreground"
                                    aria-hidden
                                />
                                <Badge variant="outline">
                                    规格项 {index + 1}
                                </Badge>
                            </div>
                            <div className="min-w-48 flex-1 space-y-1.5 sm:max-w-sm">
                                <Label
                                    htmlFor={`product-spec-name-${index}`}
                                    className="text-sm font-medium text-foreground"
                                >
                                    规格名称
                                </Label>
                                <Input
                                    id={`product-spec-name-${index}`}
                                    className="bg-card font-medium shadow-sm"
                                    value={draft.name}
                                    onChange={(event) => {
                                        const next = [...specDrafts]
                                        next[index] = {
                                            ...draft,
                                            name: event.target.value,
                                        }
                                        syncSpecDrafts(next)
                                    }}
                                    placeholder="规格名称，如：颜色"
                                />
                            </div>
                            <div className="ml-auto flex items-center gap-1">
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    disabled={index === 0}
                                    aria-label={`规格项 ${index + 1} 上移`}
                                    onClick={() =>
                                        syncSpecDrafts(
                                            moveListItem(
                                                specDrafts,
                                                index,
                                                index - 1,
                                            ),
                                        )
                                    }
                                >
                                    <ArrowUpIcon />
                                </Button>
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    disabled={
                                        index === specDrafts.length - 1
                                    }
                                    aria-label={`规格项 ${index + 1} 下移`}
                                    onClick={() =>
                                        syncSpecDrafts(
                                            moveListItem(
                                                specDrafts,
                                                index,
                                                index + 1,
                                            ),
                                        )
                                    }
                                >
                                    <ArrowDownIcon />
                                </Button>
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label={`删除规格项 ${index + 1}`}
                                    onClick={() => {
                                        if (
                                            !window.confirm(
                                                "删除规格项会移除对应组合生成的 SKU 行（含价格、主图、条码）。确定删除？",
                                            )
                                        ) {
                                            return
                                        }
                                        syncSpecDrafts(
                                            specDrafts.filter(
                                                (_, i) => i !== index,
                                            ),
                                        )
                                    }}
                                >
                                    <XIcon />
                                </Button>
                            </div>
                        </div>
                        <div className="space-y-2 p-3">
                            <Label className="text-xs text-muted-foreground">
                                规格值
                            </Label>
                            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                                {draft.values.map((specValue, valueIndex) => (
                                    <div
                                        key={valueIndex}
                                        className="flex items-center gap-1"
                                    >
                                        <Input
                                            className="h-8 bg-background"
                                            value={specValue}
                                            onChange={(event) => {
                                                const nextValues = [
                                                    ...draft.values,
                                                ]
                                                nextValues[valueIndex] =
                                                    event.target.value
                                                const next = [...specDrafts]
                                                next[index] = {
                                                    ...draft,
                                                    values: nextValues,
                                                }
                                                syncSpecDrafts(next)
                                            }}
                                            placeholder={`请输入${draft.name || "规格"}`}
                                            aria-label={`${draft.name || `规格项 ${index + 1}`}的第 ${valueIndex + 1} 个值`}
                                        />
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label={`删除规格值 ${specValue || valueIndex + 1}`}
                                            onClick={() => {
                                                if (
                                                    !window.confirm(
                                                        "删除规格取值会移除对应组合生成的 SKU 行（含价格、主图、条码）。确定删除？",
                                                    )
                                                ) {
                                                    return
                                                }
                                                const next = [...specDrafts]
                                                next[index] = {
                                                    ...draft,
                                                    values: draft.values.filter(
                                                        (_, i) =>
                                                            i !== valueIndex,
                                                    ),
                                                }
                                                syncSpecDrafts(next)
                                            }}
                                        >
                                            <XIcon />
                                        </Button>
                                    </div>
                                ))}
                            </div>
                            <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() => {
                                    const next = [...specDrafts]
                                    next[index] = {
                                        ...draft,
                                        values: [...draft.values, ""],
                                    }
                                    syncSpecDrafts(next)
                                }}
                            >
                                <PlusIcon
                                    data-icon="inline-start"
                                    aria-hidden
                                />
                                添加规格值
                            </Button>
                        </div>
                    </div>
                ))}
            </div>
            <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() =>
                    syncSpecDrafts([
                        ...specDrafts,
                        { name: "", values: [""] },
                    ])
                }
            >
                <PlusIcon data-icon="inline-start" aria-hidden />
                添加规格项
            </Button>
        </fieldset>
    )
}

export { ProductSpecDraftsEditor }
export type { ProductSpecDraftsEditorProps }
