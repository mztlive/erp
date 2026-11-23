"use client"

import {
    ArrowRightLeftIcon,
    SearchIcon,
    ShieldAlertIcon,
    XIcon,
} from "lucide-react"

import { detailWorkspaceStyles, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import {
    usePermissionPanel,
    type FocusedPermissionGroup,
} from "@/features/admin/hooks/use-permission-panel"
import {
    actionLabel,
    PERMISSION_BY_CODE,
    permissionGroupSegment,
    permissionLabel,
    type PermissionMatrixGroup,
} from "@/features/admin/lib/permission-catalog"
import {
    PERMISSION_VIEWS,
    permissionArea,
    type PermissionView,
} from "@/features/admin/lib/permission-editor"
import { selectResourceActions } from "@/features/admin/lib/role-workbench"

export type { PermissionPanelTab } from "@/features/admin/lib/permission-catalog"

type PermissionOptionsPanelProps = {
    selected: readonly string[]
    initial: readonly string[]
    preservedCodes?: readonly string[]
    onChange: (next: string[]) => void
    view: PermissionView
    onViewChange: (view: PermissionView) => void
    focusedGroup?: FocusedPermissionGroup | null
    disabled?: boolean
    className?: string
    id?: string
}

const VIEW_ORDER: readonly PermissionView[] = [
    "all",
    "selected",
    "dangerous",
    "changed",
]

const ALL_KNOWN_CODES = [...PERMISSION_BY_CODE.keys()]

/** 模块目录只切换当前编辑区域；筛选与切换均不改动表单内的授权。 */
export function PermissionOptionsPanel({
    selected,
    initial,
    preservedCodes = [],
    onChange,
    view,
    onViewChange,
    focusedGroup = null,
    disabled = false,
    className,
    id = "governance-admin-permission-panel",
}: PermissionOptionsPanelProps) {
    const effectiveSelected = ALL_KNOWN_CODES.filter((code) =>
        hasPermission(selected, code),
    )
    const effectiveInitial = ALL_KNOWN_CODES.filter((code) =>
        hasPermission(initial, code),
    )
    const crossResourceCodes = selected.filter((code) => code.startsWith("*:"))
    const panel = usePermissionPanel(
        effectiveSelected,
        effectiveInitial,
        view,
        focusedGroup,
    )

    const toggleCodes = (codes: readonly string[], next: boolean) => {
        if (disabled) return
        const editableCodes = codes.filter(
            (code) => !hasPermission(crossResourceCodes, code),
        )
        let current = [...selected]
        for (const resource of new Set(
            editableCodes.map((code) => code.split(":")[0]),
        )) {
            current = selectResourceActions(
                current,
                resource,
                ALL_KNOWN_CODES.filter((code) =>
                    code.startsWith(`${resource}:`),
                ),
            )
        }
        const drop = new Set(editableCodes)
        onChange(
            next
                ? [...new Set([...current, ...editableCodes])]
                : current.filter((code) => !drop.has(code)),
        )
    }

    const areas = [
        ...new Set(
            panel.visibleGroups.map((group) => permissionArea(group.name)),
        ),
    ]

    const progressMap = new Map(
        panel.progressByGroup.map((item) => [item.name, item] as const),
    )

    const hasActiveFilter = view !== "all" || panel.keyword.trim().length > 0

    return (
        <section
            aria-label="操作权限"
            className={cn("flex min-h-0 flex-1 flex-col", className)}
        >
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2.5 border-b border-border/60 pb-3.5">
                <div className="flex min-w-0 flex-wrap items-center gap-3">
                    <div
                        role="tablist"
                        aria-label="权限分类"
                        className={detailWorkspaceStyles.subTabs}
                    >
                        {(
                            [
                                {
                                    key: "business" as const,
                                    label: "业务权限",
                                    count: panel.countsByTab.business,
                                },
                                {
                                    key: "system" as const,
                                    label: "系统权限",
                                    count: panel.countsByTab.system,
                                },
                            ] as const
                        ).map((item) => {
                            const active = panel.tab === item.key
                            return (
                                <button
                                    key={item.key}
                                    id={`${id}-tab-${item.key}`}
                                    type="button"
                                    role="tab"
                                    aria-selected={active}
                                    data-active={active ? "true" : undefined}
                                    className={detailWorkspaceStyles.subTab}
                                    onClick={() => panel.setTab(item.key)}
                                >
                                    <span>{item.label}</span>
                                    <span
                                        className={cn(
                                            "num rounded-full px-1.5 py-0 text-tiny",
                                            active
                                                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                                                : "bg-muted text-muted-foreground",
                                        )}
                                    >
                                        {item.count}
                                    </span>
                                </button>
                            )
                        })}
                    </div>

                    <span
                        aria-hidden="true"
                        className="hidden h-4 w-px bg-border/70 sm:block"
                    />

                    <div
                        role="group"
                        aria-label="权限显示范围"
                        className="flex flex-wrap items-center gap-1.5"
                    >
                        {VIEW_ORDER.map((option) => {
                            const active = view === option
                            const count = panel.viewCounts[option]
                            if (
                                option === "changed" &&
                                count === 0 &&
                                !active
                            ) {
                                return null
                            }
                            return (
                                <button
                                    key={option}
                                    id={`${id}-view-${option}`}
                                    type="button"
                                    aria-pressed={active}
                                    onClick={() => onViewChange(option)}
                                    className={cn(
                                        "inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-primary",
                                        active
                                            ? option === "dangerous"
                                                ? "bg-destructive-soft font-medium text-destructive-soft-foreground"
                                                : option === "changed"
                                                  ? "bg-warning-soft font-medium text-warning-soft-foreground"
                                                  : "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                            : "bg-muted/65 text-muted-foreground hover:bg-muted hover:text-foreground",
                                    )}
                                >
                                    {option === "changed" && count > 0 ? (
                                        <span
                                            aria-hidden="true"
                                            className="size-1.5 rounded-full bg-warning"
                                        />
                                    ) : null}
                                    <span>{PERMISSION_VIEWS[option]}</span>
                                    {option !== "all" ? (
                                        <span className="num text-tiny opacity-80">
                                            {count}
                                        </span>
                                    ) : null}
                                </button>
                            )
                        })}
                    </div>
                </div>

                <div className="flex w-full min-w-0 items-center gap-2 sm:w-auto">
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
                    {hasActiveFilter ? (
                        <Button
                            id={`${id}-clear-toolbar-filters`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={() => {
                                panel.setKeyword("")
                                onViewChange("all")
                            }}
                        >
                            <XIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            重置
                        </Button>
                    ) : null}
                </div>
            </div>

            <div className="grid min-h-0 flex-1 md:grid-cols-[14rem_minmax(0,1fr)]">
                <nav
                    aria-label="权限模块"
                    className="hidden overflow-y-auto overscroll-contain border-r border-border/60 py-3.5 pr-3 md:block"
                >
                    {areas.map((area) => (
                        <div key={area} className="mb-5 last:mb-0">
                            <p className="mb-1.5 px-2.5 text-xs font-medium text-muted-foreground">
                                {area}
                            </p>
                            <ul className="space-y-0.5">
                                {panel.visibleGroups
                                    .filter(
                                        (group) =>
                                            permissionArea(group.name) === area,
                                    )
                                    .map((group) => {
                                        const progress = progressMap.get(
                                            group.name,
                                        )
                                        const selectedCount =
                                            progress?.selected ?? 0
                                        const totalCount =
                                            progress?.total ??
                                            group.codes.length
                                        const isActive =
                                            panel.activeGroup?.name ===
                                            group.name
                                        const isFull =
                                            totalCount > 0 &&
                                            selectedCount === totalCount
                                        return (
                                            <li key={group.name}>
                                                <button
                                                    id={`${id}-group-${permissionGroupSegment(group.name)}-nav`}
                                                    type="button"
                                                    aria-current={
                                                        isActive
                                                            ? "true"
                                                            : undefined
                                                    }
                                                    aria-controls={`${id}-content`}
                                                    onClick={() =>
                                                        panel.setActiveGroup(
                                                            group.name,
                                                        )
                                                    }
                                                    className={cn(
                                                        "relative flex min-h-9 w-full cursor-pointer items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-body-compact transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary",
                                                        isActive
                                                            ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                                            : "text-foreground/80 hover:bg-muted/55 hover:text-foreground",
                                                    )}
                                                >
                                                    {isActive ? (
                                                        <span
                                                            aria-hidden="true"
                                                            className="absolute inset-y-1.5 left-0 w-0.5 rounded-full bg-primary"
                                                        />
                                                    ) : null}
                                                    <span className="flex min-w-0 items-center gap-1.5">
                                                        <span className="truncate">
                                                            {group.name}
                                                        </span>
                                                        {progress?.hasChanges ? (
                                                            <span
                                                                aria-label="有未保存修改"
                                                                className="size-1.5 shrink-0 rounded-full bg-warning"
                                                            />
                                                        ) : null}
                                                    </span>
                                                    <span
                                                        className={cn(
                                                            "num shrink-0 text-tiny",
                                                            selectedCount === 0
                                                                ? "text-muted-foreground/55"
                                                                : isActive
                                                                  ? "font-medium text-sidebar-accent-foreground"
                                                                  : isFull
                                                                    ? "font-medium text-primary"
                                                                    : "text-muted-foreground",
                                                        )}
                                                        aria-label={`已勾选 ${selectedCount} / ${totalCount} 项`}
                                                    >
                                                        {selectedCount}/
                                                        {totalCount}
                                                    </span>
                                                </button>
                                            </li>
                                        )
                                    })}
                            </ul>
                        </div>
                    ))}
                    {areas.length === 0 ? (
                        <p className="px-2.5 text-xs text-muted-foreground">
                            没有匹配模块
                        </p>
                    ) : null}
                </nav>

                <div
                    key={panel.activeGroup?.name ?? "empty"}
                    className="min-h-0 overflow-y-auto overscroll-contain py-4 md:pl-6"
                    id={`${id}-content`}
                >
                    {panel.visibleGroups.length > 0 ? (
                        <OptionCombobox
                            id={`${id}-module`}
                            aria-label="当前权限模块"
                            value={panel.activeGroup?.name ?? ""}
                            onValueChange={(value) =>
                                value && panel.setActiveGroup(value)
                            }
                            options={panel.visibleGroups.map((group) => ({
                                value: group.name,
                                label: group.name,
                            }))}
                            getOptionId={(value) =>
                                `${id}-module-${permissionGroupSegment(value)}`
                            }
                            allowClear={false}
                            className="mb-4 w-full md:hidden"
                        />
                    ) : null}

                    {panel.activeGroup ? (
                        <PermissionSection
                            key={panel.activeGroup.name}
                            id={`${id}-group-${permissionGroupSegment(panel.activeGroup.name)}`}
                            group={panel.activeGroup}
                            selectedSet={panel.selectedSet}
                            initial={effectiveInitial}
                            preservedCodes={preservedCodes}
                            crossResourceCodes={crossResourceCodes}
                            filtered={hasActiveFilter}
                            disabled={disabled}
                            onToggle={toggleCodes}
                        />
                    ) : (
                        <div
                            className="flex min-h-48 flex-col items-center justify-center gap-3 py-8 text-center text-body-compact text-muted-foreground"
                            role="status"
                        >
                            <p>
                                {view === "changed"
                                    ? "当前分类暂无匹配的权限变更"
                                    : panel.otherTabVisibleCount > 0
                                      ? `当前分类没有匹配权限，但在「${panel.otherTab === "system" ? "系统权限" : "业务权限"}」中找到 ${panel.otherTabVisibleCount} 项`
                                      : "当前分类没有匹配权限"}
                            </p>
                            <div className="flex flex-wrap items-center justify-center gap-2">
                                {panel.otherTabVisibleCount > 0 ? (
                                    <Button
                                        id={`${id}-switch-other-tab`}
                                        type="button"
                                        variant="default"
                                        size="sm"
                                        onClick={() =>
                                            panel.setTab(panel.otherTab)
                                        }
                                    >
                                        <ArrowRightLeftIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        切换到
                                        {panel.otherTab === "system"
                                            ? "系统权限"
                                            : "业务权限"}
                                    </Button>
                                ) : null}
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
    crossResourceCodes,
    filtered,
    disabled,
    onToggle,
    id,
}: {
    group: PermissionMatrixGroup
    selectedSet: ReadonlySet<string>
    initial: readonly string[]
    preservedCodes: readonly string[]
    crossResourceCodes: readonly string[]
    filtered: boolean
    disabled: boolean
    onToggle: (codes: readonly string[], next: boolean) => void
    id: string
}) {
    const specialPermissions = preservedCodes.filter(
        (code) =>
            code.startsWith("*:") ||
            group.rows.some((row) => code.startsWith(`${row.resource}:`)),
    )
    const initialSet = new Set(initial)
    const editableCodes = group.codes.filter(
        (code) => !hasPermission(crossResourceCodes, code),
    )
    const selectedCount = group.codes.filter((code) =>
        selectedSet.has(code),
    ).length
    const progressPercent =
        group.codes.length > 0
            ? Math.round((selectedCount / group.codes.length) * 100)
            : 0
    const allEditableSelected =
        editableCodes.length > 0 &&
        editableCodes.every((code) => selectedSet.has(code))
    const anyEditableSelected = editableCodes.some((code) =>
        selectedSet.has(code),
    )

    return (
        <section aria-label={`${group.name}权限`} className="space-y-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2.5">
                        <h3 className="text-section-title font-semibold tracking-tight text-foreground">
                            {group.name}
                        </h3>
                        <span className="num text-xs text-muted-foreground">
                            {filtered ? "匹配项" : "本模块"}已勾选{" "}
                            <span className="font-medium text-foreground">
                                {selectedCount}
                            </span>{" "}
                            / {group.codes.length}
                        </span>
                        <span
                            aria-hidden="true"
                            className="hidden h-1.5 w-20 overflow-hidden rounded-full bg-muted sm:inline-block"
                        >
                            <span
                                className="block h-full rounded-full bg-primary transition-[width] duration-150"
                                style={{ width: `${progressPercent}%` }}
                            />
                        </span>
                    </div>
                    <p className="text-xs text-muted-foreground">
                        {group.description}
                    </p>
                </div>

                {!disabled ? (
                    <div className="flex shrink-0 items-center gap-1.5">
                        <Button
                            id={`${id}-select-all`}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.subtleButton}
                            disabled={allEditableSelected}
                            onClick={() => onToggle(group.codes, true)}
                        >
                            {filtered ? "选择匹配项" : "选择本模块全部"}
                        </Button>
                        <Button
                            id={`${id}-clear`}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            disabled={!anyEditableSelected}
                            onClick={() => onToggle(group.codes, false)}
                        >
                            {filtered ? "清除匹配项" : "清除本模块"}
                        </Button>
                    </div>
                ) : null}
            </div>

            {specialPermissions.length > 0 ? (
                <p className="rounded-md border border-border/60 bg-surface-sunken px-3 py-2 text-xs text-muted-foreground">
                    包含整模块全选授权：
                    {specialPermissions.map(permissionLabel).join("、")}
                    。如改为按单项操作授权，系统将仅保留本次勾选的操作（后续新增功能需手动勾选）；跨模块全选授权的操作不可单独取消。
                </p>
            ) : null}

            <div className="divide-y divide-border/60 border-y border-border/60">
                {group.rows.map((row) => {
                    const rowEditableCodes = row.codes.filter(
                        (code) => !hasPermission(crossResourceCodes, code),
                    )
                    const rowSelectedCount = row.codes.filter((code) =>
                        selectedSet.has(code),
                    ).length
                    const rowAllSelected =
                        rowEditableCodes.length > 0 &&
                        rowEditableCodes.every((code) => selectedSet.has(code))
                    const hideStandaloneLegend =
                        group.rows.length === 1 && row.label === group.name
                    const showRowQuickToggle =
                        !disabled &&
                        group.rows.length > 1 &&
                        rowEditableCodes.length > 1

                    return (
                        <fieldset key={row.resource} className="min-w-0 py-4">
                            <legend className="sr-only">{row.label}</legend>
                            {!hideStandaloneLegend ? (
                                <div className="mb-2.5 flex flex-wrap items-center justify-between gap-2">
                                    <div className="flex items-baseline gap-2">
                                        <span className="font-medium text-foreground">
                                            {row.label}
                                        </span>
                                        <span className="num text-xs text-muted-foreground">
                                            {rowSelectedCount} /{" "}
                                            {row.codes.length}
                                        </span>
                                    </div>
                                    {showRowQuickToggle ? (
                                        <Button
                                            id={`${id}-row-${toAutomationIdSegment(row.resource)}-toggle`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                            aria-label={`${rowAllSelected ? "清除" : "全选"}${row.label}`}
                                            onClick={() =>
                                                onToggle(
                                                    rowEditableCodes,
                                                    !rowAllSelected,
                                                )
                                            }
                                        >
                                            {rowAllSelected
                                                ? "清空本行"
                                                : "全选本行"}
                                        </Button>
                                    ) : null}
                                </div>
                            ) : null}

                            <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                                {row.cells.flatMap((item) => {
                                    if (!item) return []
                                    const checked = selectedSet.has(item.code)
                                    const readOnly =
                                        disabled ||
                                        hasPermission(
                                            crossResourceCodes,
                                            item.code,
                                        )
                                    const changed =
                                        checked !== initialSet.has(item.code)
                                    const checkboxId = `${id}-cell-${toAutomationIdSegment(item.code)}`
                                    return [
                                        <label
                                            key={item.code}
                                            htmlFor={checkboxId}
                                            title={[
                                                item.description,
                                                ...(
                                                    item.relatedGroups ?? []
                                                ).map(
                                                    (name) => `同时用于${name}`,
                                                ),
                                            ].join("；")}
                                            className={cn(
                                                "flex min-h-10 items-center gap-2.5 rounded-md border px-3 py-2 text-body-compact transition-colors",
                                                !readOnly && "cursor-pointer",
                                                checked
                                                    ? item.dangerous
                                                        ? "border-destructive-border bg-destructive-soft/45 text-foreground hover:bg-destructive-soft/65"
                                                        : "border-primary/35 bg-sidebar-accent/45 text-foreground hover:bg-sidebar-accent/65"
                                                    : changed
                                                      ? "border-dashed border-destructive-border/80 bg-destructive-soft/20 text-muted-foreground hover:bg-destructive-soft/35"
                                                      : "border-border/60 bg-card text-foreground/85 hover:border-border hover:bg-muted/35",
                                                readOnly &&
                                                    "cursor-default opacity-70",
                                            )}
                                        >
                                            <NativeCheckbox
                                                id={checkboxId}
                                                checked={checked}
                                                disabled={readOnly}
                                                aria-label={`${row.label} · ${actionLabel(item.action)}`}
                                                onCheckedChange={(
                                                    nextChecked,
                                                ) =>
                                                    onToggle(
                                                        [item.code],
                                                        nextChecked === true,
                                                    )
                                                }
                                            />
                                            <span
                                                className={cn(
                                                    "min-w-0 flex-1 truncate",
                                                    checked && "font-medium",
                                                )}
                                            >
                                                {actionLabel(item.action)}
                                            </span>
                                            {item.dangerous ? (
                                                <ShieldAlertIcon
                                                    className="size-3.5 shrink-0 text-destructive"
                                                    aria-label="高风险权限"
                                                />
                                            ) : null}
                                            {changed ? (
                                                <span
                                                    className={cn(
                                                        "shrink-0 rounded-sm px-1.5 py-0 text-tiny font-medium",
                                                        checked
                                                            ? "bg-success-soft text-success-soft-foreground"
                                                            : "bg-destructive-soft text-destructive-soft-foreground",
                                                    )}
                                                >
                                                    {checked ? "新增" : "移除"}
                                                </span>
                                            ) : null}
                                        </label>,
                                    ]
                                })}
                            </div>
                        </fieldset>
                    )
                })}
            </div>
        </section>
    )
}
