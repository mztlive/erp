"use client"

import { useMemo } from "react"

import {
    MultiTreeCombobox,
    type TreeComboboxNode,
} from "@/components/business/tree-combobox"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import type { OrgUnit } from "@/features/organization/types"
import { hasPermission } from "@/lib/permissions"

function buildUnitNodes(units: readonly OrgUnit[]): TreeComboboxNode[] {
    const known = new Set(units.map((unit) => unit.id))
    const childrenByParent = new Map<string | null, OrgUnit[]>()
    for (const unit of units) {
        const parentId =
            unit.parent_id &&
            known.has(unit.parent_id) &&
            unit.parent_id !== unit.id
                ? unit.parent_id
                : null
        const siblings = childrenByParent.get(parentId) ?? []
        siblings.push(unit)
        childrenByParent.set(parentId, siblings)
    }
    const build = (
        unit: OrgUnit,
        seen: ReadonlySet<string>,
    ): TreeComboboxNode => {
        const label = unit.enabled ? unit.name : `${unit.name}（已停用）`
        if (seen.has(unit.id)) return { id: unit.id, label, children: [] }
        const next = new Set(seen).add(unit.id)
        return {
            id: unit.id,
            label,
            children: (childrenByParent.get(unit.id) ?? []).map((child) =>
                build(child, next),
            ),
        }
    }
    return (childrenByParent.get(null) ?? []).map((unit) =>
        build(unit, new Set()),
    )
}

/** 组织查询条件按组织树选择；失效候选保留且可移除，不回退为全部。 */
export function OrganizationUnitFilter({
    id,
    label = "组织",
    value,
    onChange,
    includeDescendants,
    onDescendantsChange,
}: {
    id: string
    label?: string
    value: string
    onChange: (value: string) => void
    includeDescendants: boolean
    onDescendantsChange: (value: boolean) => void
}) {
    const profile = useAccountProfileQuery()
    const canRead = hasPermission(profile.data?.permissions, "org_unit:list")
    const query = useOrganizationStateQuery(canRead)
    const sourceUnits =
        canRead && !query.isError ? query.data?.units : undefined
    const units = useMemo(() => sourceUnits ?? [], [sourceUnits])
    const selected = useMemo(() => value.split(",").filter(Boolean), [value])
    const nodes = useMemo(() => {
        const tree = buildUnitNodes(units)
        const known = new Set(units.map((unit) => unit.id))
        const unavailable = selected
            .filter((unitId) => !known.has(unitId))
            .map((unitId): TreeComboboxNode => ({
                id: unitId,
                label: "已选组织（当前不可用）",
                children: [],
            }))
        return unavailable.length > 0 ? [...tree, ...unavailable] : tree
    }, [selected, units])
    const message = profile.isPending
        ? "正在加载组织权限…"
        : !canRead
          ? "当前账号无组织查询权限"
          : query.isError
            ? "组织加载失败，请重试"
            : query.isPending
              ? "正在加载组织…"
              : query.data?.emptyReason === "no_scope"
                ? "当前账号无可选组织范围"
                : undefined

    return (
        <div className="min-w-0 space-y-1.5">
            <label htmlFor={id} className="text-xs text-muted-foreground">
                {label}
            </label>
            <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
                <MultiTreeCombobox
                    id={id}
                    label={label}
                    aria-describedby={message ? `${id}-hint` : undefined}
                    className="min-w-0 [&_[data-slot=combobox-chips]]:min-h-control [&_[data-slot=combobox-chips]]:rounded-lg [&_[data-slot=combobox-chip]]:max-w-full [&_[data-slot=combobox-chip-remove]]:shrink-0"
                    nodes={nodes}
                    value={selected}
                    placeholder="搜索组织名称"
                    emptyLabel={message ?? "没有符合条件的组织"}
                    onValueChange={(ids) => {
                        const next = [...new Set(ids)].sort()
                        onChange(next.join(","))
                        if (!next.length) onDescendantsChange(false)
                    }}
                />
                <label
                    htmlFor={`${id}-descendants`}
                    className="flex h-control items-center gap-1.5 whitespace-nowrap text-xs text-muted-foreground has-[:disabled]:cursor-not-allowed"
                >
                    <Checkbox
                        id={`${id}-descendants`}
                        checked={includeDescendants}
                        disabled={!selected.length}
                        className="rounded-sm"
                        onCheckedChange={(checked) =>
                            onDescendantsChange(checked === true)
                        }
                    />
                    包含下级
                </label>
            </div>
            {message && (
                <div
                    id={`${id}-hint`}
                    role="status"
                    className="text-xs text-muted-foreground"
                >
                    {message}
                    {canRead && query.isError && (
                        <Button
                            id={`${id}-retry`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    )}
                </div>
            )}
        </div>
    )
}
