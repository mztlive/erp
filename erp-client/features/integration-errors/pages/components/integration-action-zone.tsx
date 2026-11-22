import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import * as React from "react"
import { DetailWorkspaceSection } from "@/components/business"
import { ChevronDownIcon, InfoIcon } from "lucide-react"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"

import { ReplacementWorkItemSearchCombobox } from "../../components/replacement-work-item-search-combobox"
import type { TerminalConfirm } from "../../components/terminal-action-dialog"
import { INTEGRATION_ACTION_LABEL } from "../../lib/presentation"
import type {
    IntegrationActionKind,
    IntegrationResolutionItemView,
} from "../../types"
import { IntegrationDirectReconciliation } from "./integration-direct-reconciliation"
import type { IntegrationTaskActionKind } from "../hooks/use-integration-actions"

export function IntegrationActionZone({
    item,
    can,
    formalPending,
    resolvePending,
    pendingTaskAction,
    pendingDirectAction,
    responsibilityStatus,
    comment,
    onCommentChange,
    replacementTaskId,
    onReplacementTaskIdChange,
    reconReasonId,
    onReconReasonIdChange,
    reasonMismatches,
    onTaskAction,
    onDirectAction,
    onRequestTerminal,
    onResolve,
    actionZoneRef,
    detailWorkspace,
}: {
    item: IntegrationResolutionItemView
    can: (action: IntegrationActionKind) => boolean
    formalPending: boolean
    resolvePending?: boolean
    pendingTaskAction?: IntegrationTaskActionKind
    pendingDirectAction?: IntegrationTaskActionKind
    responsibilityStatus: ResponsibilityStatus
    comment: string
    onCommentChange: (value: string) => void
    replacementTaskId: string
    onReplacementTaskIdChange: (value: string) => void
    reconReasonId: string
    onReconReasonIdChange: (value: string) => void
    reasonMismatches: (
        conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE",
    ) => boolean
    onTaskAction: (kind: IntegrationTaskActionKind) => void
    onDirectAction: (kind: IntegrationTaskActionKind) => void
    onRequestTerminal: (confirm: TerminalConfirm) => void
    onResolve: () => void
    actionZoneRef: React.Ref<HTMLDivElement>
    detailWorkspace?: boolean
}) {
    const assignedDisabled =
        responsibilityStatus !== "assigned_to_me" || formalPending
    const hasTaskActions =
        Boolean(item.workItem) &&
        (
            [
                "QUERY_ORIGINAL_RESULT",
                "REPLAY_ORIGINAL",
                "ADD_EVIDENCE",
                "LINK_COMPENSATION",
                "REATTRIBUTE",
                "RESOLVE",
            ] as const
        ).some(can)
    const canClose = item.workItem?.allowedActions.includes("CLOSE") === true
    const direct =
        item.identity.itemType === "RECONCILIATION_DIFFERENCE" &&
        !item.hasWorkItem
    const terminal =
        item.workItem?.status === "COMPLETED" ||
        item.workItem?.status === "CLOSED"

    if (terminal) return null

    const content = (
        <div
            ref={actionZoneRef}
            role="region"
            aria-labelledby="integration-action-heading"
            className={
                detailWorkspace
                    ? "scroll-mt-32 space-y-4"
                    : "space-y-4 border-t border-grid pt-5"
            }
        >
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h3
                    id="integration-action-heading"
                    className={
                        detailWorkspace
                            ? "text-section-title font-semibold tracking-tight"
                            : "font-semibold"
                    }
                >
                    {can("RESOLVE") ? "确认处理" : "处理异常"}
                </h3>
                {can("RESOLVE") ? (
                    <p className="text-xs text-muted-foreground">
                        确认后移出待处理队列
                    </p>
                ) : null}
            </div>
            {!hasTaskActions &&
            !canClose &&
            !direct &&
            item.actionBlockers.length === 0 ? (
                <div className="flex items-start gap-2 text-body-compact text-muted-foreground">
                    <InfoIcon className="mt-0.5 size-4 shrink-0" aria-hidden />
                    <p>
                        {item.workItem?.allowedActions.includes("PROCESS")
                            ? "当前没有可提交的处理动作，请刷新核验结果后重试。"
                            : "当前任务仅供查看。请由具备处理权限的责任人完成核验。"}
                    </p>
                </div>
            ) : null}
            {item.actionBlockers.length > 0 ? (
                <ul className="space-y-1 text-xs text-muted-foreground">
                    {item.actionBlockers.map((b) => (
                        <li key={`${b.action}-${b.code}`}>
                            <span className="font-medium text-foreground">
                                {INTEGRATION_ACTION_LABEL[b.action] ?? b.action}
                            </span>
                            ：{b.message}
                        </li>
                    ))}
                </ul>
            ) : null}

            {hasTaskActions || canClose || direct ? (
                <details className="group">
                    <summary
                        id="integration-action-comment-toggle"
                        className="inline-flex cursor-pointer list-none items-center gap-1 rounded-sm text-body-compact text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                        添加处理说明（可选）
                        <ChevronDownIcon
                            className="size-3.5 transition-transform group-open:rotate-180"
                            aria-hidden
                        />
                    </summary>
                    <div
                        data-slot={
                            detailWorkspace ? "detail-form-grid" : undefined
                        }
                        className="mt-3 max-w-2xl space-y-2"
                    >
                        <Label htmlFor="integration-action-comment">
                            处理说明
                        </Label>
                        <Textarea
                            id="integration-action-comment"
                            rows={2}
                            value={comment}
                            onChange={(e) => onCommentChange(e.target.value)}
                            placeholder="补充本次处理情况"
                            disabled={formalPending}
                        />
                    </div>
                </details>
            ) : null}

            <div className="flex flex-wrap gap-2">
                <WorkspaceTaskFooter>
                    {can("QUERY_ORIGINAL_RESULT") && item.workItem ? (
                        <LoadingButton
                            id="integration-action-query-original-result"
                            loading={
                                pendingTaskAction === "QUERY_ORIGINAL_RESULT"
                            }
                            type="button"
                            disabled={assignedDisabled}
                            onClick={() =>
                                void onTaskAction("QUERY_ORIGINAL_RESULT")
                            }
                        >
                            查询原结果
                        </LoadingButton>
                    ) : null}
                    {can("REPLAY_ORIGINAL") && item.workItem ? (
                        <LoadingButton
                            id="integration-action-replay-original"
                            loading={pendingTaskAction === "REPLAY_ORIGINAL"}
                            type="button"
                            variant="secondary"
                            disabled={assignedDisabled}
                            onClick={() => void onTaskAction("REPLAY_ORIGINAL")}
                        >
                            重新提交
                        </LoadingButton>
                    ) : null}
                    {can("ADD_EVIDENCE") && item.workItem ? (
                        <LoadingButton
                            id="integration-action-add-evidence"
                            loading={pendingTaskAction === "ADD_EVIDENCE"}
                            type="button"
                            variant="outline"
                            disabled={assignedDisabled}
                            onClick={() => void onTaskAction("ADD_EVIDENCE")}
                        >
                            补充证据
                        </LoadingButton>
                    ) : null}
                    {can("LINK_COMPENSATION") && item.workItem ? (
                        <LoadingButton
                            id="integration-action-link-compensation"
                            loading={pendingTaskAction === "LINK_COMPENSATION"}
                            type="button"
                            variant="outline"
                            disabled={assignedDisabled}
                            onClick={() =>
                                void onTaskAction("LINK_COMPENSATION")
                            }
                        >
                            关联补救单据
                        </LoadingButton>
                    ) : null}
                    {can("REATTRIBUTE") && item.workItem ? (
                        <LoadingButton
                            id="integration-action-reatribute"
                            loading={pendingTaskAction === "REATTRIBUTE"}
                            type="button"
                            variant="outline"
                            disabled={assignedDisabled}
                            onClick={() => void onTaskAction("REATTRIBUTE")}
                        >
                            重新匹配归属
                        </LoadingButton>
                    ) : null}
                    {can("RESOLVE") &&
                    item.workItem &&
                    item.resolutionEvidencePolicy ? (
                        <LoadingButton
                            id="integration-action-resolve"
                            type="button"
                            loading={resolvePending}
                            disabled={assignedDisabled}
                            onClick={onResolve}
                        >
                            完成处理
                        </LoadingButton>
                    ) : null}
                </WorkspaceTaskFooter>
                {canClose ? (
                    <details className="group w-full">
                        <summary
                            id="integration-action-other-options"
                            className="inline-flex cursor-pointer list-none items-center gap-1 rounded-sm text-xs text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            其他处理方式
                            <ChevronDownIcon
                                className="size-3 transition-transform group-open:rotate-180"
                                aria-hidden
                            />
                        </summary>
                        <div className="mt-3 flex flex-wrap items-end gap-3 rounded-lg border border-border/60 p-3">
                            <div className="space-y-1">
                                <Label
                                    className="text-xs"
                                    htmlFor="integration-action-replacement-work-item"
                                >
                                    替代任务
                                </Label>
                                <ReplacementWorkItemSearchCombobox
                                    id="integration-action-replacement-work-item"
                                    value={replacementTaskId || null}
                                    onValueChange={(v) =>
                                        onReplacementTaskIdChange(v ?? "")
                                    }
                                    excludeItemId={item.identity.id}
                                    className="w-72"
                                    size="sm"
                                    aria-label="选择替代任务"
                                    placeholder="选择替代任务（任务号 · 业务单）"
                                    allowClear={false}
                                />
                            </div>
                            <Button
                                id="integration-action-close-duplicate"
                                type="button"
                                size="sm"
                                disabled={formalPending || !replacementTaskId}
                                onClick={() =>
                                    onRequestTerminal({
                                        kind: "CLOSE_DUPLICATE",
                                    })
                                }
                            >
                                关闭重复
                            </Button>
                            <Button
                                id="integration-action-close-misrouted"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={formalPending}
                                onClick={() =>
                                    onRequestTerminal({
                                        kind: "CLOSE_MISROUTED",
                                    })
                                }
                            >
                                关闭误派任务
                            </Button>
                        </div>
                    </details>
                ) : null}
            </div>

            {/* Direct reconciliation */}
            {item.identity.itemType === "RECONCILIATION_DIFFERENCE" &&
            !item.hasWorkItem ? (
                <IntegrationDirectReconciliation
                    item={item}
                    can={can}
                    formalPending={formalPending}
                    pendingDirectAction={pendingDirectAction}
                    reconReasonId={reconReasonId}
                    onReconReasonIdChange={onReconReasonIdChange}
                    reasonMismatches={reasonMismatches}
                    onDirectAction={onDirectAction}
                    onRequestTerminal={onRequestTerminal}
                />
            ) : null}
        </div>
    )
    return detailWorkspace ? (
        <DetailWorkspaceSection>{content}</DetailWorkspaceSection>
    ) : (
        content
    )
}
