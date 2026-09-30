"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs"
import {
    DropdownMenu,
    DropdownMenuTrigger,
    DropdownMenuContent,
    DropdownMenuItem,
} from "@/components/ui/dropdown-menu"
import { DeleteAdminDialog } from "../components/accounts/delete-admin-dialog"
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
    const router = useRouter()
    const [editing, setEditing] = React.useState<
        "name" | "roles" | "password" | null
    >(null)
    const [deleting, setDeleting] = React.useState(false)
    const [tab, setTab] = React.useState("basic")
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
                {(can("admin:update") || can("admin:delete")) && (
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id="account-detail-more"
                            render={<Button variant="outline" size="sm" />}
                        >
                            更多
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                            {can("admin:update") && (
                                <DropdownMenuItem
                                    id="account-detail-password"
                                    onClick={() => setEditing("password")}
                                >
                                    修改密码
                                </DropdownMenuItem>
                            )}
                            {can("admin:delete") && (
                                <DropdownMenuItem
                                    id="account-detail-delete"
                                    variant="destructive"
                                    disabled={
                                        !roles.isSuccess ||
                                        account.role_ids.some((id) =>
                                            roles.data.some(
                                                (role) =>
                                                    role.id === id &&
                                                    role.system,
                                            ),
                                        )
                                    }
                                    onClick={() => setDeleting(true)}
                                >
                                    {account.role_ids.some((id) =>
                                        roles.data?.some(
                                            (role) =>
                                                role.id === id && role.system,
                                        ),
                                    )
                                        ? "系统角色账号不可删除"
                                        : "删除账号"}
                                </DropdownMenuItem>
                            )}
                        </DropdownMenuContent>
                    </DropdownMenu>
                )}
            </header>
            {notice && (
                <p role="status" className="rounded-md bg-muted p-3 text-sm">
                    {notice}
                </p>
            )}
            <Tabs value={tab} onValueChange={(next) => setTab(String(next))}>
                <TabsList
                    variant="line"
                    className="w-full border-b"
                    aria-label="人员设置分区"
                >
                    <TabsTrigger id="account-detail-basic-tab" value="basic">
                        基本资料
                    </TabsTrigger>
                    <TabsTrigger id="account-detail-access-tab" value="access">
                        角色与数据范围
                    </TabsTrigger>
                </TabsList>
                <TabsContent
                    value="basic"
                    keepMounted
                    className="space-y-5 pt-4 data-[hidden]:hidden"
                >
                    <section className="space-y-4 rounded-lg border p-5 text-sm">
                        <div className="flex items-center justify-between gap-3">
                            <div>
                                <p className="text-muted-foreground">姓名</p>
                                <p className="mt-1 font-medium">
                                    {account.name}
                                </p>
                            </div>
                            {can("admin:update") && (
                                <Button
                                    id="account-detail-name"
                                    variant="outline"
                                    size="sm"
                                    onClick={() => setEditing("name")}
                                >
                                    修改姓名
                                </Button>
                            )}
                        </div>
                        <div className="border-t pt-4">
                            <p className="text-muted-foreground">登录账号</p>
                            <p className="mt-1">{account.account}</p>
                        </div>
                    </section>
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
                                部门管理关系用于登记组织职责，不授予业务操作或数据范围。业务数据范围请在“角色与数据范围”中设置。
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
                                                    (role) =>
                                                        role.id ===
                                                        grant.role_id,
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
                </TabsContent>
                <TabsContent
                    value="access"
                    keepMounted
                    className="space-y-5 pt-4 data-[hidden]:hidden"
                >
                    <section className="space-y-4 rounded-lg border p-5 text-sm">
                        <div className="flex flex-wrap items-center justify-between gap-3">
                            <div className="space-y-1">
                                <h2 className="font-semibold">已分配角色</h2>
                                <p className="text-xs text-muted-foreground">
                                    角色决定此人可以执行哪些操作。
                                </p>
                            </div>
                            {can("admin:update") && (
                                <Button
                                    id="account-detail-roles-edit"
                                    variant="outline"
                                    size="sm"
                                    disabled={!options.isSuccess}
                                    onClick={() => setEditing("roles")}
                                >
                                    调整角色
                                </Button>
                            )}
                        </div>
                        {options.isError && (
                            <BusinessFailureState
                                id="account-detail-role-options-retry"
                                error={options.error}
                                onRetry={() => void options.refetch()}
                            />
                        )}
                        {roles.isError ? (
                            <BusinessFailureState
                                error={roles.error}
                                onRetry={() => void roles.refetch()}
                                id="account-detail-roles-retry"
                            />
                        ) : !account.role_ids.length ? (
                            <p className="text-muted-foreground">
                                尚未分配角色，请先调整角色，再设置业务数据范围。
                            </p>
                        ) : (
                            <ul className="divide-y">
                                {account.role_ids.map((roleId) => {
                                    const role = roles.data?.find(
                                        (row) => row.id === roleId,
                                    )
                                    return (
                                        <li
                                            key={roleId}
                                            className="flex flex-wrap items-center justify-between gap-2 py-3"
                                        >
                                            <span className="font-medium">
                                                {role?.name ?? "角色信息待确认"}
                                            </span>
                                            {can("role:list") && (
                                                <Link
                                                    id={`account-detail-role-${toAutomationIdSegment(roleId)}`}
                                                    href={`/system/roles/${encodeURIComponent(roleId)}/edit?returnTo=${encodeURIComponent(`/system/accounts/${accountId}`)}`}
                                                    className="text-xs text-muted-foreground hover:underline"
                                                >
                                                    查看角色操作权限
                                                </Link>
                                            )}
                                        </li>
                                    )
                                })}
                            </ul>
                        )}
                        <p className="text-xs text-muted-foreground">
                            调整角色只改变此人的角色分配。修改角色本身的操作权限会影响该角色的所有使用者。
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
                        canCreate={
                            can("data_scope:create") && can("org_unit:manage")
                        }
                    />
                    {org.isError && (
                        <BusinessFailureState
                            id="account-detail-scope-org-retry"
                            error={org.error}
                            onRetry={() => void org.refetch()}
                        />
                    )}
                    {can("data_scope:list") && (
                        <details className="rounded-lg border p-5">
                            <summary
                                id="account-detail-check-expand"
                                className="cursor-pointer text-sm font-medium"
                            >
                                访问检查
                            </summary>
                            <div className="mt-4">
                                <AccessCheckPanel accountId={accountId} />
                            </div>
                        </details>
                    )}
                </TabsContent>
            </Tabs>
            {editing && (
                <AccountFormDialog
                    mode="edit"
                    account={account}
                    roleOptions={options.data ?? []}
                    section={editing}
                    onOpenChange={(open) => {
                        if (!open) setEditing(null)
                    }}
                    onSaved={() =>
                        setNotice(
                            editing === "roles"
                                ? "角色分配已保存，请核对下方各业务的数据范围。"
                                : editing === "name"
                                  ? "姓名已保存。"
                                  : "密码已修改。",
                        )
                    }
                />
            )}
            {deleting && (
                <DeleteAdminDialog
                    account={account}
                    onOpenChange={(open) => {
                        if (!open) setDeleting(false)
                    }}
                    onDeleted={() => router.push("/system/accounts")}
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
                        setNotice("所属部门或管理关系已保存。")
                    }}
                />
            )}
        </PageScaffold>
    )
}
