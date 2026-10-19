"use client"

import { OptionCombobox } from "@/components/business/option-combobox"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { TYPE_META } from "../api/work-item-meta"
import type { WorkspaceUrlState } from "../lib/url-state"
import { workspaceCategoryLabel } from "./workspace-overview"

/** 类型、截止条件与受阻状态独立收窄队列，排序由工具栏单独控制。 */
export function WorkspaceQueueFilters({
    state,
    onChange,
}: {
    state: WorkspaceUrlState
    onChange: (
        patch: Pick<WorkspaceUrlState, "workItemType" | "due" | "blocked">,
    ) => void
}) {
    const types = Object.keys(TYPE_META).filter(
        (type) =>
            type !== "APPROVAL_INSTANCE" &&
            (!state.family || TYPE_META[type]?.family === state.family),
    )
    if (state.workItemType && !types.includes(state.workItemType))
        types.push(state.workItemType)
    return (
        <div className="flex flex-wrap items-center gap-4 border-t border-border/60 pt-3">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
                <label
                    className="text-xs text-muted-foreground"
                    htmlFor="workspace-queue-type"
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
                        onChange({
                            workItemType:
                                value && value !== "all" ? value : undefined,
                        })
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
            <div className="flex min-w-0 flex-wrap items-center gap-2">
                <label
                    className="text-xs text-muted-foreground"
                    htmlFor="workspace-queue-due"
                >
                    截止时间
                </label>
                <OptionCombobox
                    id="workspace-queue-due"
                    aria-label="截止时间"
                    className="w-40 max-w-full"
                    value={state.due ?? "all"}
                    allowClear={false}
                    onValueChange={(value) =>
                        onChange({
                            due:
                                value === "today" || value === "overdue"
                                    ? value
                                    : undefined,
                        })
                    }
                    options={[
                        { value: "all", label: "不限" },
                        { value: "today", label: "今日到期" },
                        { value: "overdue", label: "已超期" },
                    ]}
                />
            </div>
            <label
                htmlFor="workspace-queue-blocked"
                className="flex items-center gap-2 text-xs text-muted-foreground"
            >
                <NativeCheckbox
                    id="workspace-queue-blocked"
                    checked={Boolean(state.blocked)}
                    onCheckedChange={(value) =>
                        onChange({ blocked: value === true })
                    }
                />
                仅看受阻
            </label>
        </div>
    )
}
