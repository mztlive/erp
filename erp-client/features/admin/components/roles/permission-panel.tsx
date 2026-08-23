"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"
import { usePermissionPanel } from "@/features/admin/hooks/use-permission-panel"
import {
    PERMISSION_PANEL_TAB_LABEL,
    actionLabel,
    isDangerousAction,
    type PermissionItemOption,
    type PermissionMatrixGroup,
} from "@/features/admin/lib/permission-catalog"

export type { PermissionPanelTab } from "@/features/admin/lib/permission-catalog"

type PermissionOptionsPanelProps = {
    selected: readonly string[]
    /**
     * 变更回调：一次传入完整的新选中列表（组件内已按行/列/组批量计算，
     * 调用方只需落状态；不要在循环中逐项回调，否则会基于过期快照互相覆盖）。
     */
    onChange: (next: string[]) => void
    className?: string
}

/**
 * 权限点选面板：业务 / 系统维度 + 左侧分组目录 + 右侧「对象 × 动作」矩阵。
 *
 * 一个权限组的动作不超过 9 个、对象不超过 6 个，矩阵一屏可比对多行；
 * 行首、列头、组标题分别支持整行 / 整列 / 整组勾选。
 */
export function PermissionOptionsPanel({
    selected,
    onChange,
    className,
}: PermissionOptionsPanelProps) {
    const {
        keyword,
        setKeyword,
        tab,
        setTab,
        activeGroup,
        setActiveGroup,
        visibleGroups,
        progressByGroup,
        selectedCountByTab,
        selectedSet,
    } = usePermissionPanel(selected)

    const scrollRef = React.useRef<HTMLDivElement | null>(null)
    const groupRefs = React.useRef(new Map<string, HTMLElement | null>())

    const toggleCodes = React.useCallback(
        (codes: readonly string[], next: boolean) => {
            if (next) {
                onChange([...new Set([...selected, ...codes])])
                return
            }
            const drop = new Set(codes)
            onChange(selected.filter((code) => !drop.has(code)))
        },
        [onChange, selected],
    )

    /** 点击左侧目录：滚动到该组并立即点亮，不等滚动监听。 */
    const jumpToGroup = (name: string) => {
        setActiveGroup(name)
        const target = groupRefs.current.get(name)
        const container = scrollRef.current
        if (!target || !container) return
        container.scrollTo({
            top: target.offsetTop - container.offsetTop,
            behavior: "smooth",
        })
    }

    /** 滚动定位：以容器顶部为准，点亮最后一个已越过顶部的组。 */
    const handleScroll = () => {
        const container = scrollRef.current
        if (!container) return
        let current: string | null = null
        for (const group of visibleGroups) {
            const node = groupRefs.current.get(group.name)
            if (!node) continue
            if (node.offsetTop - container.offsetTop - container.scrollTop <= 8) {
                current = group.name
            }
        }
        if (current && current !== activeGroup) setActiveGroup(current)
    }

    const progressLookup = React.useMemo(
        () => new Map(progressByGroup.map((item) => [item.name, item])),
        [progressByGroup],
    )

    return (
        <div className={cn("flex flex-col gap-3", className)}>
            <div className="flex flex-wrap items-center gap-2">
                <Tabs
                    value={tab}
                    onValueChange={(next) => {
                        if (next === "business" || next === "system")
                            setTab(next)
                    }}
                >
                    <TabsList variant="line" className="justify-start">
                        <TabsTrigger value="business" className="flex-none">
                            {PERMISSION_PANEL_TAB_LABEL.business}
                            {selectedCountByTab.business > 0 ? (
                                <span className="num text-muted-foreground">
                                    {selectedCountByTab.business}
                                </span>
                            ) : null}
                        </TabsTrigger>
                        <TabsTrigger value="system" className="flex-none">
                            {PERMISSION_PANEL_TAB_LABEL.system}
                            {selectedCountByTab.system > 0 ? (
                                <span className="num text-muted-foreground">
                                    {selectedCountByTab.system}
                                </span>
                            ) : null}
                        </TabsTrigger>
                    </TabsList>
                </Tabs>
                <InputGroup className="min-w-[14rem] flex-1">
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        type="search"
                        value={keyword}
                        onChange={(e) => setKeyword(e.target.value)}
                        placeholder="搜索模块、对象或动作"
                        aria-label="搜索权限"
                    />
                </InputGroup>
            </div>

            <div className="grid min-h-0 gap-3 md:grid-cols-[11rem_minmax(0,1fr)]">
                <nav
                    aria-label="权限分组目录"
                    className="hidden max-h-[32rem] flex-col overflow-y-auto rounded-lg border border-border p-1 md:flex"
                >
                    {visibleGroups.map((group) => {
                        const progress = progressLookup.get(group.name)
                        const isActive = group.name === activeGroup
                        return (
                            <button
                                key={group.name}
                                type="button"
                                aria-current={isActive ? "true" : undefined}
                                onClick={() => jumpToGroup(group.name)}
                                className={cn(
                                    "flex shrink-0 items-baseline justify-between gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                                    isActive
                                        ? "bg-muted font-medium text-foreground"
                                        : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                                )}
                            >
                                <span className="min-w-0 truncate">
                                    {group.name}
                                </span>
                                {progress && progress.selected > 0 ? (
                                    <span className="num shrink-0 text-xs text-muted-foreground">
                                        {progress.selected}
                                    </span>
                                ) : null}
                            </button>
                        )
                    })}
                    {visibleGroups.length === 0 ? (
                        <p className="px-2 py-3 text-xs text-muted-foreground">
                            无匹配分组
                        </p>
                    ) : null}
                </nav>

                <div
                    ref={scrollRef}
                    onScroll={handleScroll}
                    className="flex max-h-[32rem] min-w-0 flex-col gap-4 overflow-y-auto rounded-lg border border-border p-3"
                >
                    {visibleGroups.length === 0 ? (
                        <p className="py-6 text-center text-xs text-muted-foreground">
                            无匹配权限，换个关键词试试
                        </p>
                    ) : (
                        visibleGroups.map((group) => (
                            <PermissionMatrixSection
                                key={group.name}
                                ref={(node) => {
                                    groupRefs.current.set(group.name, node)
                                }}
                                group={group}
                                selectedSet={selectedSet}
                                progress={progressLookup.get(group.name)}
                                onToggle={toggleCodes}
                            />
                        ))
                    )}
                </div>
            </div>
        </div>
    )
}

