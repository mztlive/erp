"use client"

import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { hasPermission } from "@/lib/permissions"

/** 组织查询条件使用授权候选名称；失效候选保留且可移除，不回退为全部。 */
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
    const units = canRead && !query.isError ? (query.data?.units ?? []) : []
    const selected = value.split(",").filter(Boolean)
    const available = new Set(units.map((unit) => unit.id))
    const options = [
        ...units.map((unit) => {
            const parent = units.find(
                (candidate) => candidate.id === unit.parent_id,
            )
            return {
                value: unit.id,
                label: `${parent ? `${parent.name} / ` : ""}${unit.name}${unit.enabled ? "" : "（已停用）"}`,
            }
        }),
        ...selected
            .filter((id) => !available.has(id))
            .map((id) => ({ value: id, label: "已选组织（当前不可用）" })),
    ]
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
        <div className="space-y-2">
            <label htmlFor={id} className="text-xs text-muted-foreground">
                {label}
            </label>
            <MultiOptionCombobox
                id={id}
                aria-label={label}
                aria-describedby={message ? `${id}-hint` : undefined}
                value={selected}
                options={options}
                placeholder="搜索组织名称"
                emptyLabel={message ?? "没有符合条件的组织"}
                onValueChange={(ids) => {
                    onChange([...new Set(ids)].sort().join(","))
                    if (!ids.length) onDescendantsChange(false)
                }}
            />
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
            <label
                htmlFor={`${id}-descendants`}
                className="flex items-center gap-2 text-xs text-muted-foreground"
            >
                <Checkbox
                    id={`${id}-descendants`}
                    checked={includeDescendants}
                    disabled={!selected.length}
                    onCheckedChange={(checked) =>
                        onDescendantsChange(checked === true)
                    }
                />
                包含下级
            </label>
        </div>
    )
}
