import * as React from "react"
import { surfacePanelClassName } from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
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
    actionZoneRef,
}: {
    item: IntegrationResolutionItemView
    can: (action: IntegrationActionKind) => boolean
    formalPending: boolean
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
    actionZoneRef: React.Ref<HTMLDivElement>
}) {
    const assignedDisabled =
        responsibilityStatus !== "assigned_to_me" || formalPending
    return (
        <Card size="sm" className={surfacePanelClassName} ref={actionZoneRef}>
            <CardHeader className="border-b border-grid">
                <CardTitle>处理动作</CardTitle>
                <CardDescription>
                    仅展示可操作范围；阻断原因见下方说明
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {item.actionBlockers.length > 0 ? (
                    <ul className="space-y-1 text-xs text-muted-foreground">
                        {item.actionBlockers.map((b) => (
                            <li key={`${b.action}-${b.code}`}>
                                <span className="font-medium text-foreground">
                                    {INTEGRATION_ACTION_LABEL[b.action] ??
                                        b.action}
                                </span>
                                ：{b.message}
                            </li>
                        ))}
                    </ul>
                ) : null}

                <div className="space-y-1">
                    <Label htmlFor="integration-action-comment">处理说明</Label>
                    <Textarea
                        id="integration-action-comment"
                        rows={2}
                        value={comment}
                        onChange={(e) => onCommentChange(e.target.value)}
                        placeholder="可选说明（不覆盖业务证据）"
                    />
                </div>

                <div className="flex flex-wrap gap-2">
                    {can("QUERY_ORIGINAL_RESULT") && item.workItem ? (
                        <Button
                            id="integration-action-query-original-result"
                            type="button"
                            disabled={assignedDisabled}
                            onClick={() =>
                                void onTaskAction("QUERY_ORIGINAL_RESULT")
                            }
                        >
                            查询原结果
                        </Button>
                    ) : null}
                    {can("REPLAY_ORIGINAL") && item.workItem ? (
                        <Button
                            id="integration-action-replay-original"
                            type="button"
                            variant="secondary"
                            disabled={assignedDisabled}
                            onClick={() => void onTaskAction("REPLAY_ORIGINAL")}
                        >
                            重新提交
                        </Button>
                    ) : null}
                    {can("ADD_EVIDENCE") && item.workItem ? (
                        <Button
                            id="integration-action-add-evidence"
                            type="button"
                            variant="outline"
                            disabled={assignedDisabled}
                            onClick={() => void onTaskAction("ADD_EVIDENCE")}
                        >
                            补充证据
                        </Button>
                    ) : null}
                    {can("LINK_COMPENSATION") && item.workItem ? (
                        <Button
                            id="integration-action-link-compensation"
                            type="button"
                            variant="outline"
                            disabled={assignedDisabled}
                            onClick={() =>
                                void onTaskAction("LINK_COMPENSATION")
                            }
                        >
                            关联补偿
                        </Button>
                    ) : null}
                    {can("REATTRIBUTE") && item.workItem ? (
                        <Button
                            id="integration-action-reatribute"
                            type="button"
                            variant="outline"
                            disabled={assignedDisabled}
                            onClick={() => void onTaskAction("REATTRIBUTE")}
                        >
                            重新归集
                        </Button>
                    ) : null}
                    {can("RESOLVE") &&
                    item.workItem &&
                    item.resolutionEvidencePolicy ? (
                        <Button
                            id="integration-action-resolve"
                            type="button"
                            disabled={assignedDisabled}
                            onClick={() =>
                                onRequestTerminal({ kind: "RESOLVE" })
                            }
                        >
                            标记已解决
                        </Button>
                    ) : null}
                    {item.workItem?.allowedActions.includes("CLOSE") ? (
                        <div className="flex w-full flex-wrap items-end gap-2 rounded-lg border p-2">
                            <div className="space-y-1">
                                <Label className="text-xs">替代任务</Label>
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
                                关闭误派
                            </Button>
                        </div>
                    ) : null}
                </div>

                {/* Direct reconciliation */}
                {item.identity.itemType === "RECONCILIATION_DIFFERENCE" &&
                !item.hasWorkItem ? (
                    <IntegrationDirectReconciliation
                        item={item}
                        can={can}
                        formalPending={formalPending}
                        reconReasonId={reconReasonId}
                        onReconReasonIdChange={onReconReasonIdChange}
                        reasonMismatches={reasonMismatches}
                        onDirectAction={onDirectAction}
                        onRequestTerminal={onRequestTerminal}
                    />
                ) : null}
            </CardContent>
        </Card>
    )
}
