"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowUpRightIcon, SearchIcon } from "lucide-react"

import { BusinessFailureState, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { accountPermissionGroups } from "../../lib/account-permission-preview"
import { useRolesQuery } from "../../hooks/queries"
import { useAccountPermissionScopes } from "../../hooks/use-account-permission-scopes"
import type { AdminAccount } from "../../types"

const SCOPE_LABEL: Record<string, string> = {
    company: "公司级",
    organization: "组织",
    team: "团队",
    self_owned: "本人负责",
    collaborative: "协作参与",
}
const id = "governance-admin-account-permissions"

/** 账号页原地只读预览；沿用公司商品池的窄栏、分隔区块与固定页脚。 */
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
    const rolesQuery = useRolesQuery()
    const scopesQuery = useAccountPermissionScopes(account, open)
    const [keyword, setKeyword] = React.useState("")
    React.useEffect(() => {
        if (open) setKeyword("")
    }, [account?.id, open])
    const preview =
        account && rolesQuery.data
            ? accountPermissionGroups(account, rolesQuery.data)
            : null
    const q = keyword.trim().toLowerCase()
    const groups =
        preview?.groups
            .map((group) => ({
                ...group,
                items: group.items.filter((item) =>
                    `${group.name} ${item.label} ${item.sources.map((source) => source.name).join(" ")}`
                        .toLowerCase()
                        .includes(q),
                ),
            }))
            .filter((group) => group.items.length) ?? []
    const roleName = (roleId: string) =>
        rolesQuery.data?.find((role) => role.id === roleId)?.name ??
        "角色信息待确认"
    return (
        <QuickPreviewSheet
            idPrefix={`${id}-sheet`}
            open={open}
            onOpenChange={onOpenChange}
            onOpenChangeComplete={(isOpen) => {
                if (!isOpen) onClosed()
            }}
            size="preview"
            overlayClassName="bg-black/20 supports-backdrop-filter:backdrop-blur-none"
            contentClassName="data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px] [&_[data-slot=sheet-header]]:gap-3 [&_[data-slot=sheet-header]]:px-7 [&_[data-slot=sheet-header]]:pt-10 [&_[data-slot=sheet-title]]:text-xl [&_[data-slot=sheet-title]]:leading-8 [&_[data-slot=sheet-title]]:font-semibold [&_[data-slot=quick-preview-identity]]:text-xs [&_[data-slot=quick-preview-content]]:px-7 [&_[data-slot=sheet-footer]]:flex-row [&_[data-slot=sheet-footer]]:justify-end [&_[data-slot=sheet-footer]]:px-7 [&_[data-slot=sheet-footer]]:py-4"
            title={
                account
                    ? `${account.name || account.account}的权限`
                    : "账号权限"
            }
            identity={account ? `登录账号：${account.account}` : undefined}
            description="查看角色授予的操作权限与数据范围配置。"
            summary={
                <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="secondary">只读</Badge>
                    <span className="text-xs text-muted-foreground">
                        已分配 {account?.role_ids.length ?? 0} 个角色
                    </span>
                </div>
            }
            footer={
                <>
                    <Button
                        id={`${id}-close`}
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        关闭
                    </Button>
                    <Button
                        id={`${id}-adjust-roles`}
                        onClick={onAdjustRoles}
                        disabled={!account}
                    >
                        调整账号角色
                    </Button>
                </>
            }
        >
            <div className="space-y-6 text-sm">
                {rolesQuery.isError ? (
                    <BusinessFailureState
                        error={rolesQuery.error}
                        title="角色权限加载失败"
                        action={
                            <Button
                                id={`${id}-retry`}
                                variant="outline"
                                onClick={() => void rolesQuery.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : !preview ? (
                    <div
                        role="status"
                        className="py-6 text-sm text-muted-foreground"
                    >
                        正在读取角色权限…
                    </div>
                ) : (
                    <>
                        <section className="border-b border-border pb-6">
                            <h3 className="text-xs font-medium text-muted-foreground">
                                角色授予的操作权限
                            </h3>
                            <p className="mt-2 text-[32px] font-semibold leading-10 tracking-tight">
                                {preview.allPermissions ? (
                                    "全部操作权限"
                                ) : (
                                    <>
                                        <span className="num">
                                            {preview.permissionCount}
                                        </span>
                                        <span className="ml-2 text-sm font-normal text-muted-foreground">
                                            项
                                        </span>
                                    </>
                                )}
                            </p>
                            <p className="mt-2 text-xs leading-5 text-muted-foreground">
                                {preview.allPermissions
                                    ? "包含全权角色授权。"
                                    : `覆盖 ${preview.groups.length} 个模块，同一权限按一项展示。`}
                                实际访问仍受数据范围和业务状态限制。
                            </p>
                            {preview.missingRoleCount > 0 ? (
                                <p
                                    role="status"
                                    className="mt-2 text-xs text-warning"
                                >
                                    有 {preview.missingRoleCount}{" "}
                                    个角色未返回，当前展示不完整。
                                </p>
                            ) : null}
                        </section>
                        <section className="space-y-3 border-b border-border pb-6">
                            <h3 className="font-medium">操作权限</h3>
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    id={`${id}-search`}
                                    type="search"
                                    value={keyword}
                                    onChange={(event) =>
                                        setKeyword(event.target.value)
                                    }
                                    placeholder="搜索模块、操作或来源角色"
                                    aria-label="搜索账号权限"
                                />
                            </InputGroup>
                            {groups.length ? (
                                <div className="divide-y divide-border">
                                    {groups.map((group) => (
                                        <details
                                            key={group.name}
                                            open={q ? true : undefined}
                                            className="group py-3"
                                        >
                                            <summary
                                                id={`${id}-group-${Array.from(group.name, (character) => character.codePointAt(0)!.toString(16)).join("-")}`}
                                                className="cursor-pointer text-sm font-medium focus-visible:outline-2 focus-visible:outline-offset-2"
                                            >
                                                {group.name}
                                                <span className="num ml-2 text-xs font-normal text-muted-foreground">
                                                    {group.items.length} 项
                                                </span>
                                            </summary>
                                            <ul className="mt-3 space-y-4">
                                                {group.items.map((item) => (
                                                    <li
                                                        key={item.code}
                                                        className="space-y-1"
                                                    >
                                                        <p className="break-words text-[13px]">
                                                            {item.label}
                                                        </p>
                                                        <p className="text-xs leading-5 text-muted-foreground">
                                                            来源：
                                                            {item.sources
                                                                .map(
                                                                    (source) =>
                                                                        source.name,
                                                                )
                                                                .join("、")}
                                                        </p>
                                                    </li>
                                                ))}
                                            </ul>
                                        </details>
                                    ))}
                                </div>
                            ) : (
                                <p className="py-3 text-xs text-muted-foreground">
                                    {q
                                        ? "没有匹配的权限，请更换关键词。"
                                        : "当前角色未配置操作权限。"}
                                </p>
                            )}
                        </section>
                        <section className="space-y-3 border-b border-border pb-6">
                            <h3 className="font-medium">已分配角色</h3>
                            <p className="text-xs leading-5 text-muted-foreground">
                                修改角色权限会影响绑定该角色的所有账号。
                            </p>
                            <ul className="space-y-3">
                                {preview.assigned.map((role) => (
                                    <li
                                        key={role.id}
                                        className="flex items-center justify-between gap-4"
                                    >
                                        <span>{role.name}</span>
                                        <Link
                                            id={`${id}-role-${toAutomationIdSegment(role.id)}-edit`}
                                            href={`/system/roles/${role.id}/edit`}
                                            className="inline-flex shrink-0 items-center gap-1 text-xs text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2"
                                        >
                                            编辑角色权限
                                            <ArrowUpRightIcon
                                                className="size-3.5"
                                                aria-hidden="true"
                                            />
                                        </Link>
                                    </li>
                                ))}
                            </ul>
                            {!account?.role_ids.length ? (
                                <p className="text-xs text-muted-foreground">
                                    尚未分配角色。
                                </p>
                            ) : null}
                        </section>
                    </>
                )}
                <section className="space-y-3">
                    <h3 className="font-medium">数据范围</h3>
                    {scopesQuery.isError ? (
                        <div className="space-y-2">
                            <p className="text-xs text-muted-foreground">
                                数据范围读取失败，不能据此判断为没有限制。
                            </p>
                            <Button
                                id={`${id}-scopes-retry`}
                                variant="outline"
                                size="sm"
                                onClick={() => void scopesQuery.refetch()}
                            >
                                重试数据范围
                            </Button>
                        </div>
                    ) : scopesQuery.isPending ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在读取数据范围…
                        </p>
                    ) : scopesQuery.data.length ? (
                        <dl className="space-y-4">
                            {scopesQuery.data.map((scope) => (
                                <div
                                    key={scope.id}
                                    className="flex items-baseline justify-between gap-5"
                                >
                                    <dt className="min-w-0 text-xs leading-5 text-muted-foreground">
                                        {scope.subject_type === "user"
                                            ? "账号直接配置"
                                            : `角色 · ${roleName(scope.subject_id)}`}
                                    </dt>
                                    <dd className="min-w-0 text-right text-[13px]">
                                        {SCOPE_LABEL[scope.scope_type] ??
                                            "其它范围"}
                                        {scope.scope_targets.length ? (
                                            <span className="mt-1 block text-xs text-muted-foreground">
                                                已指定{" "}
                                                {scope.scope_targets.length}{" "}
                                                个对象
                                            </span>
                                        ) : null}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    ) : (
                        <p className="text-xs leading-5 text-muted-foreground">
                            未查询到账号或角色的数据范围配置，不代表可访问全部数据。
                        </p>
                    )}
                </section>
                <section className="border-t border-border pt-6">
                    <h3 className="text-xs font-medium text-muted-foreground">
                        查询范围
                    </h3>
                    <p className="mt-2 text-xs leading-5 text-muted-foreground">
                        以上展示当前账号的角色授权与数据范围配置，具体业务操作是否允许，以执行时的权限校验为准。
                    </p>
                </section>
            </div>
        </QuickPreviewSheet>
    )
}
