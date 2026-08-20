"use client"

import { SearchIcon } from "lucide-react"

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
    BUSINESS_GROUPS,
    PERMISSION_PANEL_TAB_LABEL,
    SYSTEM_GROUPS,
} from "@/features/admin/lib/permission-catalog"

export type { PermissionPanelTab } from "@/features/admin/lib/permission-catalog"

type PermissionOptionsPanelProps = {
    selected: readonly string[]
    /**
     * 变更回调：一次传入完整的新选中列表（组件内已按组批量计算，
     * 调用方只需落状态；不要在循环中逐项回调，否则会基于过期快照互相覆盖）。
     */
    onChange: (next: string[]) => void
    className?: string
}

/**
 * 权限点选面板：业务 / 系统维度 Tab + 权限组下划线 Tab（横向滚动）+ 搜索过滤。
 * 组 Tab 展示组内已选数量；搜索命中时自动定位到含匹配项的组。
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
        setActiveGroup,
        visibleGroups,
        currentGroup,
        selectedCountByTab,
    } = usePermissionPanel(selected)

    return (
        <div className={cn("flex flex-col gap-3", className)}>
            <div className="flex items-center gap-2">
                <InputGroup className="min-w-0 flex-1">
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        type="search"
                        value={keyword}
                        onChange={(e) => setKeyword(e.target.value)}
                        placeholder="搜索权限名称、路径或编码"
                        aria-label="搜索权限"
                    />
                </InputGroup>
                <span className="shrink-0 text-xs text-muted-foreground">
                    已选 <span className="num">{selected.length}</span> 项
                </span>
            </div>

            <Tabs
                value={tab}
                onValueChange={(next) => {
                    if (next === "business" || next === "system") setTab(next)
                }}
            >
                <TabsList variant="line" className="w-full justify-start">
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

            {visibleGroups.length > 0 ? (
                <div
                    role="tablist"
                    aria-label="权限分组"
                    className="flex gap-1 overflow-x-auto border-b border-grid"
                >
                    {visibleGroups.map((group) => {
                        const fullCodes = (
                            (tab === "system"
                                ? SYSTEM_GROUPS
                                : BUSINESS_GROUPS
                            ).find((candidate) => candidate.name === group.name)
                                ?.items ?? []
                        ).map((item) => item.code)
                        const selectedInGroup = fullCodes.filter((code) =>
                            selected.includes(code),
                        ).length
                        const isActive = currentGroup?.name === group.name
                        return (
                            <button
                                key={group.name}
                                type="button"
                                role="tab"
                                aria-selected={isActive}
                                onClick={() => setActiveGroup(group.name)}
                                className={cn(
                                    "relative shrink-0 whitespace-nowrap px-2 py-1.5 text-sm font-medium text-foreground/60 transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    "after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-foreground after:opacity-0 after:transition-opacity",
                                    isActive &&
                                        "text-foreground after:opacity-100",
                                )}
                            >
                                {group.name}
                                {selectedInGroup > 0 ? (
                                    <span className="num ml-1 text-muted-foreground">
                                        {selectedInGroup}
                                    </span>
                                ) : null}
                            </button>
                        )
                    })}
                </div>
            ) : null}

            {currentGroup ? (
                <div className="overflow-hidden rounded-lg border border-border">
                    <div className="flex items-center justify-between gap-2 border-b border-grid bg-muted/40 px-3 py-2">
                        <div className="min-w-0">
                            <div className="text-sm font-medium">
                                {currentGroup.name}
                            </div>
                            <div className="truncate text-xs text-muted-foreground">
                                {currentGroup.description}
                            </div>
                        </div>
                        <label className="flex shrink-0 cursor-pointer items-center gap-1.5 text-xs text-muted-foreground">
                            <Checkbox
                                checked={currentGroup.items.every((item) =>
                                    selected.includes(item.code),
                                )}
                                indeterminate={
                                    currentGroup.items.some((item) =>
                                        selected.includes(item.code),
                                    ) &&
                                    !currentGroup.items.every((item) =>
                                        selected.includes(item.code),
                                    )
                                }
                                onCheckedChange={(next) => {
                                    const add = next === true
                                    onChange(
                                        add
                                            ? [
                                                  ...new Set([
                                                      ...selected,
                                                      ...currentGroup.items.map(
                                                          (item) => item.code,
                                                      ),
                                                  ]),
                                              ]
                                            : selected.filter(
                                                  (code) =>
                                                      !currentGroup.items.some(
                                                          (item) =>
                                                              item.code ===
                                                              code,
                                                      ),
                                              ),
                                    )
                                }}
                            />
                            全选
                        </label>
                    </div>
                    <div className="divide-y divide-grid">
                        {currentGroup.items.map((item) => {
                            const checked = selected.includes(item.code)
                            return (
                                <label
                                    key={item.code}
                                    className="flex cursor-pointer items-center gap-2 px-3 py-2 text-sm hover:bg-muted/40"
                                >
                                    <Checkbox
                                        checked={checked}
                                        onCheckedChange={(next) => {
                                            const add = next === true
                                            onChange(
                                                add
                                                    ? [
                                                          ...new Set([
                                                              ...selected,
                                                              item.code,
                                                          ]),
                                                      ]
                                                    : selected.filter(
                                                          (value) =>
                                                              value !==
                                                              item.code,
                                                      ),
                                            )
                                        }}
                                    />
                                    <span className="min-w-0 flex-1 truncate">
                                        {item.description}
                                    </span>
                                    <span className="shrink-0 font-mono text-xs text-muted-foreground">
                                        {item.method} {item.path}
                                    </span>
                                </label>
                            )
                        })}
                    </div>
                </div>
            ) : (
                <p className="rounded-lg border border-border px-3 py-4 text-center text-xs text-muted-foreground">
                    无匹配权限
                </p>
            )}
        </div>
    )
}
