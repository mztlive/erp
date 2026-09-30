"use client"
import * as React from "react"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    selectResourceActions,
    workbenchResources,
} from "../../lib/role-workbench"
const resources = workbenchResources()
export function RoleAccessMatrix({
    permissions,
    onChange,
    disabled,
    onOpenAdvanced,
}: {
    role: { id: string; name: string } | null
    permissions: readonly string[]
    savedPermissions: readonly string[]
    onChange: (permissions: string[]) => void
    disabled: boolean
    onOpenAdvanced: () => void
    onScopeDirtyChange: (dirty: boolean) => void
}) {
    const [resource, setResource] = React.useState("sales_order")
    const [keyword, setKeyword] = React.useState("")
    const entry = resources.find((row) => row.resource === resource)!
    const inherited = permissions.some(
        (p) =>
            p.startsWith("*:") &&
            entry.codes.some((code) => hasPermission([p], code)),
    )
    return (
        <section
            className="grid min-h-0 flex-1 grid-cols-1 overflow-y-auto md:grid-cols-[13rem_minmax(0,1fr)]"
            aria-label="角色业务操作权限"
        >
            <nav className="space-y-2 border-r p-4">
                <Input
                    id="role-access-search"
                    aria-label="搜索业务"
                    placeholder="搜索业务"
                    value={keyword}
                    onChange={(e) => setKeyword(e.target.value)}
                />
                {resources
                    .filter((row) =>
                        resourceLabel(row.resource).includes(keyword),
                    )
                    .map((row) => (
                        <button
                            id={`role-access-resource-${toAutomationIdSegment(row.resource)}`}
                            key={row.resource}
                            type="button"
                            className={`block w-full rounded-md px-3 py-2 text-left text-sm ${row.resource === resource ? "bg-muted font-medium" : "hover:bg-muted/50"}`}
                            onClick={() => setResource(row.resource)}
                        >
                            {resourceLabel(row.resource)}
                        </button>
                    ))}
            </nav>
            <div className="space-y-5 p-5 text-sm">
                <div>
                    <h2 className="font-semibold">
                        {resourceLabel(resource)} · 可以进行哪些操作？
                    </h2>
                    <p className="mt-2 text-xs text-muted-foreground">
                        勾选后，此角色的所有使用者获得相应操作资格。每个人可以处理的数据，在人员资料中设置。
                    </p>
                </div>
                <div className="flex gap-2">
                    <Button
                        id="role-access-select-all"
                        size="sm"
                        variant="outline"
                        type="button"
                        disabled={disabled || inherited}
                        onClick={() =>
                            onChange([
                                ...selectResourceActions(
                                    permissions,
                                    resource,
                                    entry.codes,
                                ),
                                ...entry.codes,
                            ])
                        }
                    >
                        选择全部操作
                    </Button>
                    <Button
                        id="role-access-clear"
                        size="sm"
                        variant="outline"
                        type="button"
                        disabled={disabled || inherited}
                        onClick={() =>
                            onChange(
                                selectResourceActions(
                                    permissions,
                                    resource,
                                    entry.codes,
                                ).filter((code) => !entry.codes.includes(code)),
                            )
                        }
                    >
                        清空此业务
                    </Button>
                </div>
                <div className="grid gap-4 sm:grid-cols-2">
                    {entry.codes.map((code) => (
                        <label key={code} className="flex items-center gap-2">
                            <Checkbox
                                id={`role-access-${toAutomationIdSegment(code)}`}
                                checked={hasPermission(permissions, code)}
                                disabled={disabled || inherited}
                                onCheckedChange={(checked) => {
                                    const current = selectResourceActions(
                                        permissions,
                                        resource,
                                        entry.codes,
                                    )
                                    onChange(
                                        checked
                                            ? [...new Set([...current, code])]
                                            : current.filter((p) => p !== code),
                                    )
                                }}
                            />
                            {actionLabel(code.split(":")[1])}
                        </label>
                    ))}
                </div>
                {inherited && (
                    <p className="text-xs text-muted-foreground">
                        此业务已由跨业务权限覆盖，请在完整权限中核对。
                    </p>
                )}
                <Button
                    id="role-editor-advanced"
                    variant="outline"
                    size="sm"
                    type="button"
                    onClick={onOpenAdvanced}
                >
                    查看完整操作权限
                </Button>
            </div>
        </section>
    )
}
