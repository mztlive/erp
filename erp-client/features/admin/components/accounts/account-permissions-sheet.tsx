"use client"
import Link from "next/link"
import { QuickPreviewSheet, BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { useRolesQuery } from "../../hooks/queries"
import { usePersonScopes } from "../../hooks/use-person-data-scopes"
import { personScopeDescription } from "../../api/person-data-scopes"
import { permissionLabel } from "../../lib/permission-catalog"
import type { AdminAccount } from "../../types"
export function AccountPermissionsSheet({
    account,
    open,
    onOpenChange,
    onClosed,
    onAdjustRoles,
}: {
    account: AdminAccount | null
    open: boolean
    onOpenChange: (open: boolean) => void
    onClosed: () => void
    onAdjustRoles: () => void
}) {
    const roles = useRolesQuery()
    const profile = useAccountProfileQuery()
    const scopes = usePersonScopes(
        account?.id ?? "",
        open && hasPermission(profile.data?.permissions, "data_scope:list"),
    )
    return (
        <QuickPreviewSheet
            idPrefix="governance-admin-account-permissions-sheet"
            open={open}
            onOpenChange={onOpenChange}
            onOpenChangeComplete={(v) => {
                if (!v) onClosed()
            }}
            size="preview"
            title={`${account?.name ?? "人员"}的权限`}
            description="角色提供操作权限，数据范围由人员独立配置。"
            footer={
                <>
                    <Button
                        id="account-permission-close"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        关闭
                    </Button>
                    <Button
                        id="account-permission-role-edit"
                        onClick={onAdjustRoles}
                    >
                        调整角色
                    </Button>
                </>
            }
        >
            <div className="space-y-5 text-sm">
                <section className="space-y-3">
                    <h3 className="font-semibold">角色与操作权限</h3>
                    {roles.isError ? (
                        <BusinessFailureState
                            error={roles.error}
                            onRetry={() => void roles.refetch()}
                        />
                    ) : (
                        account?.role_ids.map((id) => {
                            const role = roles.data?.find((r) => r.id === id)
                            return (
                                <div key={id}>
                                    <p>{role?.name ?? "角色待确认"}</p>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        {role?.permissions
                                            .map(permissionLabel)
                                            .join("、")}
                                    </p>
                                </div>
                            )
                        })
                    )}
                </section>
                <section className="space-y-3">
                    <h3 className="font-semibold">此人的数据范围</h3>
                    {scopes.isError ? (
                        <BusinessFailureState
                            error={scopes.error}
                            onRetry={() => void scopes.refetch()}
                        />
                    ) : scopes.data ? (
                        scopes.data.businesses.map((b) => (
                            <div key={b.resource}>
                                <p>{resourceLabel(b.resource)}</p>
                                {b.actions.map((a) => (
                                    <p
                                        className="mt-1 text-xs text-muted-foreground"
                                        key={a}
                                    >
                                        {actionLabel(a)}：
                                        {personScopeDescription(
                                            scopes.data.items.find(
                                                (s) =>
                                                    s.resource === b.resource &&
                                                    s.action === a,
                                            ),
                                        )}
                                    </p>
                                ))}
                            </div>
                        ))
                    ) : (
                        <p className="text-xs">范围未读取或无查看权限。</p>
                    )}
                </section>
                {account && (
                    <Link
                        id="account-permission-details"
                        className="text-primary"
                        href={`/system/accounts/${encodeURIComponent(account.id)}`}
                    >
                        进入人员资料设置范围
                    </Link>
                )}
            </div>
        </QuickPreviewSheet>
    )
}
