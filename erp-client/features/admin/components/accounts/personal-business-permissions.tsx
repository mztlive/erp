"use client"
import * as React from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { OrgUnit } from "@/features/organization/types"
import { usePersonScopes } from "../../hooks/use-person-data-scopes"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { personScopeDescription } from "../../api/person-data-scopes"
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
                    (business) => business.resource === editing,
                )))

    return (
        <section className="space-y-4 rounded-lg border p-5 text-sm">
            <div className="space-y-1">
                <h2 className="font-semibold">业务数据范围</h2>
                <p className="text-xs text-muted-foreground">
                    选择要调整的业务，为{name}
                    设置可以处理哪些数据。未设置的操作暂不能访问业务数据。
                </p>
            </div>
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
                    尚无可配置的业务操作，请先为此人分配有效角色。
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
                    <div className="divide-y">
                        <div
                            className="hidden gap-4 pb-2 text-xs text-muted-foreground md:grid md:grid-cols-[minmax(6rem,1fr)_minmax(0,2fr)_minmax(0,2fr)_6rem]"
                            aria-hidden="true"
                        >
                            <span>业务</span>
                            <span>已有操作权限</span>
                            <span>当前数据范围</span>
                            <span className="text-right">设置</span>
                        </div>
                        {query.data.businesses.map((business) => {
                            const data = query.data!
                            const scopes = business.actions.map((action) =>
                                data.items.find(
                                    (item) =>
                                        item.resource === business.resource &&
                                        item.action === action,
                                ),
                            )
                            const mixed =
                                new Set(
                                    scopes.map((scope) =>
                                        JSON.stringify(scope?.expression),
                                    ),
                                ).size > 1
                            const saved = scopes.some(Boolean)
                            const segment = toAutomationIdSegment(
                                business.resource,
                            )
                            return (
                                <div
                                    key={business.resource}
                                    className="space-y-3 py-4"
                                >
                                    <div className="grid items-start gap-3 md:grid-cols-[minmax(6rem,1fr)_minmax(0,2fr)_minmax(0,2fr)_6rem] md:gap-4">
                                        <h3 className="font-medium">
                                            {resourceLabel(business.resource)}
                                        </h3>
                                        <p className="text-xs leading-6 text-muted-foreground">
                                            {business.actions
                                                .map(actionLabel)
                                                .join("、")}
                                        </p>
                                        {mixed ? (
                                            <details className="text-xs leading-6">
                                                <summary
                                                    id={`person-scope-${segment}-details`}
                                                    className="cursor-pointer font-medium"
                                                >
                                                    按操作分别设置 · 展开查看
                                                </summary>
                                                <ul className="mt-2 space-y-1 text-muted-foreground">
                                                    {business.actions.map(
                                                        (action, index) => (
                                                            <li key={action}>
                                                                {actionLabel(
                                                                    action,
                                                                )}
                                                                ：
                                                                {personScopeDescription(
                                                                    scopes[
                                                                        index
                                                                    ],
                                                                    labels,
                                                                )}
                                                            </li>
                                                        ),
                                                    )}
                                                </ul>
                                            </details>
                                        ) : (
                                            <p
                                                className={`text-xs leading-6 ${saved ? "" : "text-amber-700"}`}
                                            >
                                                {personScopeDescription(
                                                    scopes[0],
                                                    labels,
                                                )}
                                            </p>
                                        )}
                                        {canCreate && (
                                            <Button
                                                id={`person-scope-${segment}-edit`}
                                                variant="outline"
                                                size="sm"
                                                className="w-fit md:justify-self-end"
                                                disabled={
                                                    Boolean(editing) || !ready
                                                }
                                                onClick={() => {
                                                    setNotice(null)
                                                    setEditing(
                                                        business.resource,
                                                    )
                                                }}
                                            >
                                                {saved
                                                    ? "修改范围"
                                                    : "设置范围"}
                                            </Button>
                                        )}
                                    </div>
                                    {editing === business.resource &&
                                        ready &&
                                        canCreate && (
                                            <PersonalGrantForm
                                                key={business.resource}
                                                userId={userId}
                                                resource={business.resource}
                                                name={name}
                                                data={data}
                                                units={units}
                                                draft={draft}
                                                onDraftChange={onDraftChange}
                                                onDone={() => {
                                                    onDraftChange(null)
                                                    setEditing(null)
                                                }}
                                                onSaved={() =>
                                                    setNotice(
                                                        `${name}的${resourceLabel(business.resource)}数据范围已保存。`,
                                                    )
                                                }
                                                onReload={() =>
                                                    void query.refetch()
                                                }
                                            />
                                        )}
                                </div>
                            )
                        })}
                    </div>
                </>
            )}
        </section>
    )
}
