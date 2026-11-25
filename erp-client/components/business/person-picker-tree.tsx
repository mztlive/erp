"use client"

import * as React from "react"
import { ChevronRightIcon, FolderIcon, UsersIcon } from "lucide-react"

import type {
    PersonPickerBranch,
    PersonPickerPerson,
} from "@/components/business/person-picker-model"
import { Checkbox } from "@/components/ui/checkbox"
import { RadioGroupItem } from "@/components/ui/radio-group"
import { cn } from "@/lib/utils"

type PersonPickerTreeProps = {
    roots: PersonPickerBranch[]
    unassigned: PersonPickerPerson[]
    unassignedDepartmentLabel: string
    unknownDepartment: PersonPickerPerson[]
    selected: ReadonlySet<string>
    collapsed: ReadonlySet<string>
    setCollapsed: React.Dispatch<React.SetStateAction<ReadonlySet<string>>>
    query: string
    multiple: boolean
    itemId: (kind: string, key: string) => string
    choose: (personId: string, checked: boolean) => void
}

/** 部门使用展开按钮，人员使用共享选择控件，不混用 ARIA tree 键盘模型。 */
export function PersonPickerTree({
    roots,
    unassigned,
    unassignedDepartmentLabel,
    unknownDepartment,
    selected,
    collapsed,
    setCollapsed,
    query,
    multiple,
    itemId,
    choose,
}: PersonPickerTreeProps) {
    const renderPerson = (person: PersonPickerPerson) => {
        const controlId = itemId("person", person.id)
        return (
            <li key={person.id}>
                <label
                    id={`${controlId}-label`}
                    htmlFor={controlId}
                    className={cn(
                        "flex min-h-control cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 transition-colors hover:bg-muted/70",
                        selected.has(person.id) &&
                            "bg-sidebar-accent text-sidebar-accent-foreground hover:bg-sidebar-accent",
                        person.disabled &&
                            "cursor-not-allowed text-muted-foreground hover:bg-transparent",
                    )}
                >
                    {multiple ? (
                        <Checkbox
                            id={controlId}
                            nativeButton
                            render={
                                <button
                                    type="button"
                                    aria-labelledby={`${controlId}-label`}
                                />
                            }
                            aria-labelledby={`${controlId}-label`}
                            checked={selected.has(person.id)}
                            disabled={person.disabled}
                            onCheckedChange={(checked) =>
                                choose(person.id, checked)
                            }
                        />
                    ) : (
                        <RadioGroupItem
                            id={controlId}
                            nativeButton
                            render={
                                <button
                                    type="button"
                                    aria-labelledby={`${controlId}-label`}
                                />
                            }
                            aria-labelledby={`${controlId}-label`}
                            value={person.id}
                            disabled={person.disabled}
                        />
                    )}
                    <span className="min-w-0 flex-1 break-words font-medium">
                        {person.name || "未命名人员"}
                    </span>
                    {(person.description || person.disabled) && (
                        <span className="max-w-[45%] break-words text-xs text-muted-foreground">
                            {person.disabled
                                ? person.disabledReason || "不可选择"
                                : person.description}
                        </span>
                    )}
                </label>
            </li>
        )
    }

    const renderBranch = (branch: PersonPickerBranch): React.ReactNode => {
        const expanded = Boolean(query.trim()) || !collapsed.has(branch.id)
        const contentId = itemId("department-content", branch.id)
        return (
            <li key={branch.id}>
                <button
                    id={itemId("department", branch.id)}
                    type="button"
                    aria-expanded={expanded}
                    aria-controls={contentId}
                    className="flex min-h-control w-full items-center gap-2 rounded-md px-2 py-1.5 text-left font-medium transition-colors outline-none hover:bg-muted/70 focus-visible:ring-3 focus-visible:ring-ring"
                    onClick={() => {
                        // 搜索结果保持展开，清除搜索后恢复用户原来的折叠状态。
                        if (query.trim()) return
                        setCollapsed((current) => {
                            const next = new Set(current)
                            if (next.has(branch.id)) next.delete(branch.id)
                            else next.add(branch.id)
                            return next
                        })
                    }}
                    aria-disabled={Boolean(query.trim())}
                >
                    <ChevronRightIcon
                        aria-hidden="true"
                        className={cn(
                            "size-4 shrink-0 transition-transform",
                            expanded && "rotate-90",
                        )}
                    />
                    <FolderIcon
                        aria-hidden="true"
                        className="size-4 shrink-0"
                    />
                    <span className="min-w-0 break-words">{branch.name}</span>
                </button>
                <ul
                    id={contentId}
                    hidden={!expanded}
                    className="ml-4 border-l pl-3"
                >
                    {expanded && branch.children.map(renderBranch)}
                    {expanded && branch.people.map(renderPerson)}
                    {expanded &&
                        branch.children.length === 0 &&
                        branch.people.length === 0 && (
                            <li className="px-3 py-2 text-xs text-muted-foreground">
                                此部门暂无人员
                            </li>
                        )}
                </ul>
            </li>
        )
    }

    return (
        <ul className="space-y-0.5">
            {roots.map(renderBranch)}
            {[
                { label: unassignedDepartmentLabel, group: unassigned },
                { label: "部门信息不可用", group: unknownDepartment },
            ].map(({ label, group }) => {
                return group.length ? (
                    <li key={label}>
                        <div className="flex min-h-control items-center gap-2 px-2 py-1.5 font-medium">
                            <UsersIcon aria-hidden="true" className="size-4" />
                            {label}
                        </div>
                        <ul className="ml-4 border-l pl-3">
                            {group.map(renderPerson)}
                        </ul>
                    </li>
                ) : null
            })}
        </ul>
    )
}
