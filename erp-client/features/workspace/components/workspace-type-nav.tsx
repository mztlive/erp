"use client"

import { LayoutGridIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    WorkspaceSearchOption,
    WorkspaceSearchOptions,
} from "@/lib/workspace-search"
import type { WorkspaceUrlState } from "../lib/url-state"
import { queueTypeIcon, queueTypeLabel } from "../lib/queue-presentation"

/** 全部事项类型直接铺开，宽度不足时换行，保留常用类型在前的顺序。 */
export function WorkspaceTypeNav({
    state,
    options,
    types,
    counts,
    onChange,
}: {
    state: WorkspaceUrlState
    options: WorkspaceSearchOptions
    types: readonly WorkspaceSearchOption[]
    counts: Record<string, number | undefined>
    onChange: (types: string[]) => void
}) {
    const selected = state.filters?.item_types ?? []
    const all = options.item_types ?? []
    // 聚合审批条件仍可由岗位入口携带，但不作为具体事项类型按钮展示。
    const visible = [
        ...types,
        ...all.filter(
            (option) => !types.some((type) => type.value === option.value),
        ),
    ].filter((option) => option.value !== "task:DOCUMENT_APPROVAL")
    const typeButton = (type: WorkspaceSearchOption) => {
        const Icon = queueTypeIcon(type.value)
        const active = selected.includes(type.value)
        return (
            <Button
                key={type.value}
                id={`workspace-type-${toAutomationIdSegment(type.value)}`}
                size="sm"
                variant={active ? "default" : "outline"}
                aria-pressed={active}
                className="h-9 gap-2 px-3"
                onClick={() => onChange([type.value])}
            >
                <Icon aria-hidden="true" className="size-4" />
                {queueTypeLabel(type)}
                {counts[type.value] !== undefined && (
                    <span className="num opacity-70">{counts[type.value]}</span>
                )}
            </Button>
        )
    }
    return (
        <nav
            aria-label="事项类型"
            className="border-b border-border/60 px-5 py-4"
        >
            <div className="flex flex-wrap items-center gap-2">
                <Button
                    id="workspace-type-all"
                    size="sm"
                    className="h-9 gap-2 px-3"
                    variant={!selected.length ? "default" : "outline"}
                    aria-pressed={!selected.length}
                    onClick={() => onChange([])}
                >
                    <LayoutGridIcon aria-hidden="true" />
                    全部事项
                    {counts["types-all"] !== undefined && (
                        <span className="num opacity-70">
                            {counts["types-all"]}
                        </span>
                    )}
                </Button>
                {visible.map(typeButton)}
            </div>
        </nav>
    )
}
