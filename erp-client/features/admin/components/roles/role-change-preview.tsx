"use client"

import { CircleMinusIcon, CirclePlusIcon, UserRoundIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { useAdminsQuery } from "../../hooks/queries"
import { permissionChanges } from "../../lib/role-workbench"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { OrganizationStateView } from "@/features/organization/types"

export function RoleChangePreview({
    roleId,
    permissions,
    savedPermissions,
    org,
    scopeSummary,
}: {
    roleId?: string
    permissions: readonly string[]
    savedPermissions: readonly string[]
    org?: OrganizationStateView
    scopeSummary: string
}) {
    const accounts = useAdminsQuery()
    const bound =
        accounts.data?.filter((person) =>
            person.role_ids.includes(roleId ?? ""),
        ) ?? []
    const changes = permissionChanges(savedPermissions, permissions)
    const converted = savedPermissions.filter(
        (code) => code.endsWith(":*") && !permissions.includes(code),
    )
    const broadened = permissions.some(
        (code) => code.endsWith(":*") && !savedPermissions.includes(code),
    )
    return (
        <aside
            className="h-full min-w-0 space-y-7 border-t py-6 xl:border-l xl:border-t-0 xl:px-6"
            aria-label="本次调整预览"
        >
            <section>
                <h2 className="text-base font-semibold">本次调整</h2>
                {changes.length ? (
                    <ul className="mt-5 max-h-64 space-y-5 overflow-y-auto">
                        {changes.map((change) => (
                            <li
                                key={change.code}
                                className="space-y-1.5 text-base"
                            >
                                <p className="font-medium">{change.label}</p>
                                <p className="text-muted-foreground">
                                    原来：{change.allowed ? "不允许" : "允许"}
                                </p>
                                <p className="flex items-center gap-2">
                                    {change.allowed ? (
                                        <CirclePlusIcon className="size-4 text-emerald-600" />
                                    ) : (
                                        <CircleMinusIcon className="size-4 text-destructive" />
                                    )}
                                    调整后：{change.allowed ? "允许" : "不允许"}
                                </p>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="mt-4 text-base text-muted-foreground">
                        {converted.length || broadened
                            ? "操作范围保持不变，授权方式已调整"
                            : "尚未调整操作权限"}
                    </p>
                )}
                {broadened && (
                    <p className="mt-4 text-sm leading-5 text-amber-700">
                        已选择全部操作，该业务今后新增的操作也会自动授予。
                    </p>
                )}
                {converted.length > 0 && (
                    <p className="mt-4 text-sm leading-5 text-muted-foreground">
                        已切换为逐项授权，今后新增操作需另行勾选。
                    </p>
                )}
            </section>
            <section className="border-t pt-6">
                <h2 className="text-base font-semibold">影响人员</h2>
                {accounts.isPending ? (
                    <p className="mt-4 text-base text-muted-foreground">
                        正在读取人员…
                    </p>
                ) : accounts.isError ? (
                    <div className="mt-4 text-base">
                        <p>人员读取失败，暂不能确认影响人数。</p>
                        <Button
                            id="role-impact-retry"
                            type="button"
                            variant="link"
                            onClick={() => void accounts.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : (
                    <>
                        <ul className="mt-4 max-h-52 space-y-4 overflow-y-auto">
                            {bound.map((person) => {
                                const unitId = org?.people.find(
                                    (item) => item.id === person.id,
                                )?.own_org_unit_id
                                const department = org?.units.find(
                                    (unit) => unit.id === unitId,
                                )?.name
                                return (
                                    <li
                                        key={person.id}
                                        className="flex items-center gap-3"
                                    >
                                        <UserRoundIcon className="size-9 shrink-0 rounded-full bg-muted p-2 text-muted-foreground" />
                                        <div className="min-w-0">
                                            <p
                                                id={`role-impact-${toAutomationIdSegment(person.id)}`}
                                                className="break-words text-base font-medium"
                                            >
                                                {person.name}
                                            </p>
                                            <p className="text-sm text-muted-foreground">
                                                {department ?? "部门待确认"}
                                            </p>
                                        </div>
                                    </li>
                                )
                            })}
                        </ul>
                        <p className="mt-4 text-base text-muted-foreground">
                            {roleId
                                ? `当前可见人员中，共 ${bound.length} 人使用此岗位`
                                : "创建岗位后可分配给人员"}
                        </p>
                    </>
                )}
                <p className="mt-5 text-sm leading-6 text-muted-foreground">
                    保存后，使用此岗位的人员同步生效。其他岗位授予的权限仍会保留。
                </p>
            </section>
            <section className="border-t pt-6">
                <h2 className="text-base font-semibold">当前业务的数据范围</h2>
                <p className="mt-4 text-base leading-6">{scopeSummary}</p>
                <p className="mt-3 text-sm leading-5 text-muted-foreground">
                    调整操作权限不会自动修改已保存的范围。个人限制与业务状态仍须满足。
                </p>
            </section>
        </aside>
    )
}
