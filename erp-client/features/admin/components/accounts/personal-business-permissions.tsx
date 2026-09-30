"use client"
import * as React from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api/errors"
import type { ScopeRule } from "@/features/organization/lib/scope-description"
import type { OrgUnit } from "@/features/organization/types"
import {
    usePersonalGrantMutations,
    usePersonalGrants,
} from "../../hooks/use-personal-business-grants"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { PersonalGrantForm } from "./personal-grant-form"

export function PersonalBusinessPermissions({
    userId,
    name,
    scopes,
    units,
    ready,
    canRead,
    canCreate,
    canRevoke,
    draft,
    onDraftChange,
}: {
    userId: string
    draft: PersonalGrantDraft | null
    onDraftChange: (draft: PersonalGrantDraft | null) => void
    name: string
    scopes: readonly ScopeRule[]
    units: OrgUnit[]
    ready: boolean
    canRead: boolean
    canCreate: boolean
    canRevoke: boolean
}) {
    const query = usePersonalGrants(userId, canRead)
    const { revoke } = usePersonalGrantMutations(userId)
    const [editing, setEditing] = React.useState(Boolean(draft))
    const previousDraft = React.useRef(draft)
    React.useEffect(() => {
        if (previousDraft.current && !draft) setEditing(false)
        previousDraft.current = draft
    }, [draft])
    const [confirm, setConfirm] = React.useState<string | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    return (
        <section className="space-y-4 rounded-lg border p-5 text-sm">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <h2 className="font-semibold">
                        业务权限 · 为{name}扩大数据范围
                    </h2>
                    <p className="text-xs leading-5 text-muted-foreground">
                        角色决定能做什么；这里仅为{name}
                        增加某项业务可处理的部门，不影响同角色的其他人员。
                    </p>
                </div>
                {canCreate && (
                    <Button
                        id="account-business-grant-add"
                        size="sm"
                        disabled={
                            editing ||
                            confirm !== null ||
                            !ready ||
                            !query.data?.roles.length ||
                            revoke.isPending
                        }
                        onClick={() => setEditing(true)}
                    >
                        扩大数据范围
                    </Button>
                )}
            </div>
            {!canRead ? (
                <p className="text-xs text-muted-foreground">
                    没有查看个人业务授权的权限。
                </p>
            ) : query.isError && !query.data ? (
                <BusinessFailureState
                    id="account-business-grants-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !query.data ? (
                <p role="status">正在读取业务授权…</p>
            ) : (
                <>
                    {query.isError && (
                        <p role="status" className="text-xs text-amber-700">
                            最新配置读取失败，保留当前选择；保存前请重新核对。
                        </p>
                    )}
                    {!query.data.roles.length && (
                        <p className="text-xs text-muted-foreground">
                            当前没有可按部门扩大的业务操作。请先分配有效角色并勾选相应业务操作；仓库和结算主体范围使用各自的配置入口。
                        </p>
                    )}
                    {!ready && (
                        <p className="text-xs text-muted-foreground">
                            角色范围或部门尚未读取完整，请先完成读取后再授权。
                        </p>
                    )}
                    {editing && ready && canCreate && (
                        <PersonalGrantForm
                            userId={userId}
                            draft={draft}
                            onDraftChange={onDraftChange}
                            name={name}
                            data={query.data}
                            scopes={scopes}
                            units={units}
                            onDone={() => {
                                onDraftChange(null)
                                setEditing(false)
                            }}
                            onReload={() => void query.refetch()}
                        />
                    )}
                    <div className="space-y-2">
                        <h3 className="text-sm font-medium">
                            此人的附加部门范围
                        </h3>
                        {!query.data.items.length ? (
                            <p className="text-xs text-muted-foreground">
                                尚未单独扩大范围，按角色默认范围处理数据；个人限制仍然适用。
                            </p>
                        ) : (
                            <ul className="divide-y">
                                {query.data.items.map((grant) => (
                                    <li
                                        key={grant.id}
                                        className="space-y-2 py-3"
                                    >
                                        <div className="flex flex-wrap items-start justify-between gap-2">
                                            <div className="space-y-1">
                                                <p>
                                                    {resourceLabel(
                                                        grant.resource,
                                                    )}{" "}
                                                    ·{" "}
                                                    {grant.org_unit_ids
                                                        .map(
                                                            (id) =>
                                                                units.find(
                                                                    (unit) =>
                                                                        unit.id ===
                                                                        id,
                                                                )?.name ??
                                                                "部门信息待确认",
                                                        )
                                                        .join("、")}
                                                    {grant.include_descendants
                                                        ? "（含下级）"
                                                        : "（仅本级）"}
                                                </p>
                                                <p className="text-xs text-muted-foreground">
                                                    依据角色：
                                                    {query.data.roles.find(
                                                        (role) =>
                                                            role.id ===
                                                            grant.role_id,
                                                    )?.name ??
                                                        "当前角色已失效或不可用"}{" "}
                                                    · 已设置操作：
                                                    {grant.actions
                                                        .map(actionLabel)
                                                        .join("、")}
                                                </p>
                                                <p className="text-xs text-muted-foreground">
                                                    {grant.active_actions.length
                                                        ? `当前具备操作资格：${grant.active_actions.map(actionLabel).join("、")}；仍受部门状态、个人限制与业务条件约束。`
                                                        : "暂不生效：该人员当前没有通过此角色获得所选操作。"}
                                                </p>
                                            </div>
                                            {canRevoke && (
                                                <Button
                                                    id={`personal-grant-revoke-${toAutomationIdSegment(grant.id)}`}
                                                    variant="ghost"
                                                    size="sm"
                                                    disabled={
                                                        editing ||
                                                        confirm !== null ||
                                                        revoke.isPending
                                                    }
                                                    onClick={() =>
                                                        setConfirm(grant.id)
                                                    }
                                                >
                                                    撤销附加范围
                                                </Button>
                                            )}
                                        </div>
                                        {confirm === grant.id && (
                                            <div className="space-y-2 rounded-md bg-muted p-3 text-xs">
                                                <p>
                                                    撤销后仅移除此条附加范围，角色默认范围及其他附加授权保留。
                                                </p>
                                                <div className="flex gap-2">
                                                    <Button
                                                        id={`personal-grant-revoke-confirm-${toAutomationIdSegment(grant.id)}`}
                                                        size="sm"
                                                        variant="destructive"
                                                        disabled={
                                                            revoke.isPending
                                                        }
                                                        onClick={async () => {
                                                            setError(null)
                                                            try {
                                                                await revoke.mutateAsync(
                                                                    {
                                                                        grant,
                                                                        version:
                                                                            query
                                                                                .data
                                                                                .policy_version,
                                                                    },
                                                                )
                                                                setConfirm(null)
                                                            } catch (failure) {
                                                                setError(
                                                                    getErrorMessage(
                                                                        failure,
                                                                        "撤销失败，请刷新后重试。",
                                                                    ),
                                                                )
                                                            }
                                                        }}
                                                    >
                                                        确认撤销
                                                    </Button>
                                                    <Button
                                                        id={`personal-grant-revoke-cancel-${toAutomationIdSegment(grant.id)}`}
                                                        size="sm"
                                                        variant="outline"
                                                        disabled={
                                                            revoke.isPending
                                                        }
                                                        onClick={() =>
                                                            setConfirm(null)
                                                        }
                                                    >
                                                        保留
                                                    </Button>
                                                </div>
                                            </div>
                                        )}
                                    </li>
                                ))}
                            </ul>
                        )}
                    </div>
                    <p className="text-xs leading-5 text-muted-foreground">
                        角色被停用、移除或不再提供某项操作时，相应附加范围暂不生效；恢复该依据后重新生效。需要永久取消时，请撤销附加范围。
                    </p>
                </>
            )}
            {error && (
                <div role="alert" className="text-xs text-destructive">
                    <p>{error}</p>
                    <Button
                        id="personal-grant-revoke-reload"
                        variant="ghost"
                        size="sm"
                        onClick={() => {
                            setConfirm(null)
                            setError(null)
                            void query.refetch()
                        }}
                    >
                        刷新授权
                    </Button>
                </div>
            )}
        </section>
    )
}
