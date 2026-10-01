"use client"

import * as React from "react"
import Link from "next/link"
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
import { AccountFormDialog } from "@/features/admin/components/accounts/account-form-dialog"
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
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { unitLabel } from "@/features/organization/lib/tree"
import { formatDateTime } from "@/lib/datetime"

/** 人员列表只负责查找与创建，所有人员设置统一进入人员资料。 */
export function AccountsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const profileQuery = useAccountProfileQuery()
    const canReadOrganization = hasPermission(
        profileQuery.data?.permissions,
        "org_unit:list",
    )
    const organizationQuery = useOrganizationStateQuery(canReadOrganization)
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
    const [creating, setCreating] = React.useState(false)
    const [setupMessage, setSetupMessage] = React.useState<string | null>(null)

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
                enableHiding: false,
                size: 240,
                header: "人员 / 登录账号",
                cell: ({ row }) => (
                    <div className="min-w-[9rem]">
                        <Link
                            id={`account-open-${toAutomationIdSegment(row.original.id)}`}
                            className="font-medium text-primary hover:underline"
                            href={`/system/accounts/${encodeURIComponent(row.original.id)}`}
                        >
                            {row.original.name || row.original.account}
                        </Link>
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
                        <span className="text-warning">未分配部门</span>
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
                    <span className="num text-body-compact text-muted-foreground">
                        {formatDateTime(
                            new Date(
                                row.original.created_at * 1000,
                            ).toISOString(),
                            "full",
                        )}
                    </span>
                ),
            },
        ],
        [
            roleNameById,
            canReadOrganization,
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
                description="点击人员行，统一管理资料、角色与数据范围。"
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
                        disabled={
                            !hasPermission(
                                profileQuery.data?.permissions,
                                "admin:create",
                            ) || !assignableRolesQuery.isSuccess
                        }
                        onClick={() => setCreating(true)}
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
            {assignableRolesQuery.isError &&
                hasPermission(
                    profileQuery.data?.permissions,
                    "admin:create",
                ) && (
                    <BusinessFailureState
                        id="accounts-role-options-retry"
                        title="新建账号所需角色加载失败"
                        error={assignableRolesQuery.error}
                        onRetry={() => void assignableRolesQuery.refetch()}
                    />
                )}
            <ListWorkSurface
                ariaLabel="账号列表"
                toolbar={
                    <ListWorkspaceFilterBar
                        morePresentation="popover"
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
                        rowLabel={(row) =>
                            `打开${row.name || row.account}的人员资料`
                        }
                        onRowOpen={(row) =>
                            router.push(
                                `/system/accounts/${encodeURIComponent(row.id)}`,
                            )
                        }
                        rowCount={rows.length}
                        layout="flush"
                        loading={adminsQuery.isPending}
                        defaultColumnPinning={{
                            left: ["identity"],
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

            {creating ? (
                <AccountFormDialog
                    onCreated={(accountName) => {
                        setSetupMessage(
                            "账号已创建，正在打开人员资料继续配置。",
                        )
                        void adminsQuery
                            .refetch()
                            .then((result) => {
                                const created = result.data?.find(
                                    (row) => row.account === accountName,
                                )
                                if (created)
                                    router.push(
                                        `/system/accounts/${encodeURIComponent(created.id)}`,
                                    )
                                else
                                    setSetupMessage(
                                        "账号已创建。请刷新列表并点击姓名继续配置，无需重复创建账号。",
                                    )
                            })
                            .catch(() =>
                                setSetupMessage(
                                    "账号已创建，请刷新列表后点击姓名继续配置。",
                                ),
                            )
                    }}
                    mode="create"
                    account={null}
                    roleOptions={assignableRolesQuery.data ?? []}
                    onOpenChange={(open) => {
                        if (!open) setCreating(false)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
