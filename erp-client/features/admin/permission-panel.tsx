"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { Checkbox } from "@/components/ui/checkbox"
import {
  InputGroupInput,
} from "@/components/ui/input-group"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"
import { PERMISSION_GROUPS } from "@/lib/permissions.generated"

type PermissionItemOption = {
  /** 后端权限字符串（resource:action）。 */
  code: string
  description: string
  method: string
  path: string
}

type PermissionGroupOption = {
  name: string
  description: string
  items: PermissionItemOption[]
}

/** 归属「系统」维度的权限组名（平台 / 治理 / 访问控制类）；其余归「业务」。 */
const SYSTEM_GROUP_NAMES = new Set([
  "账号管理",
  "角色管理",
  "系统审计",
  "来源注册",
  "单据注册",
  "统一待办",
  "批量任务",
  "文件资产",
  "权限与审计",
  "集成治理",
])

export type PermissionPanelTab = "business" | "system"

export const PERMISSION_PANEL_TAB_LABEL: Record<PermissionPanelTab, string> = {
  business: "业务",
  system: "系统",
}

function isSystemGroup(name: string): boolean {
  return SYSTEM_GROUP_NAMES.has(name)
}

/** 权限目录：由 build.rs 生成的 PERMISSION_GROUPS 派生，按分组与维度归类。 */
const PERMISSION_CATALOG: readonly PermissionGroupOption[] =
  PERMISSION_GROUPS.map((group) => ({
    name: group.name,
    description: group.description,
    items: group.permissions.map((permission) => ({
      code: `${permission.permission.resource}:${permission.permission.action}`,
      description: permission.description,
      method: permission.method,
      path: permission.path,
    })),
  }))

const BUSINESS_GROUPS: readonly PermissionGroupOption[] =
  PERMISSION_CATALOG.filter((group) => !isSystemGroup(group.name))
const SYSTEM_GROUPS: readonly PermissionGroupOption[] =
  PERMISSION_CATALOG.filter((group) => isSystemGroup(group.name))

/** 权限字符串 → 描述文案；目录未收录时回落为权限码本身。 */
export function permissionDescription(code: string): string {
  for (const group of PERMISSION_CATALOG) {
    const hit = group.items.find((item) => item.code === code)
    if (hit) return hit.description
  }
  return code
}

