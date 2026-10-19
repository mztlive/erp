"use client"

import { OptionCombobox } from "@/components/business/option-combobox"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { TYPE_META } from "../api/work-item-meta"
import type { WorkspaceUrlState } from "../lib/url-state"
import { workspaceCategoryLabel } from "./workspace-overview"

/** 个人类型数量使用完整服务端统计，管理范围不复用个人数量。 */
export function WorkspaceQueueTypeNav({
    state,
    counts,
    total,
    onChange,
}: {
    state: WorkspaceUrlState
    counts?: Readonly<Record<string, number>>
    total?: number
    onChange: (type?: string) => void
}) {
    const types = Object.keys(TYPE_META).filter(
        (type) => type !== "APPROVAL_INSTANCE",
    )
    const visibleTypes = counts
        ? Object.keys(counts).filter((type) => counts[type] > 0)
        : []
    if (state.workItemType && !visibleTypes.includes(state.workItemType))
        visibleTypes.push(state.workItemType)
    visibleTypes.sort((left, right) => {
        const leftRank = types.indexOf(left)
        const rightRank = types.indexOf(right)
        return (
            (leftRank < 0 ? types.length : leftRank) -
                (rightRank < 0 ? types.length : rightRank) ||
            left.localeCompare(right)
        )
    })

    if (state.view === "managed") {
        if (state.workItemType && !types.includes(state.workItemType))
            types.push(state.workItemType)
        return (
            <div className="flex flex-wrap items-center gap-2 border-t border-border/60 pt-3">
                <label
                    htmlFor="workspace-queue-type"
                    className="text-xs text-muted-foreground"
                >
                    任务类型
                </label>
                <OptionCombobox
                    id="workspace-queue-type"
                    aria-label="任务类型"
                    className="w-56 max-w-full"
                    value={state.workItemType ?? "all"}
                    allowClear={false}
                    onValueChange={(value) =>
                        onChange(value && value !== "all" ? value : undefined)
                    }
                    options={[
                        { value: "all", label: "全部类型" },
                        ...types.map((type) => ({
                            value: type,
                            label: workspaceCategoryLabel(type),
                        })),
                    ]}
                />
            </div>
        )
    }

    return (
        <div
            role="group"
            aria-label="任务类型"
            className="flex flex-wrap items-center gap-2 border-t border-border/60 pt-3"
        >
            <span className="mr-1 text-xs text-muted-foreground">任务类型</span>
            {[undefined, ...visibleTypes].map((type) => {
                const active = state.workItemType === type
                const label = type ? workspaceCategoryLabel(type) : "全部"
                const count = type
                    ? counts
                        ? (counts[type] ?? 0)
                        : undefined
                    : total
                return (
                    <Button
                        key={type ?? "all"}
                        id={`workspace-queue-type-${toAutomationIdSegment(type ?? "all")}`}
                        type="button"
                        size="sm"
                        variant={active ? "secondary" : "outline"}
                        className="h-auto min-h-8 max-w-full rounded-full px-3 py-1.5 whitespace-normal"
                        aria-pressed={active}
                        aria-label={`${label}${count == null ? "" : ` ${count} 项待处理`}`}
                        onClick={() => onChange(type)}
                    >
                        {label}
                        {count != null && (
                            <Badge
                                variant={active ? "default" : "secondary"}
                                className="num min-w-5 rounded-full shadow-none"
                            >
                                {count}
                            </Badge>
                        )}
                    </Button>
                )
            })}
        </div>
    )
}
