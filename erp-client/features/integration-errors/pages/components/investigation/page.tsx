import { useCallback, useEffect, useState, type ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceMeta,
} from "@/components/business"
import type { UnifiedTaskWorkbenchContextRail } from "@/components/business/unified-task-workbench"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isApiError } from "@/lib/api/errors"
import { formatDateTime } from "@/lib/datetime"
import { displayName } from "@/lib/display-name"
import { hasPermission } from "@/lib/permissions"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"

import {
    InvestigationActionForm,
    InvestigationFeedback,
    InvestigationFooter,
} from "./actions"
import { fetchInvestigationDetail } from "./api"
import { InvestigationCanvas, type InvestigationAmount } from "./canvas"
import { isInvestigationTerminal, useInvestigationCommands } from "./commands"
import {
    InvestigationDiagnosticRail,
    type DiagnosticTab,
} from "./diagnostic-rail"
import type { InvestigationFieldView } from "./field-table"

export type InvestigationWorkbenchSlots = {
    canRead: boolean
    canReassign: boolean
    contextRail?: UnifiedTaskWorkbenchContextRail
    subtitle?: ReactNode
    children: ReactNode
}

export type EmbeddedInvestigationOptions = {
    idPrefix: string
    workItemId: string
    inspectorMode: boolean
    number?: string
    title: string
    amount?: InvestigationAmount
    deadline?: { date: string; label: string }
    renderWorkbench: (slots: InvestigationWorkbenchSlots) => ReactNode
}

function headerMeta(
    number: string | undefined,
    category: string | undefined,
    deadline: EmbeddedInvestigationOptions["deadline"],
) {
    return (
        <DetailWorkspaceMeta
            className="min-w-0"
            items={[
                number ? (
                    <span key="number" className="num select-all">
                        {number}
                    </span>
                ) : null,
                category,
                deadline ? (
                    <span key="deadline">
                        截止{" "}
                        <time className="num" dateTime={deadline.date}>
                            {deadline.label ||
                                formatDateTime(deadline.date, "default")}
                        </time>
                    </span>
                ) : null,
            ]}
        />
    )
}

