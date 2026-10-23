"use client"

import { ArrowDownWideNarrowIcon, ChevronDownIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuRadioGroup,
    DropdownMenuRadioItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { WorkspaceSort } from "../types"

const SORT_OPTIONS = [
    {
        value: "priority_due",
        label: "超期优先，再按优先级",
        shortLabel: "超期优先 · 优先级",
    },
    {
        value: "due_asc",
        label: "截止时间最早优先",
        shortLabel: "截止时间最早",
    },
    {
        value: "created_desc",
        label: "进入时间最新优先",
        shortLabel: "进入时间最新",
    },
] satisfies { value: WorkspaceSort; label: string; shortLabel: string }[]

export function WorkspaceSortMenu({
    value,
    onValueChange,
}: {
    value: WorkspaceSort
    onValueChange: (value: WorkspaceSort) => void
}) {
    const selected = SORT_OPTIONS.find((option) => option.value === value)

    return (
        <DropdownMenu>
            <DropdownMenuTrigger
                render={
                    <Button
                        id="workspace-queue-toolbar-sort"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="font-normal text-muted-foreground"
                        aria-label={`结果排序：${selected?.label ?? "选择排序"}`}
                    />
                }
            >
                <ArrowDownWideNarrowIcon
                    data-icon="inline-start"
                    aria-hidden="true"
                />
                {selected?.shortLabel ?? "选择排序"}
                <ChevronDownIcon data-icon="inline-end" aria-hidden="true" />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-60">
                <DropdownMenuRadioGroup
                    aria-label="结果排序"
                    value={value}
                    onValueChange={(next) => {
                        const option = SORT_OPTIONS.find(
                            (item) => item.value === next,
                        )
                        if (option) onValueChange(option.value)
                    }}
                >
                    {SORT_OPTIONS.map((option) => (
                        <DropdownMenuRadioItem
                            id={`workspace-queue-toolbar-sort-option-${toAutomationIdSegment(option.value)}`}
                            key={option.value}
                            value={option.value}
                        >
                            {option.label}
                        </DropdownMenuRadioItem>
                    ))}
                </DropdownMenuRadioGroup>
            </DropdownMenuContent>
        </DropdownMenu>
    )
}
