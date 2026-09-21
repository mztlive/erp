"use client"

import { SearchIcon, ShieldAlertIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { usePermissionPanel } from "@/features/admin/hooks/use-permission-panel"
import {
    actionLabel,
    permissionLabel,
    permissionGroupSegment,
    type PermissionMatrixGroup,
} from "@/features/admin/lib/permission-catalog"
import {
    PERMISSION_VIEWS,
    permissionArea,
    type PermissionView,
} from "@/features/admin/lib/permission-editor"

export type { PermissionPanelTab } from "@/features/admin/lib/permission-catalog"

type PermissionOptionsPanelProps = {
    selected: readonly string[]
    initial: readonly string[]
    preservedCodes?: readonly string[]
    onChange: (next: string[]) => void
    view: PermissionView
    onViewChange: (view: PermissionView) => void
    disabled?: boolean
    className?: string
    id?: string
}

/** 模块目录只切换当前编辑区域；筛选与切换均不改动表单内的授权。 */
export function PermissionOptionsPanel({
    selected,
    initial,
    preservedCodes = [],
    onChange,
    view,
    onViewChange,
    disabled = false,
    className,
    id = "governance-admin-permission-panel",
}: PermissionOptionsPanelProps) {
    const panel = usePermissionPanel(selected, initial, view)
    const toggleCodes = (codes: readonly string[], next: boolean) => {
        if (disabled) return
        const drop = new Set(codes)
        onChange(
            next
                ? [...new Set([...selected, ...codes])]
                : selected.filter((code) => !drop.has(code)),
        )
    }
    const areas = [
        ...new Set(
            panel.visibleGroups.map((group) => permissionArea(group.name)),
        ),
    ]

    return (
        <section
            aria-label="操作权限"
            className={cn("flex min-h-0 flex-1 flex-col", className)}
        >
            <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-border pb-3">
                <Tabs
                    value={panel.tab}
                    onValueChange={(next) => {
                        if (next === "business" || next === "system")
                            panel.setTab(next)
                    }}
                >
                    <TabsList variant="line">
                        <TabsTrigger id={`${id}-tab-business`} value="business">
                            业务权限
                        </TabsTrigger>
                        <TabsTrigger id={`${id}-tab-system`} value="system">
                            系统权限
                        </TabsTrigger>
                    </TabsList>
                </Tabs>
                <div className="flex w-full min-w-0 items-center gap-2 sm:ml-auto sm:w-auto">
                    <select
                        id={`${id}-view`}
                        aria-label="权限显示范围"
                        className="h-9 rounded-lg border border-input bg-background px-2 text-sm focus-visible:outline-2 focus-visible:outline-ring"
                        value={view}
                        onChange={(event) =>
                            onViewChange(event.target.value as PermissionView)
                        }
                    >
                        {Object.entries(PERMISSION_VIEWS).map(
                            ([value, label]) => (
                                <option
                                    id={`${id}-view-${value}`}
                                    key={value}
                                    value={value}
                                >
                                    {label}
                                </option>
                            ),
                        )}
                    </select>
                    <InputGroup className="min-w-0 flex-1 sm:w-64 sm:flex-none">
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id={`${id}-search`}
                            type="search"
                            value={panel.keyword}
                            onChange={(event) =>
                                panel.setKeyword(event.target.value)
                            }
                            onKeyDown={(event) => {
                                if (event.key === "Enter")
                                    event.preventDefault()
                            }}
                            placeholder="搜索模块或权限"
                            aria-label="搜索权限"
                        />
                    </InputGroup>
                </div>
            </div>

            <div className="grid min-h-0 flex-1 md:grid-cols-[12rem_minmax(0,1fr)]">
                <nav
                    aria-label="权限模块"
                    className="hidden overflow-y-auto overscroll-contain border-r border-border py-4 pr-3 md:block"
                >
                    {areas.map((area) => (
                        <div key={area} className="mb-5 last:mb-0">
                            <p className="mb-1 px-2 text-xs font-medium text-muted-foreground">
                                {area}
                            </p>
                            {panel.visibleGroups
                                .filter(
                                    (group) =>
                                        permissionArea(group.name) === area,
                                )
                                .map((group) => {
                                    const progress = panel.progressByGroup.find(
                                        (item) => item.name === group.name,
                                    )
                                    return (
                                        <button
                                            key={group.name}
                                            id={`${id}-group-${permissionGroupSegment(group.name)}-nav`}
                                            type="button"
                                            aria-current={
                                                panel.activeGroup?.name ===
                                                group.name
                                                    ? "true"
                                                    : undefined
                                            }
                                            aria-controls={`${id}-content`}
                                            onClick={() =>
                                                panel.setActiveGroup(group.name)
                                            }
                                            className={cn(
                                                "flex min-h-10 w-full items-center justify-between gap-2 rounded-md px-2 py-2 text-left text-sm focus-visible:outline-2 focus-visible:-outline-offset-2",
                                                panel.activeGroup?.name ===
                                                    group.name
                                                    ? "bg-muted font-medium"
                                                    : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                                            )}
                                        >
                                            <span>{group.name}</span>
                                            <span
                                                className="num shrink-0 text-xs text-muted-foreground"
                                                aria-label={`已勾选 ${progress?.selected ?? 0} 项`}
                                            >
                                                {progress?.selected ?? 0}
                                            </span>
                                        </button>
                                    )
                                })}
                        </div>
                    ))}
                    {areas.length === 0 && (
                        <p className="px-2 text-sm text-muted-foreground">
                            没有匹配模块
                        </p>
                    )}
                </nav>
                <div
                    key={panel.activeGroup?.name}
                    className="min-h-0 overflow-y-auto overscroll-contain py-4 md:pl-6"
                    id={`${id}-content`}
                >
                    {panel.visibleGroups.length > 0 && (
                        <select
                            id={`${id}-module`}
                            aria-label="当前权限模块"
                            value={panel.activeGroup?.name ?? ""}
                            onChange={(event) =>
                                panel.setActiveGroup(event.target.value)
                            }
                            className="mb-4 h-10 w-full rounded-lg border border-input bg-background px-3 text-sm md:hidden"
                        >
                            {panel.visibleGroups.map((group) => (
                                <option
                                    key={group.name}
                                    id={`${id}-module-${permissionGroupSegment(group.name)}`}
                                    value={group.name}
                                >
                                    {group.name}
                                </option>
                            ))}
                        </select>
                    )}
                    {panel.activeGroup ? (
                        <PermissionSection
                            key={panel.activeGroup.name}
                            id={`${id}-group-${permissionGroupSegment(panel.activeGroup.name)}`}
                            group={panel.activeGroup}
                            selectedSet={panel.selectedSet}
                            initial={initial}
                            preservedCodes={preservedCodes}
                            filtered={
                                view !== "all" ||
                                panel.keyword.trim().length > 0
                            }
                            disabled={disabled}
                            onToggle={toggleCodes}
                        />
                    ) : (
                        <div
                            className="flex min-h-48 flex-col items-center justify-center gap-3 text-sm text-muted-foreground"
                            role="status"
                        >
                            <p>
                                {view === "changed"
                                    ? "当前分类暂无匹配的权限变更"
                                    : "当前分类没有匹配权限"}
                            </p>
                            <Button
                                id={`${id}-reset-filters`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => {
                                    panel.setKeyword("")
                                    onViewChange("all")
                                }}
                            >
                                显示全部权限
                            </Button>
                        </div>
                    )}
                </div>
            </div>
        </section>
    )
}

function PermissionSection({
    group,
    selectedSet,
    initial,
    preservedCodes,
    filtered,
    disabled,
    onToggle,
    id,
}: {
    group: PermissionMatrixGroup
    selectedSet: ReadonlySet<string>
    initial: readonly string[]
    preservedCodes: readonly string[]
    filtered: boolean
    disabled: boolean
    onToggle: (codes: readonly string[], next: boolean) => void
    id: string
}) {
    const specialPermissions = preservedCodes.filter(
        (code) =>
            code === "*:*" ||
            group.rows.some((row) => code.startsWith(`${row.resource}:`)),
    )
    const initialSet = new Set(initial)
    const selectedCount = group.codes.filter((code) =>
        selectedSet.has(code),
    ).length
    return (
        <section aria-label={`${group.name}权限`}>
            <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
                <div>
                    <h2 className="text-base font-semibold">{group.name}</h2>
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">
                        {group.description}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        {filtered ? "匹配项" : "本模块"}已勾选{" "}
                        <span className="num">
                            {selectedCount} / {group.codes.length}
                        </span>{" "}
                        项
                    </p>
                </div>
                <div className="flex gap-1">
                    <Button
                        id={`${id}-select-all`}
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={
                            disabled || selectedCount === group.codes.length
                        }
                        onClick={() => onToggle(group.codes, true)}
                    >
                        {filtered ? "选择匹配项" : "选择本模块全部"}
                    </Button>
                    <Button
                        id={`${id}-clear`}
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={disabled || selectedCount === 0}
                        onClick={() => onToggle(group.codes, false)}
                    >
                        {filtered ? "清除匹配项" : "清除本模块"}
                    </Button>
                </div>
            </div>
            {specialPermissions.length > 0 && (
                <p className="mb-3 rounded-md bg-muted px-3 py-2 text-xs leading-5 text-muted-foreground">
                    另有特殊授权：
                    {specialPermissions.map(permissionLabel).join("、")}
                    。调整下方单项勾选不会撤销这些授权。
                </p>
            )}
            <div className="divide-y divide-border border-y border-border">
                {group.rows.map((row) => (
                    <fieldset key={row.resource} className="min-w-0 py-4">
                        <legend
                            className={cn(
                                "float-left w-full pb-2 text-sm font-medium",
                                group.rows.length === 1 &&
                                    row.label === group.name &&
                                    "sr-only",
                            )}
                        >
                            {row.label}
                        </legend>
                        <div className="clear-both grid gap-x-3 gap-y-1 sm:grid-cols-2 xl:grid-cols-3">
                            {row.cells.flatMap((item) => {
                                if (!item) return []
                                const checked = selectedSet.has(item.code)
                                const changed =
                                    checked !== initialSet.has(item.code)
                                const checkboxId = `${id}-cell-${toAutomationIdSegment(item.code)}`
                                return [
                                    <label
                                        key={item.code}
                                        htmlFor={checkboxId}
                                        title={[
                                            item.description,
                                            ...(item.relatedGroups ?? []).map(
                                                (name) => `同时用于${name}`,
                                            ),
                                        ].join("；")}
                                        className={cn(
                                            "flex min-h-10 cursor-pointer items-center gap-2.5 rounded-md border px-3 py-2 text-sm hover:bg-muted/60",
                                            checked
                                                ? "border-border bg-muted/40"
                                                : "border-transparent",
                                            disabled &&
                                                "cursor-default opacity-60",
                                        )}
                                    >
                                        <input
                                            id={checkboxId}
                                            type="checkbox"
                                            checked={checked}
                                            disabled={disabled}
                                            aria-label={`${row.label} · ${actionLabel(item.action)}`}
                                            className="size-4 shrink-0 accent-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
                                            onChange={(event) =>
                                                onToggle(
                                                    [item.code],
                                                    event.target.checked,
                                                )
                                            }
                                        />
                                        <span className="min-w-0 flex-1">
                                            {actionLabel(item.action)}
                                        </span>
                                        {item.dangerous && (
                                            <ShieldAlertIcon
                                                className="size-3.5 shrink-0 text-destructive"
                                                aria-label="高风险权限"
                                            />
                                        )}
                                        {changed && (
                                            <span
                                                className={cn(
                                                    "shrink-0 text-xs",
                                                    checked
                                                        ? "text-success"
                                                        : "text-destructive",
                                                )}
                                            >
                                                {checked ? "新增" : "移除"}
                                            </span>
                                        )}
                                    </label>,
                                ]
                            })}
                        </div>
                    </fieldset>
                ))}
            </div>
        </section>
    )
}
