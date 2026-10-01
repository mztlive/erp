"use client"

import * as React from "react"
import { KeyRoundIcon, PencilIcon } from "lucide-react"
import {
    AccountProfileEditor,
    type AccountProfileSnapshot,
} from "../components/accounts/account-profile-editor"
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
import {
    BusinessFailureState,
    DetailPageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
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
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { unitLabel } from "@/features/organization/lib/tree"
import { AccessCheckPanel } from "../components/accounts/access-check-panel"
import { AccountOperationPermissions } from "../components/accounts/account-operation-permissions"
import { PersonalBusinessPermissions } from "../components/accounts/personal-business-permissions"
import { usePersonalGrantDraft } from "../hooks/use-personal-grant-draft"

export function AccountDetailPage({ accountId }: { accountId: string }) {
    const { draft: businessDraft, setDraft: setBusinessDraft } =
        usePersonalGrantDraft(accountId)
    const accounts = useAdminsQuery()
    const roles = useRolesQuery()
    const options = useAssignableRolesQuery()
    const profile = useAccountProfileQuery()
    const [profileDraft, setProfileSnapshot] =
        React.useState<AccountProfileSnapshot | null>(null)
    const profileSnapshot =
        profileDraft?.account.id === accountId ? profileDraft : null
    const account =
        accounts.data?.find((row) => row.id === accountId) ??
        (profileSnapshot?.account.id === accountId
            ? profileSnapshot.account
            : null)
    const can = (permission: string) =>
        hasPermission(profile.data?.permissions, permission)
    const org = useOrganizationStateQuery(can("org_unit:list"))
    const router = useRouter()
    const [editing, setEditing] = React.useState<"password" | null>(null)
    const [deleting, setDeleting] = React.useState(false)
    const [tab, setTab] = React.useState("basic")
    const [notice, setNotice] = React.useState<string | null>(null)
    if (accounts.isPending && !account)
        return (
            <PageScaffold>
                <p role="status">正在读取人员资料…</p>
            </PageScaffold>
        )
    if (accounts.isError && !profileSnapshot)
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
    const departmentLabel = org.isError
        ? "部门加载失败"
        : !can("org_unit:list")
          ? "无部门查看权限"
          : !org.data
            ? "正在读取…"
            : !person
              ? "不在可查看范围"
              : person.own_org_unit_id
                ? unitLabel(org.data.units, person.own_org_unit_id)
                : "未分配部门"
    return (
        <PageScaffold density="compact">
            <DetailPageHeader
                back={{
                    id: "account-detail-back",
                    label: "人员账号",
                    href: "/system/accounts",
                }}
                title={account.name}
                numberLabel="登录账号"
                documentNumber={account.account}
                meta={
                    <>
                        <span>所属部门：{departmentLabel}</span>
                        <span>{account.role_ids.length} 个角色</span>
                    </>
                }
                secondaryActions={
                    can("admin:delete") && (
                        <DropdownMenu>
                            <DropdownMenuTrigger
                                id="account-detail-more"
                                render={<Button variant="outline" size="sm" />}
                            >
                                更多
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end">
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
                                                    role.id === id &&
                                                    role.system,
                                            ),
                                        )
                                            ? "系统角色账号不可删除"
                                            : "删除账号"}
                                    </DropdownMenuItem>
                                )}
                            </DropdownMenuContent>
                        </DropdownMenu>
                    )
                }
            />
            {notice && (
                <p role="status" className="rounded-md bg-muted p-3 text-sm">
                    {notice}
                </p>
            )}
            <div className={surfacePanelClassName}>
                <Tabs
                    value={tab}
                    onValueChange={(next) => setTab(String(next))}
                >
                    <TabsList
                        variant="line"
                        className="w-full justify-start overflow-x-auto border-b border-grid px-4"
                        aria-label="人员设置分区"
                    >
                        <TabsTrigger
                            id="account-detail-basic-tab"
                            value="basic"
                        >
                            基本资料
                        </TabsTrigger>
                        <TabsTrigger
                            id="account-detail-access-tab"
                            value="access"
                        >
                            数据范围
                        </TabsTrigger>
                        {can("data_scope:list") && (
                            <TabsTrigger
                                id="account-detail-check-tab"
                                value="check"
                            >
                                访问检查
                            </TabsTrigger>
                        )}
                    </TabsList>
                    <TabsContent
                        value="basic"
                        keepMounted
                        className="space-y-4 p-4 data-[hidden]:hidden"
                    >
                        {profileSnapshot ? (
                            <>
                                <AccountProfileEditor
                                    key={accountId}
                                    snapshot={profileSnapshot}
                                    currentVersion={
                                        org.data?.organizationVersion
                                    }
                                    canName={can("admin:update")}
                                    onPasswordChange={() =>
                                        setEditing("password")
                                    }
                                    assignableRoles={options.data ?? []}
                                    roleLabels={roles.data ?? []}
                                    rolesReady={options.isSuccess}
                                    canOrganization={
                                        can("org_unit:manage") &&
                                        Boolean(
                                            profileSnapshot.view?.people.some(
                                                (person) =>
                                                    person.id === accountId,
                                            ),
                                        )
                                    }
                                    onDone={(saved) => {
                                        setProfileSnapshot(null)
                                        if (saved)
                                            setNotice("账号资料已统一保存。")
                                    }}
                                />
                                {options.isError && (
                                    <BusinessFailureState
                                        id="account-profile-role-options-retry"
                                        error={options.error}
                                        onRetry={() => void options.refetch()}
                                    />
                                )}
                            </>
                        ) : (
                            <section className="space-y-4 text-sm">
                                <div className="flex items-center gap-1">
                                    <h2 className="font-semibold">账号资料</h2>
                                    {(can("admin:update") ||
                                        (can("org_unit:manage") && person)) && (
                                        <Button
                                            id="account-profile-edit"
                                            variant="ghost"
                                            size="icon-sm"
                                            aria-label="编辑账号资料"
                                            title="编辑账号资料"
                                            onClick={() =>
                                                setProfileSnapshot({
                                                    account,
                                                    view: org.data,
                                                })
                                            }
                                        >
                                            <PencilIcon className="size-3.5" />
                                        </Button>
                                    )}
                                </div>
                                <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2 xl:grid-cols-3">
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            姓名
                                        </dt>
                                        <dd className="mt-1">{account.name}</dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            登录账号
                                        </dt>
                                        <dd className="mt-1">
                                            {account.account}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            所属部门
                                        </dt>
                                        <dd className="mt-1">
                                            {departmentLabel}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            创建时间
                                        </dt>
                                        <dd className="num mt-1">
                                            {formatDateTime(
                                                new Date(
                                                    account.created_at * 1000,
                                                ).toISOString(),
                                                "full",
                                            )}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            角色
                                        </dt>
                                        <dd className="mt-1">
                                            {roles.isError ? (
                                                <BusinessFailureState
                                                    error={roles.error}
                                                    onRetry={() =>
                                                        void roles.refetch()
                                                    }
                                                    id="account-detail-roles-retry"
                                                />
                                            ) : !account.role_ids.length ? (
                                                <p className="text-muted-foreground">
                                                    尚未分配角色，请点击账号资料旁的铅笔进行设置。
                                                </p>
                                            ) : (
                                                <ul className="flex flex-wrap gap-2">
                                                    {account.role_ids.map(
                                                        (roleId) => {
                                                            const role =
                                                                roles.data?.find(
                                                                    (row) =>
                                                                        row.id ===
                                                                        roleId,
                                                                )
                                                            return (
                                                                <li
                                                                    key={roleId}
                                                                    className="inline-flex items-center gap-2"
                                                                >
                                                                    <span className="font-medium">
                                                                        {role?.name ??
                                                                            "角色信息待确认"}
                                                                    </span>
                                                                    {can(
                                                                        "role:list",
                                                                    ) && (
                                                                        <Link
                                                                            id={`account-detail-role-${toAutomationIdSegment(roleId)}`}
                                                                            href={`/system/roles/${encodeURIComponent(roleId)}/edit?returnTo=${encodeURIComponent(`/system/accounts/${accountId}`)}`}
                                                                            className="text-xs text-muted-foreground hover:underline"
                                                                        >
                                                                            查看权限
                                                                        </Link>
                                                                    )}
                                                                </li>
                                                            )
                                                        },
                                                    )}
                                                </ul>
                                            )}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            登录密码
                                        </dt>
                                        <dd className="mt-1">
                                            {can("admin:update") ? (
                                                <Button
                                                    id="account-detail-password"
                                                    variant="outline"
                                                    size="sm"
                                                    onClick={() =>
                                                        setEditing("password")
                                                    }
                                                >
                                                    <KeyRoundIcon data-icon="inline-start" />
                                                    修改密码
                                                </Button>
                                            ) : (
                                                "无修改权限"
                                            )}
                                        </dd>
                                    </div>
                                </dl>
                                {org.isError && (
                                    <BusinessFailureState
                                        id="account-profile-org-retry"
                                        error={org.error}
                                        onRetry={() => void org.refetch()}
                                    />
                                )}
                            </section>
                        )}
                        <AccountOperationPermissions
                            key={accountId}
                            account={account}
                            roles={roles}
                            canRead={can("role:list")}
                        />
                    </TabsContent>
                    <TabsContent
                        value="access"
                        keepMounted
                        className="space-y-4 p-4 data-[hidden]:hidden"
                    >
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
                                can("data_scope:create") &&
                                can("org_unit:manage")
                            }
                        />
                        {org.isError && (
                            <BusinessFailureState
                                id="account-detail-scope-org-retry"
                                error={org.error}
                                onRetry={() => void org.refetch()}
                            />
                        )}
                    </TabsContent>
                    {can("data_scope:list") && (
                        <TabsContent
                            value="check"
                            keepMounted
                            className="p-4 data-[hidden]:hidden"
                        >
                            <AccessCheckPanel accountId={accountId} />
                        </TabsContent>
                    )}
                </Tabs>
            </div>
            {editing && (
                <AccountFormDialog
                    mode="edit"
                    account={account}
                    roleOptions={options.data ?? []}
                    section={editing}
                    onOpenChange={(open) => {
                        if (!open) setEditing(null)
                    }}
                    onSaved={() => setNotice("密码已修改。")}
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
        </PageScaffold>
    )
}