export function EmbeddedIntegrationInvestigation({
    forcedTaskId,
    forcedDifferenceId,
    onTaskCompleted,
    options,
}: {
    forcedTaskId?: string
    forcedDifferenceId?: string
    onTaskCompleted?: (workItemId: string) => void
    options: EmbeddedInvestigationOptions
}) {
    const { idPrefix, workItemId, inspectorMode, renderWorkbench } = options
    const profile = useAccountProfileQuery()
    const account = profile.data
    const permissions = account?.permissions
    const canRead =
        hasPermission(permissions, "work_item:detail") &&
        hasPermission(
            permissions,
            forcedDifferenceId
                ? "reconciliation_difference:detail"
                : "integration_error_task:detail",
        )
    const targetId = forcedDifferenceId ?? forcedTaskId ?? ""
    const itemType = forcedDifferenceId
        ? "RECONCILIATION_DIFFERENCE"
        : "ERROR_TASK"
    const detail = useQuery({
        // 与独立治理页的详情查询共用失效根，按待办关联区分详情快照。
        queryKey: [
            "integration-errors",
            "item",
            itemType,
            targetId,
            "work-item",
            workItemId,
            account?.userid,
            account?.policy_version,
            account?.organization_version,
        ],
        queryFn: () => fetchInvestigationDetail(itemType, targetId, workItemId),
        enabled: canRead && Boolean(targetId),
        retry: false,
    })
    const deniedByServer =
        isApiError(detail.error) &&
        [401, 403, 404].includes(detail.error.status ?? 0)
    const permitted = canRead && !deniedByServer
    const item = permitted ? detail.data?.item : undefined
    const refetch = async () => {
        const result = await detail.refetch()
        return result.isSuccess
    }
    const commands = useInvestigationCommands({
        item,
        workItemId,
        permissions: permitted ? permissions : undefined,
        userId: profile.data?.userid,
        refetch,
        onTaskCompleted,
    })
    const [railCollapsed, setRailCollapsed] = useState(inspectorMode)
    const [tab, setTab] = useState<DiagnosticTab>("payload")
    const [focusPayload, setFocusPayload] = useState(false)
    const [fieldView, setFieldView] = useState<InvestigationFieldView>({
        filter: "",
        page: 0,
    })
    const onPayloadFocused = useCallback(() => setFocusPayload(false), [])

    useEffect(() => {
        if (!permitted) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") {
                setRailCollapsed((current) => !current)
            } else if (action === "history") {
                setTab("history")
                setRailCollapsed(false)
                window.requestAnimationFrame(() => {
                    document
                        .getElementById(`${idPrefix}-rail-tab-history`)
                        ?.focus()
                })
            }
        })
    }, [idPrefix, permitted])

    const number = [
        displayName(item?.identity.number, item?.identity.id),
        options.number,
    ].find((value) => value && !options.title.includes(value))
    const category = item?.classification.label
    const categoryInTitle = Boolean(
        category && options.title.includes(category),
    )
    const subtitle =
        permitted && !inspectorMode
            ? headerMeta(
                  number,
                  categoryInTitle ? undefined : category,
                  options.deadline,
              )
            : null

    const renderState = (children: ReactNode) =>
        renderWorkbench({
            canRead: permitted,
            canReassign: false,
            subtitle,
            children,
        })

    if (
        profile.isPending ||
        (canRead && detail.isPending && (forcedTaskId || forcedDifferenceId))
    ) {
        return renderState(
            <div
                className="space-y-3 py-4"
                aria-busy="true"
                aria-label="正在加载调查详情"
            >
                <div className="h-14 animate-pulse rounded-md bg-surface-sunken" />
                <div className="h-32 animate-pulse rounded-md bg-surface-sunken" />
            </div>,
        )
    }
    if (profile.isError || (detail.isError && !item)) {
        return renderState(
            <div className="py-4">
                <BusinessFailureState
                    idPrefix={`${idPrefix}-failure`}
                    title="调查详情加载失败"
                    error={profile.error ?? detail.error}
                    onRetry={() =>
                        void (profile.isError
                            ? profile.refetch()
                            : detail.refetch())
                    }
                />
            </div>,
        )
    }
    if (!canRead || !item) {
        return renderState(
            <div className="py-4">
                <BusinessEmptyState
                    kind={!canRead ? "no-scope" : "no-data"}
                    title={
                        !canRead
                            ? "当前账号无权查看调查详情"
                            : "未找到当前调查记录"
                    }
                    description={
                        !canRead ? undefined : "请刷新任务以核对关联记录。"
                    }
                    action={
                        canRead ? (
                            <Button
                                id={`${idPrefix}-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重试
                            </Button>
                        ) : null
                    }
                />
            </div>,
        )
    }

    const terminal =
        isInvestigationTerminal(item) ||
        (commands.state.status === "settled" &&
            Boolean(commands.state.result.terminal))
    const contextRail: UnifiedTaskWorkbenchContextRail = {
        collapsible: true,
        // 检查器所在页面的 #main-content 可能比检查器宽，使用工作台的静态堆叠分支。
        resizable: !inspectorMode,
        allowOnFullWidth: true,
        defaultWidthPercent: 30,
        minWidthPercent: 24,
        maxWidthPercent: 52,
        storageKey: "erp.workspace.investigationRailWidth",
        collapsed: railCollapsed,
        onCollapsedChange: setRailCollapsed,
        label: "诊断载荷与重放日志",
        content: (
            <InvestigationDiagnosticRail
                item={item}
                idPrefix={`${idPrefix}-rail`}
                tab={tab}
                onTabChange={setTab}
                focusPayload={focusPayload}
                onPayloadFocused={onPayloadFocused}
            />
        ),
    }

    return renderWorkbench({
        canRead: true,
        canReassign:
            !terminal &&
            !commands.locked &&
            commands.state.status !== "conflict" &&
            item.workItem?.allowedActions.includes("REASSIGN") === true &&
            hasPermission(permissions, "work_item:manage"),
        subtitle,
        contextRail,
        children: (
            <>
                <InvestigationCanvas
                    item={item}
                    idPrefix={idPrefix}
                    amount={options.amount}
                    attemptCount={detail.data?.attemptCount}
                    terminal={terminal}
                    canQuery={commands.actions.includes(
                        "QUERY_ORIGINAL_RESULT",
                    )}
                    canOpenSettings={hasPermission(
                        permissions,
                        "supplier_api_connection:detail",
                    )}
                    classificationInHeader={!inspectorMode && Boolean(category)}
                    titleInHeader={
                        !inspectorMode &&
                        options.title.includes(item.businessObject.title)
                    }
                    deadline={inspectorMode ? options.deadline : undefined}
                    fieldView={fieldView}
                    onFieldViewChange={setFieldView}
                    onFocusPayload={() => {
                        setRailCollapsed(false)
                        setTab("payload")
                        setFocusPayload(true)
                    }}
                    feedback={
                        <InvestigationFeedback
                            idPrefix={idPrefix}
                            item={item}
                            commands={commands}
                        />
                    }
                    actionForm={
                        <InvestigationActionForm
                            idPrefix={idPrefix}
                            item={item}
                            commands={commands}
                        />
                    }
                />
                <InvestigationFooter
                    item={item}
                    idPrefix={idPrefix}
                    commands={commands}
                />
            </>
        ),
    })
}
