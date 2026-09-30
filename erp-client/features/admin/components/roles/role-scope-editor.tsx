"use client"

import * as React from "react"
import { ChevronDownIcon, ChevronRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    asScopeRule,
    scopeDescription,
} from "@/features/organization/lib/scope-description"
import type {
    DataScopeRecord,
    OrgUnit,
    ScopeDimension,
} from "@/features/organization/types"
import { hasOwnScopeDefault } from "../../lib/role-workbench"
import { RoleScopeChoiceForm } from "./role-scope-choice-form"

export function RoleScopeEditor({
    role,
    resource,
    actions,
    rows,
    units,
    permissions,
    savedPermissions,
    disabled,
    summary,
    ready,
    dimensions,
    policyVersion,
    onDirtyChange,
    onReload,
}: {
    role: { id: string; name: string } | null
    resource: string
    actions: readonly string[]
    rows: readonly DataScopeRecord[]
    units: readonly OrgUnit[]
    permissions: readonly string[]
    savedPermissions: readonly string[]
    disabled: boolean
    summary: string
    ready: boolean
    dimensions: readonly ScopeDimension[]
    policyVersion?: number
    onDirtyChange: (dirty: boolean) => void
    onReload: () => Promise<void>
}) {
    const profile = useAccountProfileQuery()
    const [expanded, setExpanded] = React.useState(false)
    const [exceptions, setExceptions] = React.useState(false)
    const [advanced, setAdvanced] = React.useState(false)
    const [action, setAction] = React.useState<string | null>(null)
    const [saved, setSaved] = React.useState(false)
    const [dirty, setDirty] = React.useState(false)
    const [snapshot, setSnapshot] = React.useState({ rows, policyVersion })
    const setScopeDirty = React.useCallback(
        (value: boolean) => {
            setDirty(value)
            if (value) setSaved(false)
            onDirtyChange(value)
        },
        [onDirtyChange],
    )
    React.useEffect(() => {
        if (!dirty && snapshot.policyVersion !== policyVersion)
            setSnapshot({ rows, policyVersion })
    }, [dirty, policyVersion, rows, snapshot.policyVersion])
    const selected = actions.filter((item) =>
        hasPermission(permissions, `${resource}:${item}`),
    )
    const chosen = action && selected.includes(action) ? [action] : selected
    const pendingActions = chosen.some(
        (item) => !hasPermission(savedPermissions, `${resource}:${item}`),
    )
    const canSave =
        hasPermission(profile.data?.permissions, "data_scope:create") &&
        hasPermission(profile.data?.permissions, "data_scope:delete")
    return (
        <section className="space-y-4 border-t pt-4">
            <div className="space-y-2">
                <h3 className="text-sm font-semibold">
                    角色默认范围 · {resourceLabel(resource)}
                </h3>
                <p className="text-xs leading-5 text-muted-foreground">
                    已保存的默认范围：{summary}
                </p>
            </div>
            <p className="text-xs leading-5 text-muted-foreground">
                此处修改会影响所有使用该角色的人员。只为某个人扩大范围，请到其人员资料的“业务权限”设置。
            </p>
            <Button
                id="role-default-scope-expand"
                type="button"
                variant="outline"
                size="sm"
                disabled={dirty}
                aria-expanded={expanded}
                onClick={() => setExpanded(!expanded)}
            >
                {expanded
                    ? "收起默认范围设置"
                    : "修改角色默认范围（影响所有使用者）"}
            </Button>
            {!role && (
                <p className="text-xs text-muted-foreground">
                    {hasOwnScopeDefault(resource)
                        ? "创建角色时，本业务的操作默认设置为“自己负责的数据”。"
                        : "本业务的数据范围需在创建角色后单独配置，不自动授予本人或公司范围。"}
                </p>
            )}
            {expanded && (
                <>
                    {ready &&
                    role &&
                    snapshot.policyVersion !== undefined &&
                    chosen.length ? (
                        <>
                            {action && (
                                <div className="flex items-center justify-between gap-2 text-sm">
                                    <span>单独设置：{actionLabel(action)}</span>
                                    <Button
                                        id="role-scope-common"
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        disabled={dirty}
                                        onClick={() => setAction(null)}
                                    >
                                        返回统一设置
                                    </Button>
                                </div>
                            )}
                            {pendingActions && (
                                <p className="text-xs leading-5 text-amber-700">
                                    请先在底部保存新勾选的操作权限，再设置这些操作的数据范围。
                                </p>
                            )}
                            {!canSave && (
                                <p className="text-xs text-muted-foreground">
                                    当前账号没有完整的范围修改权限。
                                </p>
                            )}
                            <RoleScopeChoiceForm
                                key={`${snapshot.policyVersion}-${chosen.join("-")}`}
                                roleId={role.id}
                                resource={resource}
                                actions={chosen}
                                rows={snapshot.rows}
                                units={units}
                                dimensions={dimensions}
                                policyVersion={snapshot.policyVersion}
                                disabled={
                                    disabled || pendingActions || !canSave
                                }
                                onDirtyChange={setScopeDirty}
                                onSaved={() => setSaved(true)}
                                onReload={onReload}
                            />
                        </>
                    ) : (
                        <p className="text-xs leading-5 text-muted-foreground">
                            {!role
                                ? "先创建岗位，再设置数据范围。"
                                : !selected.length
                                  ? "先在上方选择允许的操作。"
                                  : !ready
                                    ? "范围尚未读取成功，请先确认查看权限或重试。"
                                    : "范围版本缺失，请刷新后重试。"}
                        </p>
                    )}
                    {saved && !dirty && (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            数据范围已保存并生效
                        </p>
                    )}
                    <div className="border-t pt-3">
                        <Button
                            id="role-access-scope-exceptions"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="px-0 font-normal"
                            disabled={!ready || dirty}
                            aria-expanded={exceptions}
                            onClick={() => setExceptions(!exceptions)}
                        >
                            {exceptions ? (
                                <ChevronDownIcon />
                            ) : (
                                <ChevronRightIcon />
                            )}
                            个别操作需要不同范围？
                        </Button>
                        {exceptions && (
                            <div className="mt-2 divide-y">
                                {selected.map((item) => (
                                    <div
                                        key={item}
                                        className="flex items-center justify-between gap-3 py-2 text-sm"
                                    >
                                        <div className="min-w-0">
                                            <p>{actionLabel(item)}</p>
                                            <p className="mt-1 text-xs leading-5 text-muted-foreground">
                                                {rows
                                                    .filter(
                                                        (row) =>
                                                            row.enabled &&
                                                            row.actions.includes(
                                                                item,
                                                            ) &&
                                                            !(
                                                                row.scopeType ===
                                                                    "collaborative" &&
                                                                [
                                                                    "customer",
                                                                    "contract",
                                                                    "sales_order",
                                                                ].includes(
                                                                    resource,
                                                                )
                                                            ),
                                                    )
                                                    .map((row) =>
                                                        scopeDescription(
                                                            asScopeRule(row),
                                                            units,
                                                        ),
                                                    )
                                                    .join("；") || "尚未配置"}
                                            </p>
                                        </div>
                                        <Button
                                            id={`role-access-action-scope-${toAutomationIdSegment(item)}`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={dirty || disabled}
                                            onClick={() => setAction(item)}
                                        >
                                            单独设置
                                        </Button>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>
                    <div>
                        <Button
                            id="role-access-scope-adjust"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="px-0 text-muted-foreground"
                            aria-expanded={advanced}
                            onClick={() => setAdvanced(!advanced)}
                        >
                            {advanced ? (
                                <ChevronDownIcon />
                            ) : (
                                <ChevronRightIcon />
                            )}
                            查看原始规则
                        </Button>
                        {advanced && (
                            <ul className="mt-2 divide-y rounded-md border px-3">
                                {rows.map((row) => (
                                    <li
                                        key={row.id}
                                        className="space-y-1 py-3 text-xs leading-5 text-muted-foreground"
                                    >
                                        <p>
                                            {scopeDescription(
                                                asScopeRule(row),
                                                units,
                                            )}
                                        </p>
                                        <p>
                                            适用操作：
                                            {row.actions
                                                .map(actionLabel)
                                                .join("、")}
                                        </p>
                                    </li>
                                ))}
                                {!rows.length && (
                                    <li className="py-3 text-xs text-muted-foreground">
                                        暂无规则
                                    </li>
                                )}
                            </ul>
                        )}
                    </div>
                </>
            )}
            {dirty && (
                <p role="status" className="text-xs text-amber-700">
                    范围选择尚未保存。请保存数据范围或撤销选择，再切换业务或操作。
                </p>
            )}
        </section>
    )
}
