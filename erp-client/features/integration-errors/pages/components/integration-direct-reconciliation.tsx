import { OptionCombobox } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

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
    reconReasonId,
    onReconReasonIdChange,
    reasonMismatches,
    onDirectAction,
    onRequestTerminal,
}: {
    item: IntegrationResolutionItemView
    can: (action: IntegrationActionKind) => boolean
    formalPending: boolean
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
            <p className="text-sm font-medium">直接对账（无关联任务）</p>
            <p className="text-xs text-muted-foreground">
                处理完成只能「确认无误 /
                确认有效差异」，引用原因注册表与受控证据；不得虚构任务关闭。
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
                        aria-label="注册原因"
                        placeholder="选择注册原因"
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
                    <AlertTitle>原因注册表未配置</AlertTitle>
                    <AlertDescription>
                        确认无误/有效差异均禁用；仅展示服务端当前开放的非终结动作。
                    </AlertDescription>
                </Alert>
            )}
            <div className="flex flex-wrap gap-2">
                {can("QUERY_ORIGINAL_RESULT") ? (
                    <Button
                        id="integration-direct-query-original-result"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={formalPending}
                        onClick={() =>
                            void onDirectAction("QUERY_ORIGINAL_RESULT")
                        }
                    >
                        查询原结果
                    </Button>
                ) : null}
                {can("REPLAY_ORIGINAL") ? (
                    <Button
                        id="integration-direct-replay-original"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={formalPending}
                        onClick={() => void onDirectAction("REPLAY_ORIGINAL")}
                    >
                        重新提交
                    </Button>
                ) : null}
                {can("REATTRIBUTE") ? (
                    <Button
                        id="integration-direct-reatribute"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={formalPending}
                        onClick={() => void onDirectAction("REATTRIBUTE")}
                    >
                        重新归集
                    </Button>
                ) : null}
                {can("LINK_COMPENSATION") ? (
                    <Button
                        id="integration-direct-link-compensation"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                            formalPending || item.linkedEvidence.length === 0
                        }
                        onClick={() => void onDirectAction("LINK_COMPENSATION")}
                    >
                        关联补偿
                    </Button>
                ) : null}
                {can("ADD_EVIDENCE") ? (
                    <Button
                        id="integration-direct-add-evidence"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={
                            formalPending || item.linkedEvidence.length === 0
                        }
                        onClick={() => void onDirectAction("ADD_EVIDENCE")}
                    >
                        补充证据（暂不完成对账）
                    </Button>
                ) : null}
            </div>
        </div>
    )
}
