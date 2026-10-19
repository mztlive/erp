"use client"

import { OptionCombobox } from "@/components/business/option-combobox"
import { NativeCheckbox } from "@/components/ui/checkbox"
import type { WorkspaceUrlState } from "../lib/url-state"

/** 截止条件与受阻状态放在主查询栏，独立收窄队列。 */
export function WorkspaceQueueFilters({
    state,
    onChange,
}: {
    state: WorkspaceUrlState
    onChange: (
        patch: Pick<WorkspaceUrlState, "workItemType" | "due" | "blocked">,
    ) => void
}) {
    return (
        <div className="flex flex-wrap items-center gap-4">
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
