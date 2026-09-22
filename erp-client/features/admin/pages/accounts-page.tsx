"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef } from "@tanstack/react-table"
import { PlusIcon, ShieldCheckIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { AccountPermissionsSheet } from "@/features/admin/components/accounts/account-permissions-sheet"
import { AccountFormDialog } from "@/features/admin/components/accounts/account-form-dialog"
import type { AccountDraft } from "@/features/admin/components/accounts/account-form-dialog"
import { DeleteAdminDialog } from "@/features/admin/components/accounts/delete-admin-dialog"
import {
    useAdminsQuery,
    useAssignableRolesQuery,
    useRolesQuery,
} from "@/features/admin/hooks/queries"
import type { AdminAccount } from "@/features/admin/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { PeopleNavigation } from "@/features/organization/components/people-navigation"
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
import { unitLabel } from "@/features/organization/lib/tree"
import { formatDateTime } from "@/lib/datetime"

type AccountFormState = {
    mode: "create" | "edit"
    account: AccountDraft | null
}

/**
 * 账号管理：登录账号的新建、改资料与删除。
 *
 * 与「权限配置」分工：这里管账号本身（账号、姓名、密码），
 * 角色只做初始绑定；授权口径与有效权限解释在权限配置页。
 */
export function AccountsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const profileQuery = useAccountProfileQuery()
    const canReadOrganization = hasPermission(
        profileQuery.data?.permissions,
        "org_unit:list",
    )
    const canManageOrganization =
        canReadOrganization &&
        hasPermission(profileQuery.data?.permissions, "org_unit:manage")
    const organizationQuery = useOrganizationStateQuery(canReadOrganization)
    const previewChange = usePreviewOrganizationChangeMutation()
    const submitChange = useSubmitOrganizationChangeMutation()
    const [departmentDraft, setDepartmentDraft] =
        React.useState<OrganizationChangeDraft | null>(null)
    const adminsQuery = useAdminsQuery()
    const rolesQuery = useRolesQuery()
    const assignableRolesQuery = useAssignableRolesQuery()

    /** 权限配置页按角色跳转过来时（?q=角色名），首屏直接带上该筛选。 */
    const [keyword, setKeyword] = React.useState(
        () => searchParams.get("q") ?? "",
    )
    const [searchDraft, setSearchDraft] = React.useState(keyword)
    React.useEffect(() => {
        const next = searchParams.get("q") ?? ""
        setKeyword(next)
        setSearchDraft(next)
    }, [searchParams])
    const [accountForm, setAccountForm] =
        React.useState<AccountFormState | null>(null)
    const [permissionAccount, setPermissionAccount] =
        React.useState<AdminAccount | null>(null)
    const [permissionsOpen, setPermissionsOpen] = React.useState(false)
    const permissionReturnId = React.useRef<string | null>(null)
    const editAfterPermissionsClose = React.useRef(false)

    const [setupMessage, setSetupMessage] = React.useState<string | null>(null)
    const [deletingAccount, setDeletingAccount] = React.useState<{
        id: string
        account: string
    } | null>(null)

    const roleNameById = React.useMemo(
        () =>
            new Map(
                (rolesQuery.data ?? []).map((role) => [role.id, role.name]),
            ),
        [rolesQuery.data],
    )

    const rows = React.useMemo(() => {
        const q = keyword.trim().toLowerCase()
        const all = adminsQuery.data ?? []
        if (!q) return all
        return all.filter((account) =>
            [
                account.account,
                account.name,
                ...account.role_ids.map((id) => roleNameById.get(id) ?? ""),
            ]
                .join(" ")
                .toLowerCase()
                .includes(q),
        )
    }, [adminsQuery.data, keyword, roleNameById])

    const columns = React.useMemo<ColumnDef<AdminAccount>[]>(
        () => [
            {
                id: "identity",
                size: 240,
                header: "账号",
                cell: ({ row }) => (
                    <div className="min-w-[9rem]">
                        <div className="font-medium">
                            {row.original.name || row.original.account}
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                            {row.original.account}
                        </div>
                    </div>
                ),
            },
            {
                id: "department",
                size: 190,
                header: "所属部门",
                cell: ({ row }) => {
                    if (!canReadOrganization) return "无部门查看权限"
                    if (organizationQuery.isError) return "部门加载失败"
                    if (!organizationQuery.data) return "正在加载…"
                    const person = organizationQuery.data.people.find(
                        (item) => item.id === row.original.id,
                    )
                    if (!person) return "不在可查看范围"
                    return person.own_org_unit_id ? (
                        unitLabel(
                            organizationQuery.data.units,
                            person.own_org_unit_id,
                        )
                    ) : (
                        <span className="text-amber-700">未分配部门</span>
                    )
                },
            },
            {
                id: "roles",
                size: 300,
                header: "角色",
                cell: ({ row }) =>
                    row.original.role_ids
                        .map((id) => roleNameById.get(id) ?? "角色信息待确认")
                        .join("、") || "—",
            },
            {
                id: "createdAt",
                size: 190,
                header: "创建时间",
                cell: ({ row }) => (
                    <span className="num text-[13px] text-muted-foreground">
                        {formatDateTime(
                            new Date(
                                row.original.created_at * 1000,
                            ).toISOString(),
                            "full",
                        )}
                    </span>
                ),
            },
            {
                id: "actions",
                size: 320,
                minSize: 280,
                header: () => <span className="block text-right">操作</span>,
                cell: ({ row }) => {
                    const account = row.original
                    const protectedAccount = account.role_ids.some((id) =>
                        rolesQuery.data?.some(
                            (role) => role.id === id && role.system,
                        ),
                    )
                    const segment = toAutomationIdSegment(account.id)
                    return (
                        <div className="flex flex-wrap items-center justify-end gap-1">
                            {canManageOrganization &&
                            organizationQuery.data?.people.some(
                                (person) => person.id === account.id,
                            ) ? (
                                <Button
                                    id={`governance-admin-accounts-row-${segment}-department`}
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={() =>
                                        setDepartmentDraft({
                                            ...EMPTY_CHANGE_DRAFT,
                                            operation: "transfer_member",
                                            userId: account.id,
                                            orgUnitId:
                                                organizationQuery.data?.people.find(
                                                    (person) =>
                                                        person.id ===
                                                        account.id,
                                                )?.own_org_unit_id ?? "",
                                        })
                                    }
                                >
                                    调整部门
                                </Button>
                            ) : null}
                            {canManageOrganization &&
                            organizationQuery.data?.people.some(
                                (person) => person.id === account.id,
                            ) ? (
                                <Button
                                    id={`governance-admin-accounts-row-${segment}-management`}
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={() =>
                                        setDepartmentDraft({
                                            ...EMPTY_CHANGE_DRAFT,
                                            operation: "grant_management",
                                            userId: account.id,
                                            roleId:
                                                account.role_ids.length === 1
                                                    ? account.role_ids[0]!
                                                    : "",
                                        })
                                    }
                                >
                                    管理部门
                                </Button>
                            ) : null}
                            <Button
                                id={`governance-admin-accounts-row-${segment}-edit`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={() =>
                                    setAccountForm({
                                        mode: "edit",
                                        account: {
                                            id: account.id,
                                            account: account.account,
                                            name: account.name,
                                            role_ids: [...account.role_ids],
                                        },
                                    })
                                }
                            >
                                编辑
                            </Button>
                            <Button
                                id={`governance-admin-accounts-row-${segment}-permissions`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={() => {
                                    permissionReturnId.current = `governance-admin-accounts-row-${segment}-permissions`
                                    setPermissionAccount(account)
                                    setPermissionsOpen(true)
                                }}
                            >
                                查看权限
                            </Button>
                            <Button
                                id={`governance-admin-accounts-row-${segment}-delete`}
                                disabled={protectedAccount}
                                title={
                                    protectedAccount
                                        ? "绑定系统角色的账号不可删除"
                                        : undefined
                                }
                                type="button"
                                size="xs"
                                variant="ghost"
                                className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                                onClick={() =>
                                    setDeletingAccount({
                                        id: account.id,
                                        account: account.account,
                                    })
                                }
                            >
                                删除
                            </Button>
                        </div>
                    )
                },
            },
        ],
        [
            roleNameById,
            rolesQuery.data,
            canReadOrganization,
            canManageOrganization,
            organizationQuery.data,
            organizationQuery.isError,
        ],
    )

    const hasSearch = keyword.trim().length > 0
    const hasPendingChanges = searchDraft.trim() !== keyword.trim()
    const appliedChips = hasSearch
        ? [{ key: "q", label: `搜索：${keyword.trim()}` }]
        : []

    const applyFilters = React.useCallback(() => {
        const next = searchDraft.trim()
        setSearchDraft(next)
        setKeyword(next)
        router.replace(
            `/system/accounts${next ? `?q=${encodeURIComponent(next)}` : ""}`,
            { scroll: false },
        )
    }, [searchDraft, router])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setKeyword("")
        router.replace("/system/accounts", { scroll: false })
    }, [router])

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="系统"
                title="组织与人员"
                description="创建人员账号，分配所属部门与角色，并查看权限配置。"
            >
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="governance-admin-accounts-permission-config"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() => router.push("/system/access-audit")}
                    >
                        <ShieldCheckIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        角色与权限
                    </Button>
                    <Button
                        id="governance-admin-accounts-create"
                        type="button"
                        size="sm"
                        onClick={() =>
                            setAccountForm({
                                mode: "create",
                                account: null,
                            })
                        }
                    >
                        <PlusIcon className="size-3.5" aria-hidden="true" />
                        新建账号
                    </Button>
                </div>
            </ListWorkspaceHeader>

            <PeopleNavigation current="accounts" />
            {setupMessage ? (
                <p role="status" className="rounded-lg bg-muted p-3 text-sm">
                    {setupMessage}
                </p>
            ) : null}
            {canReadOrganization && organizationQuery.isError ? (
                <BusinessFailureState
                    id="accounts-organization-retry"
                    title="部门信息加载失败"
                    error={organizationQuery.error}
                    onRetry={() => {
                        void organizationQuery.refetch()
                    }}
                />
            ) : null}
            <ListWorkSurface
                ariaLabel="账号列表"
                toolbar={
                    <ListWorkspaceFilterBar
                        idPrefix="governance-admin-accounts"
                        formAriaLabel="账号查询"
                        onSubmit={applyFilters}
                        search={
                            <ListSearchField
                                id="governance-admin-accounts-search"
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="账号、姓名或角色"
                                aria-label="搜索账号"
                            />
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: adminsQuery.isPending,
                            failed: adminsQuery.isError,
                            resultCount: adminsQuery.data
                                ? rows.length
                                : undefined,
                            noun: "个账号",
                            loadingLabel: "正在加载账号…",
                        })}
                        chips={appliedChips}
                        onClearChip={clearAllFilters}
                        onClearAll={clearAllFilters}
                        hasPendingChanges={hasPendingChanges}
                    />
                }
                table={
                    <DataTable
                        id="governance-admin-accounts-table"
                        columns={columns}
                        data={rows}
                        getRowId={(row) => row.id}
                        rowCount={rows.length}
                        layout="flush"
                        loading={adminsQuery.isPending}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                        errorState={
                            adminsQuery.isError ? (
                                <BusinessFailureState
                                    error={adminsQuery.error}
                                    title="账号列表加载失败"
                                    action={
                                        <Button
                                            id="governance-admin-accounts-retry"
                                            type="button"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={() =>
                                                void adminsQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !adminsQuery.isError && rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={hasSearch ? "filter" : "no-data"}
                                    className={listWorkspaceEmptyStateClassName}
                                    title={
                                        hasSearch
                                            ? "当前筛选无结果"
                                            : "还没有登录账号"
                                    }
                                    description={
                                        hasSearch
                                            ? "没有账号符合当前搜索条件。"
                                            : "点击「新建账号」创建第一条账号记录。"
                                    }
                                />
                            ) : undefined
                        }
                    />
                }
            />

            {departmentDraft &&
            organizationQuery.data &&
            canManageOrganization ? (
                <OrganizationChangeDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setDepartmentDraft(null)
                    }}
                    view={organizationQuery.data}
                    draft={departmentDraft}
                    expectedVersion={organizationQuery.data.organizationVersion}
                    previewing={previewChange.isPending}
                    submitting={submitChange.isPending}
                    onPreview={(request) => previewChange.mutateAsync(request)}
                    onSubmit={async (request) => {
                        await submitChange.mutateAsync(request)
                    }}
                />
            ) : null}
            <AccountPermissionsSheet
                account={permissionAccount}
                open={permissionsOpen}
                onOpenChange={setPermissionsOpen}
                onAdjustRoles={() => {
                    editAfterPermissionsClose.current = true
                    setPermissionsOpen(false)
                }}
                onClosed={() => {
                    if (
                        editAfterPermissionsClose.current &&
                        permissionAccount
                    ) {
                        editAfterPermissionsClose.current = false
                        setAccountForm({
                            mode: "edit",
                            account: {
                                ...permissionAccount,
                                role_ids: [...permissionAccount.role_ids],
                            },
                        })
                    } else if (permissionReturnId.current) {
                        document
                            .getElementById(permissionReturnId.current)
                            ?.focus({ preventScroll: true })
                    }
                }}
            />

            {accountForm ? (
                <AccountFormDialog
                    key={
                        accountForm.mode === "edit"
                            ? (accountForm.account?.id ?? "edit")
                            : "create"
                    }
                    onCreated={
                        canManageOrganization
                            ? (accountName) => {
                                  setSetupMessage(
                                      "账号已创建，正在准备分配部门。",
                                  )
                                  void Promise.all([
                                      adminsQuery.refetch(),
                                      organizationQuery.refetch(),
                                  ])
                                      .then(([accounts, organization]) => {
                                          const created = accounts.data?.find(
                                              (account) =>
                                                  account.account ===
                                                  accountName,
                                          )
                                          if (
                                              !created ||
                                              !organization.data?.people.some(
                                                  (person) =>
                                                      person.id === created.id,
                                              ) ||
                                              organization.isError
                                          ) {
                                              setSetupMessage(
                                                  "账号已创建，部门信息暂不可用。请刷新后点击该账号的「调整部门」完成分配，无需重复创建账号。",
                                              )
                                              return
                                          }
                                          setSetupMessage(
                                              "账号已创建。请分配所属部门；取消后也可在列表中继续调整。",
                                          )
                                          setDepartmentDraft({
                                              ...EMPTY_CHANGE_DRAFT,
                                              operation: "transfer_member",
                                              userId: created.id,
                                          })
                                      })
                                      .catch(() =>
                                          setSetupMessage(
                                              "账号已创建，请在列表中点击「调整部门」完成分配。",
                                          ),
                                      )
                              }
                            : undefined
                    }
                    departmentLabel={
                        accountForm.account && organizationQuery.data
                            ? (() => {
                                  const person =
                                      organizationQuery.data.people.find(
                                          (item) =>
                                              item.id ===
                                              accountForm.account?.id,
                                      )
                                  return !person
                                      ? "不在可查看范围"
                                      : person.own_org_unit_id
                                        ? unitLabel(
                                              organizationQuery.data.units,
                                              person.own_org_unit_id,
                                          )
                                        : "未分配部门"
                              })()
                            : undefined
                    }
                    onAdjustDepartment={
                        canManageOrganization &&
                        accountForm.account &&
                        organizationQuery.data?.people.some(
                            (person) => person.id === accountForm.account?.id,
                        )
                            ? () => {
                                  const account = accountForm.account!
                                  setAccountForm(null)
                                  setDepartmentDraft({
                                      ...EMPTY_CHANGE_DRAFT,
                                      operation: "transfer_member",
                                      userId: account.id,
                                      orgUnitId:
                                          organizationQuery.data?.people.find(
                                              (person) =>
                                                  person.id === account.id,
                                          )?.own_org_unit_id ?? "",
                                  })
                              }
                            : undefined
                    }
                    mode={accountForm.mode}
                    account={accountForm.account}
                    roleOptions={assignableRolesQuery.data ?? []}
                    onOpenChange={(open) => {
                        if (!open) setAccountForm(null)
                    }}
                />
            ) : null}

            {deletingAccount ? (
                <DeleteAdminDialog
                    key={deletingAccount.id}
                    account={deletingAccount}
                    onOpenChange={(open) => {
                        if (!open) setDeletingAccount(null)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
