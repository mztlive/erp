"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    PageHeader,
    PageScaffold,
    SequentialProcessBar,
} from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useTeamOptionsQuery } from "@/hooks/use-options"
import { hasAnyPermission } from "@/lib/permissions"
import { Button } from "@/components/ui/button"

import { BlockedApprovalList } from "../components/blocked-approval-list"
import { TaskActionPanel } from "../components/task-action-panel"
import { TaskDetailCard } from "../components/task-detail-card"
import { TaskFilterBar } from "../components/task-filter-bar"
import { TaskListPanel } from "../components/task-list-panel"
import { TaskNavigation } from "../components/task-navigation"
import { useQueueUrlState } from "../hooks/use-queue-url-state"
import { useTaskAction } from "../hooks/use-task-action"
import { useUnifiedTaskQueueQuery } from "../hooks/queries"
import { buildHandlerHref } from "../lib/handler-destination"
import { containsAction, toResponsibilityStatus } from "../lib/responsibility"

export function UnifiedTaskQueuePage() {
    const router = useRouter()
    const {
        approvalBlockers,
        scope,
        family,
        due,
        priorities,
        sort,
        historyStatus,
        workItemType,
        queryText,
        queueContextId,
        currentWorkItemId,
        replaceUrl,
    } = useQueueUrlState()
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const canManage = hasAnyPermission(permissions, [
        "work_item:reassign",
        "work_item:manage",
    ])
    const canRecover = hasAnyPermission(permissions, [
        "approval_instance:recover",
    ])
    const [searchDraft, setSearchDraft] = React.useState(queryText)

    const queueQuery = useUnifiedTaskQueueQuery({
        scope,
        family,
        workItemType,
        historyStatus,
        due,
        priorities,
        query: queryText || undefined,
        sort,
        queueContextId,
        currentWorkItemId,
        viewerKey: profileQuery.data
            ? `${profileQuery.data.userid}:${[...profileQuery.data.role_ids].sort().join(",")}`
            : "profile-pending",
    })
    const items = queueQuery.data?.items ?? []
    const selectedIndex = currentWorkItemId
        ? Math.max(
              0,
              items.findIndex((item) => item.workItemId === currentWorkItemId),
          )
        : 0
    const selected = items[selectedIndex]
    const canSelectReassignmentTarget = Boolean(
        selected?.allowedActions.includes("REASSIGN"),
    )
    const teamOptions = useTeamOptionsQuery(canSelectReassignmentTarget)
    const taskAction = useTaskAction(selected)

    const handlerHref = selected ? buildHandlerHref(selected) : null
    const readonly = scope === "history"
    const responsibilityStatus = selected
        ? toResponsibilityStatus(selected, scope, profileQuery.data?.userid)
        : "blocked"

    return (
        <PageScaffold>
            <PageHeader
                title={approvalBlockers ? "受阻审批" : "统一待办"}
                description={
                    approvalBlockers
                        ? "重试当前受阻步骤，不改变处理人和审批路径。"
                        : "按当前责任处理任务；业务结论在对应页面提交。"
                }
                metadata={
                    approvalBlockers ? null : (
                        <DataFreshness
                            updatedAt="当前查询"
                            state={queueQuery.isFetching ? "syncing" : "fresh"}
                        />
                    )
                }
                actions={
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => {
                            if (approvalBlockers) {
                                replaceUrl({ approvalBlockers: false })
                            } else {
                                void queueQuery.refetch()
                            }
                        }}
                    >
                        <RefreshCwIcon aria-hidden="true" />
                        {approvalBlockers ? "返回待办" : "刷新"}
                    </Button>
                }
            />

            {approvalBlockers ? (
                <BlockedApprovalList canRecover={canRecover} />
            ) : (
                <>
                    <TaskFilterBar
                        scope={scope}
                        family={family}
                        workItemType={workItemType}
                        historyStatus={historyStatus}
                        due={due}
                        priorities={priorities}
                        sort={sort}
                        queryText={queryText}
                        searchDraft={searchDraft}
                        onSearchDraftChange={setSearchDraft}
                        canManage={canManage}
                        canRecover={canRecover}
                        total={queueQuery.data?.total ?? 0}
                        replaceUrl={replaceUrl}
                    />

                    {queueQuery.isPending ? (
                        <div className="h-96 animate-pulse rounded-lg bg-muted" />
                    ) : queueQuery.isError ? (
                        <BusinessFailureState
                            error={queueQuery.error}
                            onRetry={() => void queueQuery.refetch()}
                        />
                    ) : items.length === 0 ? (
                        <BusinessEmptyState
                            kind="no-tasks"
                            title="当前范围没有待办"
                            description="可调整责任范围或清除筛选后继续查看。"
                        />
                    ) : selected ? (
                        <div className="grid min-h-[32rem] gap-4 lg:grid-cols-[minmax(18rem,34%)_minmax(0,66%)]">
                            <TaskListPanel
                                items={items}
                                selectedWorkItemId={selected.workItemId}
                                onSelect={(item) =>
                                    replaceUrl({
                                        currentWorkItemId: item.workItemId,
                                    })
                                }
                            />

                            <section
                                className="space-y-4"
                                aria-label="当前任务"
                            >
                                <SequentialProcessBar
                                    current={selectedIndex + 1}
                                    total={items.length}
                                    responsibilityStatus={responsibilityStatus}
                                    responsibilityStatusLabel={
                                        selected.responsibilityLabel
                                    }
                                    processLabel="打开业务对象"
                                    showProcessNext={false}
                                    showProcess={!readonly}
                                    processDisabled={!handlerHref}
                                    pending={taskAction.isPending}
                                    onBack={() => router.back()}
                                    onProcess={() => {
                                        if (handlerHref)
                                            router.push(handlerHref)
                                    }}
                                    onProcessNext={() => undefined}
                                    onStartProcessing={
                                        containsAction(
                                            selected,
                                            "START_PROCESSING",
                                        )
                                            ? () => {
                                                  void taskAction.runResponsibilityAction(
                                                      "START_PROCESSING",
                                                  )
                                              }
                                            : undefined
                                    }
                                />

                                <TaskDetailCard
                                    selected={selected}
                                    handlerHref={handlerHref}
                                    readonly={readonly}
                                    isError={taskAction.isError}
                                    error={taskAction.error}
                                    onRetry={() => void queueQuery.refetch()}
                                    onAction={taskAction.setAction}
                                />

                                {taskAction.action ? (
                                    <TaskActionPanel
                                        action={taskAction.action}
                                        form={taskAction.actionForm}
                                        teamOptions={teamOptions.data ?? []}
                                        items={items}
                                        selected={selected}
                                        isPending={taskAction.isPending}
                                        onCancel={() => {
                                            taskAction.setAction(null)
                                            taskAction.actionForm.reset()
                                        }}
                                        onSubmit={
                                            taskAction.runResponsibilityAction
                                        }
                                    />
                                ) : null}

                                <TaskNavigation
                                    previousDisabled={selectedIndex === 0}
                                    nextDisabled={
                                        selectedIndex >= items.length - 1
                                    }
                                    onPrevious={() =>
                                        replaceUrl({
                                            currentWorkItemId:
                                                items[selectedIndex - 1]
                                                    ?.workItemId,
                                        })
                                    }
                                    onNext={() =>
                                        replaceUrl({
                                            currentWorkItemId:
                                                items[selectedIndex + 1]
                                                    ?.workItemId,
                                        })
                                    }
                                />
                            </section>
                        </div>
                    ) : null}
                </>
            )}
        </PageScaffold>
    )
}
