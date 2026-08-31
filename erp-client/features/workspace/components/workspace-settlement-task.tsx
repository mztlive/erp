"use client"

import * as React from "react"

import { WorkspaceTaskPane } from "@/components/business"
import { SettlementCenter } from "@/features/supplier-settlements/components/settlement-center"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 供应商结算复核：以本地页签状态承载 W27 正式作业，不污染工作台 URL。 */
export function WorkspaceSettlementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [urlState, setUrlState] = React.useState<SettlementsUrlState>({
        view: "review_by_me",
        page: 1,
        statementId: item.businessObjectId,
        workItemId: item.workItemId,
        queueContextId: item.queueContextId,
        from: "W01",
        section: "review",
    })
    const patchUrl = React.useCallback(
        (patch: Partial<SettlementsUrlState>) =>
            setUrlState((current) => ({ ...current, ...patch })),
        [],
    )

    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前供应商结算复核任务"
        >
            <SettlementCenter
                statementId={item.businessObjectId}
                workItemId={item.workItemId}
                urlState={urlState}
                patchUrl={patchUrl}
                onBack={() => undefined}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceTaskPane>
    )
}
