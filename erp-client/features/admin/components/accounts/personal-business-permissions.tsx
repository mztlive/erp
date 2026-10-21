"use client"
import * as React from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import type { OrgUnit } from "@/features/organization/types"
import { usePersonScopes } from "../../hooks/use-person-data-scopes"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { PersonalPolicyWorkspace } from "./personal-policy-workspace"
import { PersonalGrantForm } from "./personal-grant-form"

export function PersonalBusinessPermissions({
    userId,
    name,
    units,
    ready,
    canRead,
    canCreate,
    draft,
    onDraftChange,
}: {
    userId: string
    name: string
    units: OrgUnit[]
    ready: boolean
    canRead: boolean
    canCreate: boolean
    draft: PersonalGrantDraft | null
    onDraftChange: (draft: PersonalGrantDraft | null) => void
}) {
    const query = usePersonScopes(userId, canRead)
    const [editing, setEditing] = React.useState<string | null>(
        draft?.values.resource ?? null,
    )
    const [notice, setNotice] = React.useState<string | null>(null)
    const previous = React.useRef(draft)
    React.useEffect(() => {
        if (previous.current && !draft) setEditing(null)
        previous.current = draft
    }, [draft])
    const labels = new Map(units.map((u) => [u.id, u.name]))
    const unavailableDraft =
        editing &&
        (!canRead ||
            !canCreate ||
            !ready ||
            (query.isSuccess &&
                !query.data.businesses.some(
                    (business) =>
                        business.resource === editing &&
                        business.configurable_actions.length > 0,
                )))

    return (
        <section
            className="min-w-0 space-y-4 text-sm"
            aria-label="人员数据范围"
        >
            {notice && (
                <p role="status" className="rounded-md bg-muted p-3">
                    {notice}
                </p>
            )}
            {unavailableDraft && (
                <div role="status" className="space-y-2 rounded-md border p-3">
                    <p>
                        {resourceLabel(editing)}
                        的草稿已保留，当前权限或部门信息不足，暂不能继续编辑。恢复后可继续核对，也可以放弃草稿。
                    </p>
                    <Button
                        id="person-scope-unavailable-discard"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => {
                            if (
                                window.confirm("确定放弃当前未保存的数据范围？")
                            ) {
                                onDraftChange(null)
                                setEditing(null)
                            }
                        }}
                    >
                        放弃草稿
                    </Button>
                </div>
            )}
            {!canRead ? (
                <p>没有查看数据范围的权限，请联系管理员。</p>
            ) : query.isError ? (
                <BusinessFailureState
                    id="person-scopes-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !query.data ? (
                <p role="status">正在读取数据范围…</p>
            ) : !query.data.businesses.length ? (
                <p className="text-muted-foreground">
                    尚无需要说明的数据访问政策，请先核对有效角色。
                </p>
            ) : (
                <>
                    {!canCreate && (
                        <p className="text-xs text-muted-foreground">
                            当前只能查看范围，修改需要人员范围配置权限。
                        </p>
                    )}
                    {canCreate && !ready && (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            部门信息尚未就绪，读取成功后即可设置范围。
                        </p>
                    )}
                    {editing && (
                        <p className="text-xs text-muted-foreground">
                            正在设置{resourceLabel(editing)}
                            ，请先保存或取消，再调整其他业务。
                        </p>
                    )}
                    <PersonalPolicyWorkspace
                        data={query.data}
                        labels={labels}
                        canEdit={canCreate && ready}
                        editing={editing}
                        onEdit={(resource) => {
                            setNotice(null)
                            setEditing(resource)
                        }}
                    />
                    {query.data.retired_items.length > 0 && (
                        <details className="border-t pt-3 text-xs text-muted-foreground">
                            <summary
                                id="person-scope-retired-details"
                                className="cursor-pointer leading-6"
                            >
                                {query.data.retired_items.length}{" "}
                                项旧范围已停用，仅保留审计记录
                            </summary>
                            <ul className="mt-2 space-y-2">
                                {query.data.retired_items.map((item) => (
                                    <li key={`${item.resource}:${item.action}`}>
                                        {resourceLabel(item.resource)} ·{" "}
                                        {actionLabel(item.action)}：
                                        {item.reason}
                                    </li>
                                ))}
                            </ul>
                        </details>
                    )}
                    {editing && ready && canCreate && !unavailableDraft && (
                        <PersonalGrantForm
                            key={editing}
                            userId={userId}
                            resource={editing}
                            name={name}
                            data={query.data}
                            units={units}
                            draft={draft}
                            onDraftChange={onDraftChange}
                            onDone={() => {
                                onDraftChange(null)
                                setEditing(null)
                            }}
                            onSaved={() =>
                                setNotice(
                                    `${name}的${resourceLabel(editing)}数据范围已保存。`,
                                )
                            }
                            onReload={() => void query.refetch()}
                        />
                    )}
                </>
            )}
        </section>
    )
}