type PermissionMatrixSectionProps = {
    ref: React.Ref<HTMLElement>
    group: PermissionMatrixGroup
    selectedSet: ReadonlySet<string>
    progress?: { selected: number; total: number }
    onToggle: (codes: readonly string[], next: boolean) => void
}

/** 单个权限组的矩阵：列为动作，行为业务对象。 */
function PermissionMatrixSection({
    ref,
    group,
    selectedSet,
    progress,
    onToggle,
}: PermissionMatrixSectionProps) {
    const baseId = React.useId()
    const groupState = checkedState(group.codes, selectedSet)

    return (
        <section
            ref={ref}
            aria-label={group.name}
            className="shrink-0 overflow-hidden rounded-lg border border-border"
        >
            <div className="flex items-center justify-between gap-2 border-b border-grid bg-muted/40 px-3 py-2">
                <div className="min-w-0">
                    <div className="flex items-center gap-2">
                        <span className="text-sm font-medium">
                            {group.name}
                        </span>
                        {progress && progress.selected > 0 ? (
                            <Badge variant="outline">
                                <span className="num">
                                    {progress.selected}
                                </span>
                                /
                                <span className="num">{progress.total}</span>
                            </Badge>
                        ) : null}
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                        {group.description}
                    </div>
                </div>
                <label
                    htmlFor={`${baseId}-all`}
                    className="flex shrink-0 cursor-pointer items-center gap-1.5 text-xs text-muted-foreground"
                >
                    <Checkbox
                        id={`${baseId}-all`}
                        checked={groupState === "all"}
                        indeterminate={groupState === "some"}
                        onCheckedChange={(next) =>
                            onToggle(group.codes, next === true)
                        }
                        aria-label={`全选 ${group.name}`}
                    />
                    全选
                </label>
            </div>
            <div className="overflow-x-auto">
                <table className="w-full min-w-[32rem] border-collapse text-sm">
                    <thead>
                        <tr className="border-b border-grid">
                            <th
                                scope="col"
                                className="sticky left-0 z-10 w-full bg-card px-3 py-1.5 text-left text-xs font-medium text-muted-foreground"
                            >
                                对象
                            </th>
                            {group.actions.map((action, columnIndex) => {
                                const codes = columnCodes(group, columnIndex)
                                const state = checkedState(codes, selectedSet)
                                const dangerous = isDangerousAction(action)
                                return (
                                    <th
                                        key={action}
                                        scope="col"
                                        className="px-3 py-1.5 text-center align-bottom"
                                    >
                                        <button
                                            type="button"
                                            onClick={() =>
                                                onToggle(
                                                    codes,
                                                    state !== "all",
                                                )
                                            }
                                            title={`勾选或取消整列：${actionLabel(action)}`}
                                            className={cn(
                                                "whitespace-nowrap rounded px-1 text-xs font-medium transition-colors hover:text-foreground",
                                                dangerous
                                                    ? "text-destructive"
                                                    : "text-muted-foreground",
                                            )}
                                        >
                                            {actionLabel(action)}
                                        </button>
                                    </th>
                                )
                            })}
                        </tr>
                    </thead>
                    <tbody>
                        {group.rows.map((row) => {
                            const rowState = checkedState(row.codes, selectedSet)
                            return (
                                <tr
                                    key={row.resource}
                                    className="border-b border-grid last:border-0 hover:bg-muted/30"
                                >
                                    <th
                                        scope="row"
                                        className="sticky left-0 z-10 w-full bg-card px-3 py-1.5 text-left font-normal"
                                    >
                                        <label
                                            htmlFor={`${baseId}-${row.resource}`}
                                            className="flex cursor-pointer items-center gap-2"
                                        >
                                            <Checkbox
                                                id={`${baseId}-${row.resource}`}
                                                checked={rowState === "all"}
                                                indeterminate={
                                                    rowState === "some"
                                                }
                                                onCheckedChange={(next) =>
                                                    onToggle(
                                                        row.codes,
                                                        next === true,
                                                    )
                                                }
                                                aria-label={`全选 ${row.label}`}
                                            />
                                            <span className="whitespace-nowrap">
                                                {row.label}
                                            </span>
                                        </label>
                                    </th>
                                    {row.cells.map((cell, index) => (
                                        <td
                                            key={group.actions[index]}
                                            className="px-3 py-1.5 text-center"
                                        >
                                            {cell ? (
                                                <PermissionCell
                                                    item={cell}
                                                    rowLabel={row.label}
                                                    checked={selectedSet.has(
                                                        cell.code,
                                                    )}
                                                    onCheckedChange={(next) =>
                                                        onToggle(
                                                            [cell.code],
                                                            next,
                                                        )
                                                    }
                                                />
                                            ) : (
                                                <span
                                                    aria-hidden="true"
                                                    className="text-muted-foreground/40"
                                                >
                                                    —
                                                </span>
                                            )}
                                        </td>
                                    ))}
                                </tr>
                            )
                        })}
                    </tbody>
                </table>
            </div>
        </section>
    )
}

