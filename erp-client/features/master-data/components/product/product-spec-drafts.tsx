"use client"

import { ArrowDownIcon, ArrowUpIcon, PlusIcon, XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { moveListItem } from "@/features/master-data/lib/move-list-item"
import {
    createSpecDraft,
    nextSpecDraftId,
    type ProductSpecDraft,
} from "@/features/master-data/lib/product-editor-model"
import { toAutomationIdSegment } from "@/lib/automation-id"

type ProductSpecDraftsEditorProps = {
    canRevise: boolean
    specDrafts: readonly ProductSpecDraft[]
    skuCount: number
    syncSpecDrafts: (next: readonly ProductSpecDraft[]) => void
    idPrefix?: string
}

function ProductSpecDraftsEditor({
    canRevise,
    specDrafts,
    skuCount,
    syncSpecDrafts,
    idPrefix = "master-data-product-spec",
}: ProductSpecDraftsEditorProps) {
    return (
        <fieldset disabled={!canRevise} className="min-w-0 space-y-3">
            <legend className="sr-only">商品规格</legend>
            <div className="space-y-3">
                {specDrafts.map((draft, index) => {
                    const specSegment = toAutomationIdSegment(draft.draftId)
                    const specItemId = `${idPrefix}-${specSegment}`
                    return (
                        <div
                            key={draft.draftId}
                            className="grid min-w-0 gap-3 rounded-lg border border-border bg-muted/20 p-3 sm:grid-cols-[10rem_minmax(0,1fr)_auto]"
                        >
                            <div className="min-w-0 space-y-1.5">
                                <Label
                                    htmlFor={`${specItemId}-name`}
                                    className="text-xs text-muted-foreground"
                                >
                                    规格名称
                                </Label>
                                <Input
                                    id={`${specItemId}-name`}
                                    className="h-8 bg-card"
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
                            <div className="min-w-0 space-y-1.5">
                                <p className="text-xs font-medium text-muted-foreground">
                                    规格值
                                </p>
                                <div className="flex flex-wrap items-center gap-2">
                                    {draft.values.map(
                                        (specValue, valueIndex) => {
                                            const valueSegment =
                                                toAutomationIdSegment(
                                                    draft.valueIds[valueIndex],
                                                )
                                            const valueId = `${specItemId}-value-${valueSegment}-input`
                                            return (
                                                <div
                                                    key={
                                                        draft.valueIds[
                                                            valueIndex
                                                        ]
                                                    }
                                                    className="flex w-40 max-w-full items-center gap-1"
                                                >
                                                    <Input
                                                        id={valueId}
                                                        className="h-8 bg-background"
                                                        value={specValue}
                                                        onChange={(event) => {
                                                            const nextValues = [
                                                                ...draft.values,
                                                            ]
                                                            nextValues[
                                                                valueIndex
                                                            ] =
                                                                event.target.value
                                                            const next = [
                                                                ...specDrafts,
                                                            ]
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
                                                        id={`${specItemId}-value-${valueSegment}-remove`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-xs"
                                                        aria-label={`删除规格值 ${specValue || valueIndex + 1}`}
                                                        onClick={() => {
                                                            const next = [
                                                                ...specDrafts,
                                                            ]
                                                            next[index] = {
                                                                ...draft,
                                                                valueIds:
                                                                    draft.valueIds.filter(
                                                                        (
                                                                            _,
                                                                            i,
                                                                        ) =>
                                                                            i !==
                                                                            valueIndex,
                                                                    ),
                                                                values: draft.values.filter(
                                                                    (_, i) =>
                                                                        i !==
                                                                        valueIndex,
                                                                ),
                                                            }
                                                            syncSpecDrafts(next)
                                                        }}
                                                    >
                                                        <XIcon />
                                                    </Button>
                                                </div>
                                            )
                                        },
                                    )}
                                    <Button
                                        id={`${specItemId}-add-value`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        className="h-8"
                                        onClick={() => {
                                            const next = [...specDrafts]
                                            next[index] = {
                                                ...draft,
                                                values: [...draft.values, ""],
                                                valueIds: [
                                                    ...draft.valueIds,
                                                    nextSpecDraftId(),
                                                ],
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
                            <div className="flex items-center justify-end gap-1 sm:pt-6">
                                <Button
                                    id={`${specItemId}-move-up`}
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
                                    id={`${specItemId}-move-down`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    disabled={index === specDrafts.length - 1}
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
                                    id={`${specItemId}-remove`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label={`删除规格项 ${index + 1}`}
                                    onClick={() => {
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
                    )
                })}
            </div>
            <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="text-xs text-muted-foreground">
                    {specDrafts.length} 个规格项 · {skuCount} 个 SKU 已应用
                </p>
                <Button
                    id={`${idPrefix}-add`}
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() =>
                        syncSpecDrafts([...specDrafts, createSpecDraft()])
                    }
                >
                    <PlusIcon data-icon="inline-start" aria-hidden />
                    添加规格项
                </Button>
            </div>
        </fieldset>
    )
}

export { ProductSpecDraftsEditor }
export type { ProductSpecDraftsEditorProps }
