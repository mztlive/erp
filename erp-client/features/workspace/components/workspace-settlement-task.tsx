"use client"

import * as React from "react"

import { DetailWorkspaceMeta } from "@/components/business"
import { useWorkspaceInspectorMode } from "@/components/business/workspace-task-pane"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SettlementCenter } from "@/features/supplier-settlements/components/settlement-center"
import { useSettlementDetailQuery } from "@/features/supplier-settlements/hooks/queries"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"

import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 供应商结算复核：以本地页签状态承载 W27 正式作业，不污染工作台 URL。 */
export function WorkspaceSettlementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <WorkspaceSettlementTaskContent
            key={item.workItemId}
            item={item}
            onTaskCompleted={onTaskCompleted}
        />
    )
}

function WorkspaceSettlementTaskContent({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [urlState, setUrlState] = React.useState<SettlementsUrlState>(() => ({
        view: "review_by_me",
        page: 1,
        statementId: item.businessObjectId,
        workItemId: item.workItemId,
        queueContextId: item.queueContextId,
        from: "W01",
        section: "items",
    }))
    const patchUrl = React.useCallback(
        (patch: Partial<SettlementsUrlState>) =>
            setUrlState((current) => ({ ...current, ...patch })),
        [],
    )

    const inspector = useWorkspaceInspectorMode()
    const account = useAccountProfileQuery()
    const canDetail = hasPermission(
        account.data?.permissions,
        "supplier_settlement_statement:detail",
    )
    const detail = useSettlementDetailQuery(
        item.businessObjectId,
        item.workItemId,
        !account.isPending && canDetail,
    )
    const statement = canDetail ? detail.data?.statement : undefined
    const forbidden = !account.isPending && !canDetail
    const headerTitle = forbidden ? "业务详情" : item.workItemTypeLabel
    const rawStatementNo =
        statement?.statementNo || stripDocumentNumberPrefix(item.stableNumber)
    const statementNo =
        !forbidden && rawStatementNo && rawStatementNo !== headerTitle
            ? rawStatementNo
            : ""
    const rawSupplier = statement?.supplierName || item.counterpartyName
    const supplier =
        !forbidden &&
        rawSupplier &&
        rawSupplier !== headerTitle &&
        rawSupplier !== statementNo
            ? rawSupplier
            : ""
    const idPrefix = `workspace-settlement-${toAutomationIdSegment(item.workItemId)}`
    const headerItem = inspector
        ? {
              ...item,
              allowedActions: item.allowedActions.filter(
                  (action) => action !== "REASSIGN" && action !== "CLOSE",
              ),
          }
        : item

    return (
        <WorkspaceProcessingPane
            archetype="action"
            className="@container/settlement-task"
            idPrefix={idPrefix}
            header={
                <WorkspaceTaskIdentityHeader
                    item={headerItem}
                    title={headerTitle}
                    compact
                    subtitle={
                        <DetailWorkspaceMeta
                            items={[
                                supplier || null,
                                statementNo ? (
                                    <span
                                        key="number"
                                        className="num select-all"
                                        data-workspace-document-number={
                                            statementNo
                                        }
                                    >
                                        {statementNo}
                                    </span>
                                ) : null,
                                !forbidden && statement?.periodLabel ? (
                                    <span key="period" className="num">
                                        {statement.periodLabel}
                                    </span>
                                ) : null,
                                item.dueAt ? (
                                    <span key="deadline" className="num">
                                        截止 {item.dueAtLabel}
                                    </span>
                                ) : null,
                            ]}
                        />
                    }
                />
            }
            aria-label="当前供应商结算复核任务"
        >
            <SettlementCenter
                statementId={item.businessObjectId}
                workItemId={item.workItemId}
                urlState={urlState}
                patchUrl={patchUrl}
                onBack={() => undefined}
                embedded
                idPrefix={idPrefix}
                fullTaskHref={
                    inspector
                        ? buildWorkspaceDetailHref("tasks", item.workItemId)
                        : undefined
                }
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceProcessingPane>
    )
}
