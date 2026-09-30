"use client"
import * as React from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
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
    const [editing, setEditing] = React.useState(Boolean(draft))
    const previous = React.useRef(draft)
    React.useEffect(() => {
        if (previous.current && !draft) setEditing(false)
        previous.current = draft
    }, [draft])
    const labels = new Map(units.map((u) => [u.id, u.name]))
    return (
        <section className="space-y-4 rounded-lg border p-5 text-sm">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <h2 className="font-semibold">业务权限与数据范围</h2>
                    <p className="text-xs text-muted-foreground">
                        角色决定{name}
                        能做什么；这里决定每项操作可以处理哪些数据。
                    </p>
                </div>
                {canCreate && (
                    <Button
                        id="account-person-scope-edit"
                        size="sm"
                        disabled={
                            editing || !ready || !query.data?.businesses.length
                        }
                        onClick={() => setEditing(true)}
                    >
                        设置数据范围
                    </Button>
                )}
            </div>
            {!canRead ? (
                <p>没有查看范围的权限。</p>
            ) : query.isError ? (
                <BusinessFailureState
                    id="person-scopes-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !query.data ? (
                <p role="status">正在读取数据范围…</p>
            ) : (
                <>
                    {!query.data.businesses.length && (
                        <p className="text-xs text-muted-foreground">
                            尚无可配置的业务操作，请先为此人分配有效角色。
                        </p>
                    )}
                    {editing && ready && (
                        <PersonalGrantForm
                            userId={userId}
                            name={name}
                            data={query.data}
                            units={units}
                            draft={draft}
                            onDraftChange={onDraftChange}
                            onDone={() => {
                                onDraftChange(null)
                                setEditing(false)
                            }}
                            onReload={() => void query.refetch()}
                        />
                    )}
                    <div className="divide-y">
                        {query.data.businesses.map((business) => (
                            <div
                                key={business.resource}
                                className="space-y-2 py-3"
                            >
                                <h3 className="font-medium">
                                    {resourceLabel(business.resource)}
                                </h3>
                                <div className="grid gap-2 sm:grid-cols-2">
                                    {business.actions.map((action) => (
                                        <p key={action} className="text-xs">
                                            <span className="text-muted-foreground">
                                                {actionLabel(action)}：
                                            </span>
                                            {personScopeDescription(
                                                query.data.items.find(
                                                    (s) =>
                                                        s.resource ===
                                                            business.resource &&
                                                        s.action === action,
                                                ),
                                                labels,
                                            )}
                                        </p>
                                    ))}
                                </div>
                            </div>
                        ))}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        “待设置”表示尚未获得该操作的数据范围。选择业务后可一次设置全部已有操作，也可展开“按操作区分”单独设置。
                    </p>
                </>
            )}
        </section>
    )
}
