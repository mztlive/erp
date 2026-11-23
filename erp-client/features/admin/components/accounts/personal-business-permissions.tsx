"use client"

import * as React from "react"
import { ChevronDownIcon, InfoIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    DiscardConfirmDialog,
} from "@/components/business"
import {
    Alert,
    AlertAction,
    AlertDescription,
    AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import type { OrgUnit } from "@/features/organization/types"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { usePersonScopes } from "../../hooks/use-person-data-scopes"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { summarizePersonScopeViews } from "../../lib/account-detail-view"
import { PersonalGrantForm } from "./personal-grant-form"
import { PersonalPolicyWorkspace } from "./personal-policy-workspace"

export function PersonalBusinessPermissions({
    userId,
    name,
    units,
    ready,
    canRead,
    canCreate,
    draft,
    onDraftChange,
    focusedResource,
    onFocusedResourceHandled,
}: {
    userId: string
    name: string
    units: OrgUnit[]
    ready: boolean
    canRead: boolean
    canCreate: boolean
    draft: PersonalGrantDraft | null
    onDraftChange: (draft: PersonalGrantDraft | null) => void
    focusedResource?: string | null
    onFocusedResourceHandled?: () => void
}) {
    const query = usePersonScopes(userId, canRead)
    const [editing, setEditing] = React.useState<string | null>(
        draft?.values.resource ?? null,
    )
    const [discardUnavailableOpen, setDiscardUnavailableOpen] =
        React.useState(false)
    const previous = React.useRef(draft)
    React.useEffect(() => {
        if (previous.current && !draft) setEditing(null)
        previous.current = draft
    }, [draft])

    const labels = React.useMemo(
        () => new Map(units.map((u) => [u.id, u.name])),
        [units],
    )
    const summary = React.useMemo(
        () =>
            query.data ? summarizePersonScopeViews(query.data, labels) : null,
        [labels, query.data],
    )

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
        <DetailWorkspaceSection
            title="数据范围"
            headingId="person-scopes-title"
            ariaLabel="人员数据范围"
            width="full"
            badge={
                summary ? (
                    <div className="flex flex-wrap items-center gap-1.5">
                        <Badge variant="secondary" className="num">
                            {summary.configurable.length} 项可配置
                        </Badge>
                        {summary.unconfiguredCount > 0 ? (
                            <StatusBadge
                                tone="warning"
                                label={`${summary.unconfiguredCount} 项待配置`}
                            />
                        ) : null}
                    </div>
                ) : null
            }
        >
            {unavailableDraft ? (
                <Alert variant="warning">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                        {resourceLabel(editing)}的数据范围草稿暂无法继续编辑
                    </AlertTitle>
                    <AlertDescription>
                        草稿已保留，当前权限或部门信息不足，暂不能继续保存。恢复后可继续核对，也可以放弃草稿。
                    </AlertDescription>
                    <AlertAction>
                        <Button
                            id="person-scope-unavailable-discard"
                            type="button"
                            variant="outline"
                            size="xs"
                            onClick={() => setDiscardUnavailableOpen(true)}
                        >
                            放弃草稿
                        </Button>
                    </AlertAction>
                    <DiscardConfirmDialog
                        idPrefix="person-scope-unavailable-discard-dialog"
                        open={discardUnavailableOpen}
                        onOpenChange={setDiscardUnavailableOpen}
                        title="放弃未保存的数据范围？"
                        description="当前未保存的数据范围草稿将被清除，放弃后无法恢复。"
                        confirmLabel="放弃草稿"
                        cancelLabel="继续核对"
                        onConfirm={() => {
                            setDiscardUnavailableOpen(false)
                            onDraftChange(null)
                            setEditing(null)
                        }}
                    />
                </Alert>
            ) : null}

            {!canRead ? (
                <p className="text-muted-foreground">
                    没有查看数据范围的权限，请联系管理员。
                </p>
            ) : query.isError ? (
                <BusinessFailureState
                    id="person-scopes-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !query.data ? (
                <div
                    className="grid gap-6 border-y border-border/60 py-6 lg:grid-cols-[19rem_minmax(0,1fr)]"
                    aria-busy="true"
                >
                    <div className="space-y-2.5">
                        <div className="h-8 animate-pulse rounded-md bg-muted" />
                        <div className="h-12 animate-pulse rounded-md bg-muted/70" />
                        <div className="h-12 animate-pulse rounded-md bg-muted/55" />
                        <div className="h-12 animate-pulse rounded-md bg-muted/40" />
                    </div>
                    <div className="space-y-4">
                        <div className="h-6 w-40 animate-pulse rounded bg-muted" />
                        <div className="h-24 animate-pulse rounded-lg bg-muted/60" />
                        <div className="h-28 animate-pulse rounded-lg bg-muted/45" />
                    </div>
                </div>
            ) : !query.data.businesses.length ? (
                <p className="border-y border-border/60 py-8 text-muted-foreground">
                    尚无需要说明的数据访问政策，请先核对有效角色。
                </p>
            ) : (
                <div className="space-y-4">
                    {!canCreate ? (
                        <Alert variant="info">
                            <InfoIcon aria-hidden="true" />
                            <AlertTitle>当前为只读模式</AlertTitle>
                            <AlertDescription>
                                你可以查看此人的各业务数据范围；修改范围需要人员范围配置与组织管理权限。
                            </AlertDescription>
                        </Alert>
                    ) : !ready ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            部门信息尚未就绪，读取成功后即可配置范围。
                        </p>
                    ) : null}

                    {editing ? (
                        <p className="text-xs text-muted-foreground">
                            正在配置「{resourceLabel(editing)}
                            」，请先保存或取消后再切换其他业务。
                        </p>
                    ) : null}

                    <PersonalPolicyWorkspace
                        data={query.data}
                        labels={labels}
                        canEdit={canCreate && ready}
                        editing={editing}
                        focusedResource={focusedResource}
                        onFocusedResourceHandled={onFocusedResourceHandled}
                        onEdit={(resource) => setEditing(resource)}
                    />

                    {editing && ready && canCreate && !unavailableDraft ? (
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
                                toast.add({
                                    title: `${name}的${resourceLabel(editing)}数据范围已保存`,
                                    type: "success",
                                    timeout: 4000,
                                })
                            }
                            onReload={() => void query.refetch()}
                        />
                    ) : null}
                </div>
            )}

            {canRead &&
            query.isSuccess &&
            query.data.retired_items.length > 0 ? (
                <Collapsible className="border-t border-border/60 pt-3">
                    <CollapsibleTrigger
                        id="person-scope-retired-details"
                        className="group flex w-fit cursor-pointer items-center gap-1.5 rounded-sm py-1 text-xs text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                    >
                        <ChevronDownIcon
                            className="size-3.5 transition-transform group-aria-expanded:rotate-180"
                            aria-hidden="true"
                        />
                        <span className="num">
                            {query.data.retired_items.length} 项旧范围已停用
                        </span>
                        <span>（仅保留审计记录）</span>
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                        <ul className="mt-2 space-y-1.5 pl-5 text-xs text-muted-foreground">
                            {query.data.retired_items.map((item) => (
                                <li key={`${item.resource}:${item.action}`}>
                                    <span className="font-medium text-foreground/80">
                                        {resourceLabel(item.resource)} ·{" "}
                                        {actionLabel(item.action)}
                                    </span>
                                    ：{item.reason}
                                </li>
                            ))}
                        </ul>
                    </CollapsibleContent>
                </Collapsible>
            ) : null}
        </DetailWorkspaceSection>
    )
}
