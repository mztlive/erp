"use client"

import { SearchIcon } from "lucide-react"

import { Checkbox } from "@/components/ui/checkbox"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
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
        <div className="flex flex-col gap-2">
            <InputGroup>
                <InputGroupAddon>
                    <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                    type="search"
                    value={keyword}
                    onChange={(e) => setKeyword(e.target.value)}
                    placeholder="搜索角色"
                    aria-label="搜索角色"
                />
            </InputGroup>
            <div
                data-invalid={invalid || undefined}
                className="flex max-h-44 flex-col gap-0.5 overflow-y-auto rounded-lg border border-border p-1.5 data-[invalid]:border-destructive"
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
                                className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1.5 text-sm hover:bg-muted/40"
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
