"use client"

import { SearchIcon } from "lucide-react"

import { Checkbox } from "@/components/ui/checkbox"
import { InputGroupInput } from "@/components/ui/input-group"
import {
    useRoleFilter,
    type RoleOption,
} from "@/features/admin/hooks/use-role-filter"

/** 角色多选项面板：搜索框 + 滚动复选列表。 */
export function RoleOptionsPanel({
    options,
    selected,
    onToggle,
    invalid,
}: {
    options: readonly RoleOption[]
    selected: readonly string[]
    onToggle: (id: string, checked: boolean) => void
    invalid: boolean
}) {
    const { keyword, setKeyword, filtered } = useRoleFilter(options)

    return (
        <div className="space-y-2">
            <div className="relative">
                <SearchIcon
                    aria-hidden="true"
                    className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
                />
                <InputGroupInput
                    type="search"
                    value={keyword}
                    onChange={(e) => setKeyword(e.target.value)}
                    placeholder="搜索角色"
                    aria-label="搜索角色"
                    className="h-8 pl-8 text-sm"
                />
            </div>
            <div
                data-invalid={invalid || undefined}
                className="max-h-44 space-y-0.5 overflow-y-auto rounded-lg border p-1.5 data-[invalid]:border-destructive"
            >
                {filtered.length === 0 ? (
                    <p className="px-2 py-3 text-center text-xs text-muted-foreground">
                        无匹配角色
                    </p>
                ) : (
                    filtered.map((role) => {
                        const checked = selected.includes(role.id)
                        return (
                            <label
                                key={role.id}
                                className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-sm hover:bg-accent"
                            >
                                <Checkbox
                                    checked={checked}
                                    onCheckedChange={(next) =>
                                        onToggle(role.id, next === true)
                                    }
                                />
                                <span className="min-w-0 truncate">
                                    {role.name}
                                </span>
                            </label>
                        )
                    })
                )}
            </div>
        </div>
    )
}