function matchesKeyword(item: PermissionItemOption, q: string): boolean {
  return [item.code, item.description, item.path]
    .join(" ")
    .toLowerCase()
    .includes(q)
}

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
  const [keyword, setKeyword] = React.useState("")
  const [tab, setTab] = React.useState<PermissionPanelTab>("business")
  const [activeGroup, setActiveGroup] = React.useState<string | null>(null)
  const q = keyword.trim().toLowerCase()

  const sourceGroups = tab === "system" ? SYSTEM_GROUPS : BUSINESS_GROUPS

  const visibleGroups = React.useMemo(() => {
    if (!q) return sourceGroups
    return sourceGroups
      .map((group) => ({
        ...group,
        items: group.items.filter((item) => matchesKeyword(item, q)),
      }))
      .filter((group) => group.items.length > 0)
  }, [q, sourceGroups])

  // 搜索或切换维度后，把当前组定位到第一个仍可见的组
  React.useEffect(() => {
    if (!visibleGroups.some((group) => group.name === activeGroup)) {
      setActiveGroup(visibleGroups[0]?.name ?? null)
    }
  }, [visibleGroups, activeGroup])

  const currentGroup =
    visibleGroups.find((group) => group.name === activeGroup) ?? null

  const selectedCountByTab = React.useMemo(() => {
    const counts: Record<PermissionPanelTab, number> = {
      business: 0,
      system: 0,
    }
    for (const code of selected) {
      const group = PERMISSION_CATALOG.find((candidate) =>
        candidate.items.some((item) => item.code === code)
      )
      if (!group) continue
      counts[isSystemGroup(group.name) ? "system" : "business"] += 1
    }
    return counts
  }, [selected])

  return (
    <div className={cn("space-y-2", className)}>
      <div className="flex items-center justify-between gap-2">
        <div className="relative flex-1">
          <SearchIcon
            aria-hidden="true"
            className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
          />
          <InputGroupInput
            type="search"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            placeholder="搜索权限名称、路径或编码"
            aria-label="搜索权限"
            className="h-8 pl-8 text-sm"
          />
        </div>
        <span className="shrink-0 text-xs text-muted-foreground">
          已选 {selected.length} 项
        </span>
      </div>

      <Tabs
        value={tab}
        onValueChange={(next) => {
          if (next === "business" || next === "system") setTab(next)
        }}
      >
        <TabsList className="h-8">
          <TabsTrigger value="business" className="text-xs">
            {PERMISSION_PANEL_TAB_LABEL.business}
            {selectedCountByTab.business > 0 ? (
              <span className="num ml-1 text-muted-foreground">
                {selectedCountByTab.business}
              </span>
            ) : null}
          </TabsTrigger>
          <TabsTrigger value="system" className="text-xs">
            {PERMISSION_PANEL_TAB_LABEL.system}
            {selectedCountByTab.system > 0 ? (
              <span className="num ml-1 text-muted-foreground">
                {selectedCountByTab.system}
              </span>
            ) : null}
          </TabsTrigger>
        </TabsList>
      </Tabs>

      {/* 权限组下划线 Tab（横向滚动） */}
      {visibleGroups.length > 0 ? (
        <div
          role="tablist"
          aria-label="权限分组"
          className="flex gap-1 overflow-x-auto border-b pb-0"
        >
          {visibleGroups.map((group) => {
            const fullCodes = (
              (tab === "system" ? SYSTEM_GROUPS : BUSINESS_GROUPS).find(
                (candidate) => candidate.name === group.name
              )?.items ?? []
            ).map((item) => item.code)
            const selectedInGroup = fullCodes.filter((code) =>
              selected.includes(code)
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
                  "relative shrink-0 whitespace-nowrap px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  "after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-full after:bg-foreground after:opacity-0 after:transition-opacity",
                  isActive &&
                    "text-foreground after:opacity-100"
                )}
              >
                {group.name}
                {selectedInGroup > 0 ? (
                  <span className="num ml-1">{selectedInGroup}</span>
                ) : null}
              </button>
            )
          })}
        </div>
      ) : null}

      {currentGroup ? (
        <div className="rounded-lg border">
          <div className="flex items-center justify-between gap-2 border-b bg-muted/40 px-3 py-2">
            <div className="min-w-0">
              <div className="text-sm font-medium">{currentGroup.name}</div>
              <div className="truncate text-xs text-muted-foreground">
                {currentGroup.description}
              </div>
            </div>
            <label className="flex shrink-0 cursor-pointer items-center gap-1.5 text-xs text-muted-foreground">
              <Checkbox
                checked={currentGroup.items.every((item) =>
                  selected.includes(item.code)
                )}
                indeterminate={
                  currentGroup.items.some((item) =>
                    selected.includes(item.code)
                  ) &&
                  !currentGroup.items.every((item) =>
                    selected.includes(item.code)
                  )
                }
                onCheckedChange={(next) => {
                  const add = next === true
                  onChange(
                    add
                      ? [
                          ...new Set([
                            ...selected,
                            ...currentGroup.items.map((item) => item.code),
                          ]),
                        ]
                      : selected.filter(
                          (code) =>
                            !currentGroup.items.some(
                              (item) => item.code === code
                            )
                        )
                  )
                }}
              />
              全选
            </label>
          </div>
          <div className="divide-y">
            {currentGroup.items.map((item) => {
              const checked = selected.includes(item.code)
              return (
                <label
                  key={item.code}
                  className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-sm hover:bg-accent"
                >
                  <Checkbox
                    checked={checked}
                    onCheckedChange={(next) => {
                      const add = next === true
                      onChange(
                        add
                          ? [...new Set([...selected, item.code])]
                          : selected.filter((value) => value !== item.code)
                      )
                    }}
                  />
                  <span className="min-w-0 flex-1 truncate">
                    {item.description}
                  </span>
                  <span className="shrink-0 font-mono text-2xs text-muted-foreground">
                    {item.method} {item.path}
                  </span>
                </label>
              )
            })}
          </div>
        </div>
      ) : (
        <p className="rounded-lg border px-3 py-4 text-center text-xs text-muted-foreground">
          无匹配权限
        </p>
      )}
    </div>
  )
}
