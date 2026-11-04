"use client"

import { ListWorkspaceViews } from "@/components/business/list-workspace"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    WorkspaceSearchOption,
    WorkspaceSearchOptions,
} from "@/lib/workspace-search"
import type { WorkspaceUrlState } from "../lib/url-state"
import { queueTypeLabel } from "../lib/queue-presentation"

/** 事项类型复用列表页的下划线导航；窄窗口可横向滚动到全部类型。 */
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
    return (
        <ListWorkspaceViews
            ariaLabel="事项类型"
            items={[
                {
                    id: "workspace-type-all",
                    label: "全部",
                    count: counts["types-all"],
                    active: !selected.length,
                    onClick: () => onChange([]),
                },
                ...visible.map((type) => ({
                    id: `workspace-type-${toAutomationIdSegment(type.value)}`,
                    label: queueTypeLabel(type),
                    count: counts[type.value],
                    active: selected.includes(type.value),
                    onClick: () => onChange([type.value]),
                })),
            ]}
        />
    )
}
