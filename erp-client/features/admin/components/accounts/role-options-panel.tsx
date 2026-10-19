"use client"

import { ChevronsUpDownIcon, SearchIcon } from "lucide-react"

import { roleCapabilitySummary } from "../../lib/role-summary"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { permissionLabel } from "@/lib/permission-catalog"
import { cn } from "@/lib/utils"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    useRoleFilter,
    type RoleOption,
} from "@/features/admin/hooks/use-role-filter"

/** 收起时展示所选角色，搜索、复选和权限明细放在限高的下拉面板内。 */
export function RoleOptionsPanel({
    options,
    selected,
    onToggle,
    invalid,
    disabled = false,
    id = "governance-admin-role-options",
}: {
    options: readonly RoleOption[]
    selected: readonly string[]
    onToggle: (id: string, checked: boolean) => void
    invalid: boolean
    disabled?: boolean
    id?: string
}) {
    const { keyword, setKeyword, filtered } = useRoleFilter(options)
    const selectedNames = selected.map(
        (roleId) =>
            options.find((role) => role.id === roleId)?.name ??
            "角色信息待确认",
    )
    const selectionLabel = selectedNames.length
        ? selectedNames.slice(0, 2).join("、") +
          (selectedNames.length > 2 ? `等 ${selectedNames.length} 个角色` : "")
        : "请选择角色，可多选"

    return (
        <Popover
            onOpenChange={(open) => {
                if (!open) setKeyword("")
            }}
        >
            <PopoverTrigger
                id={`${id}-trigger`}
                disabled={disabled}
                aria-labelledby={`${id}-label ${id}-selection`}
                aria-describedby={invalid ? `${id}-error` : undefined}
                aria-invalid={invalid || undefined}
                render={
                    <Button
                        type="button"
                        variant="outline"
                        className="w-full min-w-0 justify-between font-normal"
                    />
                }
            >
                <span
                    id={`${id}-selection`}
                    title={selectedNames.join("、")}
                    className={cn(
                        "truncate",
                        !selected.length && "text-muted-foreground",
                    )}
                >
                    {selectionLabel}
                </span>
                <ChevronsUpDownIcon
                    className="shrink-0 text-muted-foreground"
                    aria-hidden="true"
                />
            </PopoverTrigger>
            <PopoverContent
                align="start"
                aria-labelledby={`${id}-label`}
                className="max-h-[min(24rem,var(--available-height))] w-[var(--anchor-width)] max-w-[calc(100vw-2rem)] gap-2 overflow-hidden rounded-lg p-2"
            >
                <InputGroup className="shrink-0">
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        id={`${id}-search`}
                        type="search"
                        value={keyword}
                        onChange={(e) => setKeyword(e.target.value)}
                        placeholder="搜索角色"
                        aria-label="搜索角色"
                    />
                </InputGroup>
                <div
                    data-invalid={invalid || undefined}
                    className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto overscroll-contain data-[invalid]:border-destructive"
                >
                    {filtered.length === 0 ? (
                        <p className="px-2 py-3 text-center text-xs text-muted-foreground">
                            无匹配角色
                        </p>
                    ) : (
                        filtered.map((role) => {
                            const checked = selected.includes(role.id)
                            const segment = toAutomationIdSegment(role.id)
                            return (
                                <Collapsible
                                    key={role.id}
                                    className={cn(
                                        "shrink-0 rounded-md border border-transparent transition-colors",
                                        checked &&
                                            "border-primary/50 bg-primary/5",
                                    )}
                                >
                                    <div className="flex items-center gap-2 pr-3">
                                        <label
                                            htmlFor={`${id}-option-${segment}`}
                                            className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-md p-2 text-sm hover:bg-muted/40"
                                        >
                                            <Checkbox
                                                id={`${id}-option-${segment}`}
                                                checked={checked}
                                                disabled={disabled}
                                                aria-invalid={
                                                    invalid || undefined
                                                }
                                                className="rounded-sm border-muted-foreground/50 bg-background"
                                                onCheckedChange={(next) =>
                                                    onToggle(
                                                        role.id,
                                                        next === true,
                                                    )
                                                }
                                                aria-label={role.name}
                                            />
                                            <span className="min-w-0">
                                                <span className="block font-medium">
                                                    {role.name}
                                                </span>
                                                <span className="mt-1 block text-xs leading-5 text-muted-foreground">
                                                    {roleCapabilitySummary(
                                                        role.permissions,
                                                    )}
                                                </span>
                                            </span>
                                        </label>
                                        <CollapsibleTrigger
                                            id={`${id}-permissions-${segment}`}
                                            render={
                                                <Button
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    className="shrink-0 text-xs text-muted-foreground"
                                                />
                                            }
                                            aria-label={`查看${role.name}的权限`}
                                        >
                                            查看权限
                                        </CollapsibleTrigger>
                                    </div>
                                    <CollapsibleContent className="border-t border-border px-3 py-3">
                                        <ul className="grid gap-2 text-xs leading-5 text-muted-foreground sm:grid-cols-2">
                                            {role.permissions?.length ? (
                                                [
                                                    ...new Set(
                                                        role.permissions,
                                                    ),
                                                ].map((permission) => (
                                                    <li key={permission}>
                                                        {permissionLabel(
                                                            permission,
                                                        )}
                                                    </li>
                                                ))
                                            ) : (
                                                <li>
                                                    {role.permissions
                                                        ? "尚未授予操作权限"
                                                        : "操作权限信息待确认"}
                                                </li>
                                            )}
                                        </ul>
                                    </CollapsibleContent>
                                </Collapsible>
                            )
                        })
                    )}
                </div>
            </PopoverContent>
        </Popover>
    )
}
