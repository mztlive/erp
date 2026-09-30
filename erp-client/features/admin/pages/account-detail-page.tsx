"use client"

import * as React from "react"
import Link from "next/link"
import { BusinessFailureState, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useAdminsQuery,
    useRolesQuery,
    useAssignableRolesQuery,
} from "../hooks/queries"
import { AccountFormDialog } from "../components/accounts/account-form-dialog"
import { OrganizationChangeDialog } from "@/features/organization/components/organization-change-dialog"
import {
    useOrganizationStateQuery,
    usePreviewOrganizationChangeMutation,
    useSubmitOrganizationChangeMutation,
} from "@/features/organization/hooks/queries"
import {
    EMPTY_CHANGE_DRAFT,
    type OrganizationChangeDraft,
} from "@/features/organization/lib/change-payload"
import { isRelationActive, unitLabel } from "@/features/organization/lib/tree"
import { AccessCheckPanel } from "../components/accounts/access-check-panel"
import { PersonalBusinessPermissions } from "../components/accounts/personal-business-permissions"
import { usePersonalGrantDraft } from "../hooks/use-personal-grant-draft"

import { permissionLabel } from "../lib/permission-catalog"

export function AccountDetailPage({ accountId }: { accountId: string }) {
    const { draft: businessDraft, setDraft: setBusinessDraft } =
        usePersonalGrantDraft(accountId)
    const accounts = useAdminsQuery()
    const roles = useRolesQuery()
    const options = useAssignableRolesQuery()
    const profile = useAccountProfileQuery()
    const account = accounts.data?.find((row) => row.id === accountId) ?? null
    const can = (permission: string) =>
        hasPermission(profile.data?.permissions, permission)
    const org = useOrganizationStateQuery(can("org_unit:list"))
    const preview = usePreviewOrganizationChangeMutation()
    const submit = useSubmitOrganizationChangeMutation()
    const [editing, setEditing] = React.useState(false)
    const [change, setChange] = React.useState<OrganizationChangeDraft | null>(
        null,
    )
    const [notice, setNotice] = React.useState<string | null>(null)
    if (accounts.isPending)
        return (
            <PageScaffold>
                <p role="status">正在读取人员资料…</p>
            </PageScaffold>
        )
    if (accounts.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={accounts.error}
                    onRetry={() => void accounts.refetch()}
                    id="account-detail-retry"
                />
            </PageScaffold>
        )
    if (!account)
        return (
            <PageScaffold>
                <p>账号不存在或无权查看。</p>
                <Link id="account-detail-missing-back" href="/system/accounts">
                    返回人员账号
                </Link>
            </PageScaffold>
        )
    const person = org.data?.people.find((row) => row.id === account.id)
    const grants =
        org.data?.management.filter(
            (row) =>
                row.user_id === accountId &&
                isRelationActive(row.valid_from, row.valid_to, org.data.asOf),
        ) ?? []
    const begin = (
        operation: OrganizationChangeDraft["operation"],
        roleId = "",
        assignmentId = "",
    ) =>
        setChange({
            ...EMPTY_CHANGE_DRAFT,
            operation,
            userId: accountId,
            roleId,
            assignmentId,
            orgUnitId:
                operation === "transfer_member"
                    ? (person?.own_org_unit_id ?? "")
                    : "",
        })
    return (
        <PageScaffold className="mx-auto w-full max-w-6xl space-y-6">
            <header className="flex flex-wrap items-center justify-between gap-3">
                <div>
                    <Link
                        id="account-detail-back"
                        className="text-sm text-muted-foreground"
                        href="/system/accounts"
                    >
                        返回人员账号
                    </Link>
                    <h1 className="mt-2 text-xl font-semibold">
                        {account.name} · 人员资料
                    </h1>
                    <p className="text-sm text-muted-foreground">
                        登录账号：{account.account}
                    </p>
                </div>
                {can("admin:update") && (
                    <Button
                        id="account-detail-edit"
                        onClick={() => setEditing(true)}
                    >
                        编辑资料与角色
                    </Button>
                )}
            </header>
            {notice && (
                <p role="status" className="rounded-md bg-muted p-3 text-sm">
                    {notice}
                </p>
            )}
            <section className="space-y-3 rounded-lg border p-5">
                <h2 className="font-semibold">开通进度与配置待办</h2>
                <ol className="flex flex-wrap gap-x-6 gap-y-2 text-sm">
                    <li>1. 账号已创建</li>
                    <li>
                        2.{" "}
                        {person?.own_org_unit_id
                            ? "所属部门已设置"
                            : "核对所属部门"}
                    </li>
                    <li>3. 已分配 {account.role_ids.length} 个角色</li>
                    <li>4. 设置此人的业务数据范围</li>
                </ol>
                <p className="text-xs text-muted-foreground">
                    先分配角色，再在下方为此人的业务操作设置数据范围。未配置的操作显示“待设置”。
                </p>
            </section>
            <PersonalBusinessPermissions
                key={accountId}
                userId={accountId}
                draft={businessDraft}
                onDraftChange={setBusinessDraft}
                name={account.name}
                units={org.data?.units ?? []}
                ready={org.isSuccess}
                canRead={can("data_scope:list")}
                canCreate={can("data_scope:create") && can("org_unit:manage")}
            />
            <section className="space-y-5 rounded-lg border p-5">
                <div className="space-y-3">
                    <h2 className="font-semibold">所属部门</h2>
                    <p className="text-sm">
                        {org.isError
                            ? "部门加载失败"
                            : !can("org_unit:list")
                              ? "无部门查看权限"
                              : !org.data
                                ? "正在读取…"
                                : !person
                                  ? "不在可查看范围"
                                  : person.own_org_unit_id
                                    ? unitLabel(
                                          org.data.units,
                                          person.own_org_unit_id,
                                      )
                                    : "未分配部门"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        所属部门影响“本人所属部门”范围；调岗不改写历史业绩，也不会自动改派业务单据。
                    </p>
                    {can("org_unit:manage") && person && (
                        <Button
                            id="account-detail-department"
                            variant="outline"
                            onClick={() => begin("transfer_member")}
                        >
                            调整所属部门
                        </Button>
                    )}
                </div>
                <details className="space-y-3 border-t pt-4">
                    <summary
                        id="account-detail-management-expand"
                        className="cursor-pointer text-sm font-medium"
                    >
                        高级：部门管理关系
                    </summary>
                    <p className="text-xs leading-5 text-muted-foreground">
                        部门管理关系用于登记组织职责，不授予业务操作或数据范围。此人的业务范围在上方单独设置。
                    </p>
                    {org.isError ? (
                        <BusinessFailureState
                            error={org.error}
                            onRetry={() => void org.refetch()}
                            id="account-detail-org-retry"
                        />
                    ) : grants.length ? (
                        <ul className="space-y-3">
                            {grants.map((grant) => (
                                <li key={grant.id} className="text-sm">
                                    <p>
                                        {unitLabel(
                                            org.data!.units,
                                            grant.org_unit_id,
                                        )}
                                        {grant.include_descendants
                                            ? "（含下级）"
                                            : "（仅本级）"}{" "}
                                        ·{" "}
                                        {roles.data?.find(
                                            (role) => role.id === grant.role_id,
                                        )?.name ?? "角色信息待确认"}
                                    </p>
                                    {!account.role_ids.includes(
                                        grant.role_id,
                                    ) && (
                                        <p className="text-amber-700">
                                            当前未持有对应角色，该管理关系不提供授权。
                                        </p>
                                    )}
                                    {can("org_unit:manage") && (
                                        <Button
                                            id={`account-detail-revoke-${toAutomationIdSegment(grant.id)}`}
                                            variant="ghost"
                                            size="sm"
                                            onClick={() =>
                                                begin(
                                                    "revoke_management",
                                                    grant.role_id,
                                                    grant.id,
                                                )
                                            }
                                        >
                                            撤销管理关系
                                        </Button>
                                    )}
                                </li>
                            ))}
                        </ul>
                    ) : (
                        <p className="text-sm">
                            {org.isSuccess
                                ? "当前可查看范围内未配置"
                                : "管理部门待确认"}
                        </p>
                    )}
                    {can("org_unit:manage") && person && (
                        <Button
                            id="account-detail-management"
                            variant="outline"
                            onClick={() =>
                                begin(
                                    "grant_management",
                                    account.role_ids.length === 1
                                        ? account.role_ids[0]
                                        : "",
                                )
                            }
                        >
                            设置此人管理的部门
                        </Button>
                    )}
                </details>
            </section>
            <section className="space-y-4 rounded-lg border p-5">
                <h2 className="font-semibold">角色与操作权限</h2>
                <p className="text-xs text-muted-foreground">
                    修改角色操作权限会影响所有使用者。此人的数据范围在上方独立设置。
                </p>
                {roles.isError ? (
                    <BusinessFailureState
                        error={roles.error}
                        onRetry={() => void roles.refetch()}
                        id="account-detail-roles-retry"
                    />
                ) : (
                    account.role_ids.map((roleId) => {
                        const role = roles.data?.find(
                            (row) => row.id === roleId,
                        )
                        return (
                            <div
                                key={roleId}
                                className="space-y-2 border-t pt-3"
                            >
                                <div className="flex flex-wrap justify-between gap-2">
                                    <h3 className="font-medium">
                                        {role?.name ?? "角色信息待确认"}
                                    </h3>
                                    <Link
                                        id={`account-detail-role-${toAutomationIdSegment(roleId)}`}
                                        href={`/system/roles/${encodeURIComponent(roleId)}/edit?returnTo=${encodeURIComponent(`/system/accounts/${accountId}`)}`}
                                        className="text-sm text-primary"
                                    >
                                        查看角色操作权限
                                    </Link>
                                </div>
                                <p className="text-sm text-muted-foreground">
                                    {role?.permissions.includes("*:*")
                                        ? "全部操作权限；数据范围另行核对。"
                                        : role?.permissions
                                              .map(permissionLabel)
                                              .join("、") || "操作权限待确认"}
                                </p>
                            </div>
                        )
                    })
                )}
            </section>
            {can("data_scope:list") && (
                <AccessCheckPanel accountId={accountId} />
            )}
            {editing && (
                <AccountFormDialog
                    mode="edit"
                    account={account}
                    roleOptions={options.data ?? []}
                    onOpenChange={setEditing}
                    departmentLabel={
                        person?.own_org_unit_id && org.data
                            ? unitLabel(org.data.units, person.own_org_unit_id)
                            : undefined
                    }
                />
            )}
            {change && org.data && (
                <OrganizationChangeDialog
                    open
                    draft={change}
                    view={org.data}
                    expectedVersion={org.data.organizationVersion}
                    previewing={preview.isPending}
                    submitting={submit.isPending}
                    onOpenChange={(open) => {
                        if (!open) setChange(null)
                    }}
                    onPreview={(request) => preview.mutateAsync(request)}
                    onSubmit={async (request) => {
                        await submit.mutateAsync(request)
                        setNotice(
                            "组织关系已保存，请核对更新后的配置待办与访问检查结果。",
                        )
                    }}
                />
            )}
        </PageScaffold>
    )
}
