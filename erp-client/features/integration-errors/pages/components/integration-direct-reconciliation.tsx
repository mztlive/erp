import { OptionCombobox } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"

import type { TerminalConfirm } from "../../components/terminal-action-dialog"
import type {
    IntegrationActionKind,
    IntegrationResolutionItemView,
} from "../../types"
import type { IntegrationTaskActionKind } from "../hooks/use-integration-actions"

export function IntegrationDirectReconciliation({
    item,
    can,
    formalPending,
    pendingDirectAction,
    reconReasonId,
    onReconReasonIdChange,
    reasonMismatches,
    onDirectAction,
    onRequestTerminal,
}: {
    item: IntegrationResolutionItemView
    can: (action: IntegrationActionKind) => boolean
    formalPending: boolean
    pendingDirectAction?: IntegrationTaskActionKind
    reconReasonId: string
    onReconReasonIdChange: (value: string) => void
    reasonMismatches: (
        conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE",
    ) => boolean
    onDirectAction: (kind: IntegrationTaskActionKind) => void
    onRequestTerminal: (confirm: TerminalConfirm) => void
}) {
    return (
        <div className="space-y-3 rounded-xl border border-dashed p-3">
            <p className="text-body-compact font-medium">登记对账结论</p>
            <p className="text-xs text-muted-foreground">
                请选择差异原因并核对关联凭证后，确认该笔对账结果。
            </p>
            {item.reconciliationReasonRegistry ? (
                <>
                    <OptionCombobox
                        id="integration-direct-recon-reason"
                        value={reconReasonId || null}
                        onValueChange={(v) => onReconReasonIdChange(v ?? "")}
                        options={item.reconciliationReasonRegistry.registeredReasons.map(
                            (r) => ({
                                value: r.registeredReasonId,
                                label: r.label,
                            }),
                        )}
                        className="w-full max-w-md"
                        size="sm"
                        aria-label="差异原因"
                        placeholder="请选择差异原因"
                        allowClear={false}
                    />
                    <div className="flex flex-wrap gap-2">
                        {can("CONFIRM_NO_ERROR") ? (
                            <Button
                                id="integration-direct-confirm-no-error"
                                type="button"
                                size="sm"
                                disabled={
                                    formalPending ||
                                    reasonMismatches("CONFIRM_NO_ERROR")
                                }
                                onClick={() =>
                                    onRequestTerminal({
                                        kind: "CONFIRM_NO_ERROR",
                                    })
                                }
                            >
                                确认无误
                            </Button>
                        ) : null}
                        {can("CONFIRM_VALID_DIFFERENCE") ? (
                            <Button
                                id="integration-direct-confirm-valid-difference"
                                type="button"
                                size="sm"
                                variant="secondary"
                                disabled={
                                    formalPending ||
                                    reasonMismatches("CONFIRM_VALID_DIFFERENCE")
                                }
                                onClick={() =>
                                    onRequestTerminal({
                                        kind: "CONFIRM_VALID_DIFFERENCE",
                                    })
                                }
                            >
                                确认有效差异
                            </Button>
                        ) : null}
                    </div>
                </>
            ) : (
                <Alert variant="warning">
                    <AlertTitle>暂未配置差异原因选项</AlertTitle>
                    <AlertDescription>
                        请联系管理员配置对账差异原因选项后再确认结果；当前仅可查询或补充凭证。
                    </AlertDescription>
                </Alert>
            )}
            <div className="flex flex-wrap gap-2">
                {can("QUERY_ORIGINAL_RESULT") ? (
                    <LoadingButton
                        id="integration-direct-query-original-result"
                        loading={
                            pendingDirectAction === "QUERY_ORIGINAL_RESULT"
                        }
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={formalPending}
                        onClick={() =>
                            void onDirectAction("QUERY_ORIGINAL_RESULT")
                        }
                    >
                        查询原结果
                    </LoadingButton>
                ) : null}
                {can("REPLAY_ORIGINAL") ? (
                    <LoadingButton
                        id="integration-direct-replay-original"
                        loading={pendingDirectAction === "REPLAY_ORIGINAL"}
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={formalPending}
                        onClick={() => void onDirectAction("REPLAY_ORIGINAL")}
                    >
                        重新提交
                    </LoadingButton>
                ) : null}
                {can("REATTRIBUTE") ? (
                    <LoadingButton
                        id="integration-direct-reatribute"
                        loading={pendingDirectAction === "REATTRIBUTE"}
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={formalPending}
                        onClick={() => void onDirectAction("REATTRIBUTE")}
                    >
                        重新匹配归属
                    </LoadingButton>
                ) : null}
                {can("LINK_COMPENSATION") ? (
                    <LoadingButton
                        id="integration-direct-link-compensation"
                        loading={pendingDirectAction === "LINK_COMPENSATION"}
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                            formalPending || item.linkedEvidence.length === 0
                        }
                        onClick={() => void onDirectAction("LINK_COMPENSATION")}
                    >
                        关联补救单据
                    </LoadingButton>
                ) : null}
                {can("ADD_EVIDENCE") ? (
                    <LoadingButton
                        id="integration-direct-add-evidence"
                        loading={pendingDirectAction === "ADD_EVIDENCE"}
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                            formalPending || item.linkedEvidence.length === 0
                        }
                        onClick={() => void onDirectAction("ADD_EVIDENCE")}
                    >
                        补充凭证（暂不完成对账）
                    </LoadingButton>
                ) : null}
            </div>
        </div>
    )
}