function PermissionCell({
    item,
    rowLabel,
    checked,
    onCheckedChange,
}: {
    item: PermissionItemOption
    rowLabel: string
    checked: boolean
    onCheckedChange: (next: boolean) => void
}) {
    const endpoints = item.endpoints
        .map((endpoint) => `${endpoint.method} ${endpoint.path}`)
        .join("\n")
    return (
        <span
            className="inline-flex cursor-pointer items-center justify-center"
            title={`${item.description}\n${endpoints}`}
        >
            <Checkbox
                checked={checked}
                onCheckedChange={(next) => onCheckedChange(next === true)}
                aria-label={`${rowLabel} · ${actionLabel(item.action)}`}
            />
        </span>
    )
}

/** 一组编码在当前选中集合里的状态。 */
function checkedState(
    codes: readonly string[],
    selectedSet: ReadonlySet<string>,
): "none" | "some" | "all" {
    if (codes.length === 0) return "none"
    let hit = 0
    for (const code of codes) if (selectedSet.has(code)) hit += 1
    if (hit === 0) return "none"
    return hit === codes.length ? "all" : "some"
}

function columnCodes(
    group: PermissionMatrixGroup,
    columnIndex: number,
): readonly string[] {
    const codes: string[] = []
    for (const row of group.rows) {
        const cell = row.cells[columnIndex]
        if (cell) codes.push(cell.code)
    }
    return codes
}

/** 「全选」按钮组：整个维度一键勾选 / 清空，供表单顶部快捷操作使用。 */
export function PermissionBulkActions({
    codes,
    selected,
    onChange,
}: {
    codes: readonly string[]
    selected: readonly string[]
    onChange: (next: string[]) => void
}) {
    const selectedSet = new Set(selected)
    const state = checkedState(codes, selectedSet)
    return (
        <Button
            type="button"
            size="xs"
            variant="ghost"
            onClick={() => {
                if (state === "all") {
                    const drop = new Set(codes)
                    onChange(selected.filter((code) => !drop.has(code)))
                    return
                }
                onChange([...new Set([...selected, ...codes])])
            }}
        >
            {state === "all" ? "取消全选" : "全选"}
        </Button>
    )
}
