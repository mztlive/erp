"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import {
    EyeIcon,
    MoreHorizontalIcon,
    ShieldOffIcon,
    Trash2Icon,
} from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { AccountDraft } from "@/features/admin/account-form-dialog"
import type {
    AccessChangeCommand,
    AccessGovernancePolicyView,
    AccessListView,
    AuditEventRow,
    FieldPolicyRow,
    RoleRow,
    ScopeRow,
    UserRow,
} from "@/features/access-audit/types"
import { riskLabel } from "@/features/access-audit/lib/risk-labels"
import { formatDateTime } from "@/lib/datetime"

type AccountFormState = {
    mode: "create" | "edit"
    account: AccountDraft | null
}

type DeletingAccountState = {
    id: string
    account: string
}

type DeletingRoleState = {
    id: string
    name: string
}

type AccessColumnsInput = {
    data?: AccessListView
    policies?: AccessGovernancePolicyView
    router: { push: (href: string) => void }
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openExplain: (type: "ROLE" | "USER", id: string) => void
    openEvent: (id: string) => void
    startChange: (command: AccessChangeCommand) => Promise<void>
    setAccountForm: React.Dispatch<
        React.SetStateAction<AccountFormState | null>
    >
    setDeletingAccount: React.Dispatch<
        React.SetStateAction<DeletingAccountState | null>
    >
    setDeletingRole: React.Dispatch<
        React.SetStateAction<DeletingRoleState | null>
    >
}

