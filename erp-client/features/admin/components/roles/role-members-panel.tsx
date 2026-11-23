"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowUpRightIcon, SearchIcon, UsersIcon, XIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"
import type { useAdminsQuery } from "../../hooks/queries"
import { accountInitial } from "../../lib/account-detail-view"
import { adminAccountStatusBadge } from "../../lib/account-status"
import { summarizeBoundAccounts } from "../../lib/role-detail-view"
import type { AdminRole } from "../../types"

type MemberStatusFilter = "all" | "active" | "inactive"

/**
 * 角色关联人员视图（纯白通栏细线列表）：
 * 展示所有绑定当前角色的人员账号及启停状态，支持搜索与状态筛选，并直达人员资料页。
 */
export function RoleMembersPanel({
    role,
    adminsQuery,
}: {
    role: AdminRole
    adminsQuery: ReturnType<typeof useAdminsQuery>
}) {
    const [search, setSearch] = React.useState("")
    const [statusFilter, setStatusFilter] =
        React.useState<MemberStatusFilter>("all")

    const summary = React.useMemo(
        () => summarizeBoundAccounts(adminsQuery.data ?? [], role.id),
        [adminsQuery.data, role.id],
    )

    const effectiveFilter: MemberStatusFilter =
        statusFilter === "inactive" && summary.inactiveCount === 0
            ? "all"
            : statusFilter

    const keyword = search.trim().toLocaleLowerCase()
    const filteredMembers = React.useMemo(() => {
        return summary.members.filter((account) => {
            if (effectiveFilter === "active" && account.status !== "active") {
                return false
            }
            if (effectiveFilter === "inactive" && account.status === "active") {
                return false
            }
            if (!keyword) return true
            return `${account.name} ${account.account}`
                .toLocaleLowerCase()
                .includes(keyword)
        })
    }, [effectiveFilter, keyword, summary.members])

    const hasFilter = Boolean(keyword) || effectiveFilter !== "all"
    const accountsHref = `/system/accounts?q=${encodeURIComponent(role.name)}`

    return (
        <DetailWorkspaceSection
            title="关联人员"
            headingId="role-detail-members-title"
            width="full"
            badge={
                adminsQuery.isSuccess ? (
                    <Badge variant="secondary" className="num">
                        {summary.members.length} 人
                    </Badge>
                ) : null
            }
            subtitle="角色操作权限保存后，将对所有启用中的关联人员即时生效；人员角色分配请在人员账号中调整。"
            actions={
                <>
                    {adminsQuery.isSuccess && summary.members.length > 0 ? (
                        <InputGroup className="w-full sm:w-64">
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                id="role-detail-members-search"
                                type="search"
                                aria-label="搜索关联人员姓名或登录账号"
                                placeholder="搜索姓名或登录账号"
                                value={search}
                                onChange={(event) =>
                                    setSearch(event.target.value)
                                }
                            />
                        </InputGroup>
                    ) : null}
                    <Button
                        id="role-detail-members-open-accounts"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.inlineLinkButton}
                        render={<Link href={accountsHref} />}
                    >
                        人员账号
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                </>
            }
        >
            {adminsQuery.isError ? (
                <BusinessFailureState
                    id="role-detail-members-retry"
                    title="关联人员加载失败"
                    error={adminsQuery.error}
                    onRetry={() => void adminsQuery.refetch()}
                />
            ) : adminsQuery.isPending && !adminsQuery.data ? (
                <div className="space-y-3" aria-busy="true">
                    <div className="h-12 animate-pulse rounded-md bg-muted/70" />
                    <div className="h-12 animate-pulse rounded-md bg-muted/55" />
                    <div className="h-12 animate-pulse rounded-md bg-muted/40" />
                </div>
            ) : summary.members.length === 0 ? (
                <div className="flex flex-wrap items-center justify-between gap-4 border-y border-border/60 py-6">
                    <div className="flex min-w-0 items-start gap-3">
                        <UsersIcon
                            className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <div className="min-w-0 space-y-1">
                            <p className="font-medium text-foreground">
                                尚未绑定任何人员账号
                            </p>
                            <p className="text-xs text-muted-foreground">
                                可在人员账号管理中将此角色分配给对应岗位人员。
                            </p>
                        </div>
                    </div>
                    <Button
                        id="role-detail-members-empty-accounts"
                        type="button"
                        variant="outline"
                        size="sm"
                        render={<Link href="/system/accounts" />}
                    >
                        前往分配角色
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                </div>
            ) : (
                <div className="space-y-3">
                    {summary.inactiveCount > 0 || hasFilter ? (
                        <div className="flex flex-wrap items-center justify-between gap-3">
                            {summary.inactiveCount > 0 ? (
                                <div
                                    role="group"
                                    aria-label="按账号状态筛选"
                                    className="flex flex-wrap items-center gap-1.5"
                                >
                                    {(
                                        [
                                            {
                                                id: "all" as const,
                                                label: "全部",
                                                count: summary.members.length,
                                            },
                                            {
                                                id: "active" as const,
                                                label: "启用",
                                                count: summary.activeCount,
                                            },
                                            {
                                                id: "inactive" as const,
                                                label: "已停用",
                                                count: summary.inactiveCount,
                                            },
                                        ] as const
                                    ).map((option) => {
                                        const active =
                                            effectiveFilter === option.id
                                        return (
                                            <button
                                                key={option.id}
                                                id={`role-detail-members-status-${option.id}`}
                                                type="button"
                                                aria-pressed={active}
                                                onClick={() =>
                                                    setStatusFilter(option.id)
                                                }
                                                className={cn(
                                                    "inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-primary",
                                                    active
                                                        ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                                        : "bg-muted/65 text-muted-foreground hover:bg-muted hover:text-foreground",
                                                )}
                                            >
                                                <span>{option.label}</span>
                                                <span className="num text-tiny opacity-80">
                                                    {option.count}
                                                </span>
                                            </button>
                                        )
                                    })}
                                </div>
                            ) : (
                                <span />
                            )}
                            {hasFilter ? (
                                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                    <span className="num">
                                        匹配 {filteredMembers.length} 人
                                    </span>
                                    <Button
                                        id="role-detail-members-clear-filter"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        onClick={() => {
                                            setSearch("")
                                            setStatusFilter("all")
                                        }}
                                    >
                                        <XIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        重置筛选
                                    </Button>
                                </div>
                            ) : null}
                        </div>
                    ) : null}

                    {filteredMembers.length > 0 ? (
                        <ul className="divide-y divide-border/60 border-y border-border/60">
                            {filteredMembers.map((member) => {
                                const isActive = member.status === "active"
                                const badge = adminAccountStatusBadge(
                                    member.status,
                                )
                                return (
                                    <li
                                        key={member.id}
                                        className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 py-3"
                                    >
                                        <div className="flex min-w-0 flex-1 items-center gap-3">
                                            <div
                                                aria-hidden="true"
                                                className={cn(
                                                    "flex size-8 shrink-0 items-center justify-center rounded-md border text-xs font-semibold",
                                                    isActive
                                                        ? "border-info-border/70 bg-info-soft text-info-soft-foreground"
                                                        : "border-neutral-border bg-neutral-soft text-neutral-soft-foreground",
                                                )}
                                            >
                                                {accountInitial(
                                                    member.name,
                                                    member.account,
                                                )}
                                            </div>
                                            <div className="min-w-0 flex-1 space-y-0.5">
                                                <div className="flex flex-wrap items-center gap-2">
                                                    <span className="truncate font-medium text-foreground">
                                                        {member.name}
                                                    </span>
                                                    <StatusBadge
                                                        {...badge}
                                                        appearance={
                                                            isActive
                                                                ? "quiet"
                                                                : "badge"
                                                        }
                                                    />
                                                </div>
                                                <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                                                    <span className="num">
                                                        账号：{member.account}
                                                    </span>
                                                    <span
                                                        aria-hidden="true"
                                                        className="select-none text-muted-foreground/40"
                                                    >
                                                        ·
                                                    </span>
                                                    <span className="num">
                                                        {member.role_ids
                                                            .length > 1
                                                            ? `共绑定 ${member.role_ids.length} 个角色`
                                                            : "仅绑定此角色"}
                                                    </span>
                                                    {member.created_at ? (
                                                        <>
                                                            <span
                                                                aria-hidden="true"
                                                                className="select-none text-muted-foreground/40"
                                                            >
                                                                ·
                                                            </span>
                                                            <span className="num">
                                                                创建于{" "}
                                                                {formatDateTime(
                                                                    new Date(
                                                                        member.created_at *
                                                                            1000,
                                                                    ).toISOString(),
                                                                    "full",
                                                                )}
                                                            </span>
                                                        </>
                                                    ) : null}
                                                </div>
                                            </div>
                                        </div>
                                        <Link
                                            id={`role-detail-member-${toAutomationIdSegment(member.id)}`}
                                            href={`/system/accounts/${encodeURIComponent(member.id)}`}
                                            className="inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-primary transition-colors hover:bg-sidebar-accent/70 focus-visible:outline-2 focus-visible:outline-primary"
                                        >
                                            <span>查看人员</span>
                                            <ArrowUpRightIcon
                                                className="size-3.5 shrink-0"
                                                aria-hidden="true"
                                            />
                                        </Link>
                                    </li>
                                )
                            })}
                        </ul>
                    ) : (
                        <div className="flex flex-col items-start gap-2.5 border-y border-border/60 py-8">
                            <p
                                role="status"
                                className="text-body-compact text-muted-foreground"
                            >
                                没有符合当前筛选条件的关联人员。
                            </p>
                            <Button
                                id="role-detail-members-empty-reset"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => {
                                    setSearch("")
                                    setStatusFilter("all")
                                }}
                            >
                                清除筛选
                            </Button>
                        </div>
                    )}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}