function useAccessColumns({
    data,
    policies,
    router,
    rowFocusRef,
    openExplain,
    openEvent,
    startChange,
    setAccountForm,
    setDeletingAccount,
    setDeletingRole,
}: AccessColumnsInput) {
    const roleColumns = React.useMemo<ColumnDef<RoleRow>[]>(
        () => [
            {
                id: "identity",
                header: "角色",
                cell: ({ row }) => (
                    <div className="min-w-[10rem]">
                        <div className="font-medium">{row.original.name}</div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.roleCode}
                        </div>
                    </div>
                ),
            },
            {
                id: "org",
                header: "组织",
                cell: ({ row }) => row.original.organizationLabel,
            },
            {
                id: "perms",
                header: "模块与动作权限",
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {row.original.permissionSummary}
                    </span>
                ),
            },
            {
                id: "scope",
                header: "数据范围",
                cell: ({ row }) => row.original.dataScopeSummary,
            },
            {
                id: "status",
                header: "状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "version",
                header: "版本",
                cell: ({ row }) => (
                    <span className="num text-xs">
                        v{row.original.permissionVersion.split("-").at(-1)}
                    </span>
                ),
            },
            {
                id: "risk",
                header: "风险",
                cell: ({ row }) =>
                    row.original.riskFlags.length ? (
                        <div className="flex flex-wrap gap-1">
                            {row.original.riskFlags.map((f) => (
                                <Badge key={f} variant="warning">
                                    {riskLabel(f)}
                                </Badge>
                            ))}
                        </div>
                    ) : (
                        "—"
                    ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => {
                    const role = row.original
                    const version =
                        data?.permissionVersion ?? role.permissionVersion
                    const canAdjust =
                        role.status === "enabled" &&
                        !role.riskFlags.includes("HIGH_PRIVILEGE")
                    const canExpand = role.riskFlags.includes("HIGH_PRIVILEGE")
                    const canDisable =
                        role.status === "enabled" &&
                        role.riskFlags.includes("PENDING_DISABLE")

                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                ref={(el) => {
                                    rowFocusRef.current.set(role.id, el)
                                }}
                                onClick={() => openExplain("ROLE", role.id)}
                            >
                                <EyeIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                有效权限
                            </Button>
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() =>
                                    router.push(`/system/roles/${role.id}/edit`)
                                }
                            >
                                编辑
                            </Button>
                            <DropdownMenu>
                                <DropdownMenuTrigger
                                    render={
                                        <Button
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label={`${role.name} 更多操作`}
                                        />
                                    }
                                >
                                    <MoreHorizontalIcon aria-hidden="true" />
                                </DropdownMenuTrigger>
                                <DropdownMenuContent
                                    align="end"
                                    className="min-w-40"
                                >
                                    {canAdjust ? (
                                        <DropdownMenuItem
                                            onClick={() =>
                                                void startChange({
                                                    subjectType: "ROLE",
                                                    subjectId: role.id,
                                                    action: "UPDATE_ROLE_PERMISSIONS",
                                                    expectedPermissionVersion:
                                                        version,
                                                    reasonCode: "SECURITY_OPS",
                                                    idempotencyKey: "pending",
                                                    changeSet: [
                                                        {
                                                            targetReference:
                                                                "W22.publish",
                                                            operation: "REMOVE",
                                                        },
                                                    ],
                                                })
                                            }
                                        >
                                            调整权限
                                        </DropdownMenuItem>
                                    ) : null}
                                    {canExpand ? (
                                        <DropdownMenuItem
                                            onClick={() =>
                                                void startChange({
                                                    subjectType: "ROLE",
                                                    subjectId: role.id,
                                                    action: "UPDATE_ROLE_PERMISSIONS",
                                                    expectedPermissionVersion:
                                                        version,
                                                    reasonCode: "SECURITY_OPS",
                                                    idempotencyKey: "pending",
                                                    changeSet: [
                                                        {
                                                            targetReference:
                                                                "sensitive.field.expand",
                                                            operation: "ADD",
                                                            valueReference:
                                                                "FULL_COMPANY",
                                                        },
                                                    ],
                                                })
                                            }
                                        >
                                            扩权（将阻断）
                                        </DropdownMenuItem>
                                    ) : null}
                                    {canDisable ? (
                                        <DropdownMenuItem
                                            onClick={() =>
                                                void startChange({
                                                    subjectType: "ROLE",
                                                    subjectId: role.id,
                                                    action: "DISABLE_ROLE",
                                                    expectedPermissionVersion:
                                                        version,
                                                    reasonCode: "SECURITY_OPS",
                                                    idempotencyKey: "pending",
                                                    changeSet: [
                                                        {
                                                            targetReference:
                                                                "status",
                                                            operation:
                                                                "REPLACE",
                                                            valueReference:
                                                                "disabled",
                                                        },
                                                    ],
                                                })
                                            }
                                        >
                                            停用
                                        </DropdownMenuItem>
                                    ) : null}
                                    {(canAdjust || canExpand || canDisable) && (
                                        <DropdownMenuSeparator />
                                    )}
                                    <DropdownMenuItem
                                        variant="destructive"
                                        onClick={() =>
                                            setDeletingRole({
                                                id: role.id,
                                                name: role.name,
                                            })
                                        }
                                    >
                                        <Trash2Icon aria-hidden="true" />
                                        删除
                                    </DropdownMenuItem>
                                </DropdownMenuContent>
                            </DropdownMenu>
                        </div>
                    )
                },
            },
        ],
        [
            openExplain,
            startChange,
            router,
            data?.permissionVersion,
            rowFocusRef,
            setDeletingRole,
        ],
    )

    const userColumns = React.useMemo<ColumnDef<UserRow>[]>(
        () => [
            {
                id: "identity",
                header: "用户",
                cell: ({ row }) => (
                    <div className="min-w-[9rem]">
                        <div className="font-medium">
                            {row.original.displayName}
                        </div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.userId}
                        </div>
                    </div>
                ),
            },
            {
                id: "roles",
                header: "当前角色",
                cell: ({ row }) => row.original.activeRoles,
            },
            {
                id: "period",
                header: "有效期间",
                cell: ({ row }) => (
                    <span
                        className="num text-xs text-muted-foreground"
                        title="只读记录；策略未配置时不可编辑预约/到期"
                    >
                        {formatDateTime(row.original.effectiveFrom, "full")}
                        {" ~ "}
                        {row.original.effectiveTo
                            ? formatDateTime(row.original.effectiveTo, "full")
                            : "长期"}
                    </span>
                ),
            },
            {
                id: "scope",
                header: "数据范围",
                cell: ({ row }) => row.original.dataScopeSummary,
            },
            {
                id: "status",
                header: "账号状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "risk",
                header: "风险",
                cell: ({ row }) =>
                    row.original.riskFlags.length ? (
                        <div className="flex flex-wrap gap-1">
                            {row.original.riskFlags.map((f) => (
                                <Badge key={f} variant="warning">
                                    {riskLabel(f)}
                                </Badge>
                            ))}
                        </div>
                    ) : (
                        "—"
                    ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => {
                    const user = row.original
                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                ref={(el) => {
                                    rowFocusRef.current.set(user.id, el)
                                }}
                                onClick={() => openExplain("USER", user.userId)}
                            >
                                <EyeIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                有效权限
                            </Button>
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() =>
                                    setAccountForm({
                                        mode: "edit",
                                        account: {
                                            id: user.userId,
                                            account: user.accountName,
                                            name: user.displayName,
                                            role_ids: [...user.roleIds],
                                        },
                                    })
                                }
                            >
                                编辑
                            </Button>
                            <DropdownMenu>
                                <DropdownMenuTrigger
                                    render={
                                        <Button
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label={`${user.displayName} 更多操作`}
                                        />
                                    }
                                >
                                    <MoreHorizontalIcon aria-hidden="true" />
                                </DropdownMenuTrigger>
                                <DropdownMenuContent
                                    align="end"
                                    className="min-w-40"
                                >
                                    {user.roleAssignmentId ? (
                                        <>
                                            <DropdownMenuItem
                                                variant="destructive"
                                                onClick={() =>
                                                    void startChange({
                                                        subjectType: "USER",
                                                        subjectId: user.userId,
                                                        action: "EMERGENCY_REVOKE_USER_ROLE",
                                                        roleAssignmentId:
                                                            user.roleAssignmentId!,
                                                        expectedPermissionVersion:
                                                            data?.permissionVersion ??
                                                            user.permissionVersion,
                                                        reasonCode:
                                                            "EMERGENCY_STOP_LOSS",
                                                        idempotencyKey:
                                                            "pending",
                                                    })
                                                }
                                            >
                                                <ShieldOffIcon aria-hidden="true" />
                                                紧急撤权
                                            </DropdownMenuItem>
                                            <DropdownMenuSeparator />
                                        </>
                                    ) : null}
                                    <DropdownMenuItem
                                        variant="destructive"
                                        onClick={() =>
                                            setDeletingAccount({
                                                id: user.userId,
                                                account: user.accountName,
                                            })
                                        }
                                    >
                                        <Trash2Icon aria-hidden="true" />
                                        删除
                                    </DropdownMenuItem>
                                </DropdownMenuContent>
                            </DropdownMenu>
                        </div>
                    )
                },
            },
        ],
        [
            openExplain,
            startChange,
            data?.permissionVersion,
            rowFocusRef,
            setAccountForm,
            setDeletingAccount,
        ],
    )

    const scopeColumns = React.useMemo<ColumnDef<ScopeRow>[]>(
        () => [
            {
                id: "subject",
                header: "主体",
                cell: ({ row }) => (
                    <div>
                        <div className="font-medium">
                            {row.original.subjectLabel}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.subjectType === "ROLE"
                                ? "角色"
                                : "用户"}
                        </div>
                    </div>
                ),
            },
            {
                id: "type",
                header: "范围类型",
                cell: ({ row }) => row.original.scopeTypeLabel,
            },
            {
                id: "targets",
                header: "范围对象",
                cell: ({ row }) => row.original.scopeTargets,
            },
            {
                id: "risk",
                header: "风险",
                cell: ({ row }) =>
                    row.original.riskFlags.length
                        ? row.original.riskFlags
                              .map((f) => riskLabel(f))
                              .join("、")
                        : "—",
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => (
                    <div className="flex justify-end">
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            ref={(el) => {
                                rowFocusRef.current.set(row.original.id, el)
                            }}
                            onClick={() =>
                                openExplain(
                                    row.original.subjectType,
                                    row.original.subjectId,
                                )
                            }
                        >
                            <EyeIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            有效权限
                        </Button>
                    </div>
                ),
            },
        ],
        [openExplain, rowFocusRef],
    )

    const fieldColumns = React.useMemo<ColumnDef<FieldPolicyRow>[]>(
        () => [
            {
                id: "target",
                header: "策略目标",
                cell: ({ row }) => (
                    <div>
                        <div className="font-medium">
                            {row.original.targetLabel}
                        </div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.policyTargetId}
                        </div>
                    </div>
                ),
            },
            {
                id: "subject",
                header: "适用",
                cell: ({ row }) => row.original.subjectLabel,
            },
            {
                id: "caps",
                header: "访问能力",
                cell: ({ row }) =>
                    data?.emptyReason === "FIELD_MASKED"
                        ? "****"
                        : row.original.capabilitySummary,
            },
            {
                id: "mode",
                header: "可编辑",
                cell: ({ row }) =>
                    row.original.editable ? (
                        <Badge variant="success">可调整</Badge>
                    ) : (
                        <Badge variant="default">只读</Badge>
                    ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) =>
                    row.original.editable ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                const gp = policies?.fieldPolicyGranularity
                                if (!gp || gp.state !== "CONFIGURED") return
                                void startChange({
                                    subjectType: "FIELD_POLICY",
                                    subjectId: row.original.id,
                                    action: "UPDATE_FIELD_POLICY",
                                    granularityPolicyVersion: gp.policyVersion,
                                    policyTargetId: row.original.policyTargetId,
                                    accessCapabilities: ["MASKED", "VISIBLE"],
                                    expectedPermissionVersion:
                                        data?.permissionVersion ??
                                        row.original.permissionVersion,
                                    reasonCode: "SECURITY_OPS",
                                    idempotencyKey: "pending",
                                })
                            }}
                        >
                            调整能力
                        </Button>
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            策略缺失时只读
                        </span>
                    ),
            },
        ],
        [data?.emptyReason, data?.permissionVersion, policies, startChange],
    )

    const auditColumns = React.useMemo<ColumnDef<AuditEventRow>[]>(
        () => [
            {
                id: "time",
                header: "时间",
                cell: ({ row }) => (
                    <span className="num text-xs">
                        {formatDateTime(row.original.recordedAt, "full")}
                    </span>
                ),
            },
            {
                id: "actor",
                header: "操作者",
                cell: ({ row }) => (
                    <div className="min-w-[7rem]">
                        <div className="font-medium">
                            {row.original.actorLabel}
                        </div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.actorId}
                        </div>
                    </div>
                ),
            },
            {
                id: "role",
                header: "责任角色",
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {row.original.actorRole}
                    </span>
                ),
            },
            {
                id: "action",
                header: "动作",
                cell: ({ row }) => row.original.actionLabel,
            },
            {
                id: "object",
                header: "对象",
                cell: ({ row }) => <span>{row.original.objectLabel}</span>,
            },
            {
                id: "result",
                header: "结果",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        label={row.original.resultLabel}
                        tone={row.original.resultTone}
                    />
                ),
            },
            {
                id: "fields",
                header: "变更字段",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.changedFieldDisplay !== "—"
                            ? row.original.changedFieldDisplay
                            : "—"}
                    </span>
                ),
            },
            {
                id: "trace",
                header: "请求追踪号",
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.traceId}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "查看",
                cell: ({ row }) => (
                    <div className="flex justify-end">
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            ref={(el) => {
                                rowFocusRef.current.set(
                                    row.original.auditEventId,
                                    el,
                                )
                            }}
                            onClick={() => openEvent(row.original.auditEventId)}
                        >
                            详情
                        </Button>
                    </div>
                ),
            },
        ],
        [openEvent, rowFocusRef],
    )

    return {
        auditColumns,
        fieldColumns,
        roleColumns,
        scopeColumns,
        userColumns,
    }
}

export { useAccessColumns }
