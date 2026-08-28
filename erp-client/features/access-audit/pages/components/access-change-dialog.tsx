"use client"

import { ShieldAlertIcon } from "lucide-react"

import {
    BatchImpactPreview,
    BusinessDiffPanel,
    OptionCombobox,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { riskLabel } from "@/features/access-audit/lib/risk-labels"
import type { useAccessChangeFlow } from "@/features/access-audit/pages/hooks/use-access-change-flow"
import type {
    AccessChangeCommand,
    AccessChangeOutcome,
    AccessImpactPreview,
} from "@/features/access-audit/types"

/** 影响预览内嵌表单实例类型：与变更流程 hook 中的 useAppForm 结果保持一致。 */
export type ChangeReasonFormApi = ReturnType<
    typeof useAccessChangeFlow
>["form"]

type AccessChangeDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    impact: AccessImpactPreview | null
    pendingCommand: AccessChangeCommand | null
    isSubmitting: boolean
    form: ChangeReasonFormApi
    onConfirm: () => Promise<void>
    onApplyOutcome: (outcome: AccessChangeOutcome) => void
}

function AccessChangeDialog({
    open,
    onOpenChange,
    impact,
    pendingCommand,
    isSubmitting,
    form,
    onConfirm,
    onApplyOutcome,
}: AccessChangeDialogProps) {
    return (
        <Dialog
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen && isSubmitting) return
                onOpenChange(nextOpen)
            }}
        >
            <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
                <DialogHeader>
                    <DialogTitle>授权变更影响预览</DialogTitle>
                    <DialogDescription>
                        提交前先查看变更预览与受影响人员；若数据已被他人更新，需确认后重新提交。
                    </DialogDescription>
                </DialogHeader>

                {impact ? (
                    <div className="flex flex-col gap-4">
                        <BatchImpactPreview
                            title={impact.actionLabel}
                            description={impact.changeSummary}
                            filterSummary={`主体：${impact.subjectLabel}`}
                            selectionScope={
                                impact.affectedWorkSurfaceSummary
                            }
                            estimated={impact.affectedSubjectCount}
                            processable={
                                impact.submissionBlocker
                                    ? 0
                                    : impact.affectedSubjectCount
                            }
                            skipped={
                                impact.submissionBlocker
                                    ? impact.affectedSubjectCount
                                    : 0
                            }
                            background={false}
                            sensitiveFields={["密钥", "卡密", "完整银行账号"]}
                            skippedReason={impact.submissionBlocker?.message}
                        />

                        <Alert
                            variant={
                                impact.riskLevel === "high"
                                    ? "warning"
                                    : impact.riskLevel === "medium"
                                      ? "info"
                                      : "default"
                            }
                        >
                            <ShieldAlertIcon aria-hidden="true" />
                            <AlertTitle>
                                风险{" "}
                                {impact.riskLevel === "high"
                                    ? "高"
                                    : impact.riskLevel === "medium"
                                      ? "中"
                                      : "低"}
                                {impact.riskFlags.length
                                    ? ` · ${impact.riskFlags.map(riskLabel).join("、")}`
                                    : ""}
                            </AlertTitle>
                            <AlertDescription>
                                {impact.riskSummary}
                            </AlertDescription>
                        </Alert>

                        {impact.submissionBlocker ? (
                            <Alert variant="destructive">
                                <AlertTitle>
                                    {impact.submissionBlocker.code}
                                </AlertTitle>
                                <AlertDescription>
                                    {impact.submissionBlocker.message}
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        <BusinessDiffPanel
                            title="配置差异"
                            changes={impact.diffs.map((d) => ({
                                id: d.id,
                                field: d.field,
                                before: d.before,
                                after: d.after,
                                note: d.note,
                            }))}
                        />

                        {!impact.submissionBlocker ? (
                            <form
                                className="space-y-3"
                                onSubmit={async (e) => {
                                    e.preventDefault()
                                    // 校验通过后才执行提交：说明超长等校验失败时不再绕过
                                    await form.handleSubmit()
                                    if (form.state.isFieldsValid) {
                                        await onConfirm()
                                    }
                                }}
                            >
                                <div className="space-y-1.5">
                                    <Label htmlFor="w19-reason">
                                        变更原因
                                    </Label>
                                    <form.AppField
                                        name="reasonCode"
                                        children={(field) => (
                                            <OptionCombobox
                                                id="w19-reason"
                                                value={field.state.value}
                                                onValueChange={(v) =>
                                                    field.handleChange(
                                                        v ??
                                                            field.state.value,
                                                    )
                                                }
                                                options={[
                                                    {
                                                        value: "SECURITY_OPS",
                                                        label: "安全运维",
                                                    },
                                                    {
                                                        value: "EMERGENCY_STOP_LOSS",
                                                        label: "紧急止损",
                                                    },
                                                    {
                                                        value: "ORG_CHANGE",
                                                        label: "组织调整",
                                                    },
                                                ]}
                                                className="w-full"
                                                allowClear={false}
                                                aria-label="变更原因"
                                                placeholder="变更原因"
                                            />
                                        )}
                                    />
                                </div>
                                <div className="space-y-1.5">
                                    <Label htmlFor="w19-comment">
                                        说明（可选，勿填密钥）
                                    </Label>
                                    <form.AppField
                                        name="comment"
                                        children={(field) => (
                                            <Textarea
                                                id="w19-comment"
                                                value={field.state.value ?? ""}
                                                onChange={(e) =>
                                                    field.handleChange(
                                                        e.target.value,
                                                    )
                                                }
                                                rows={2}
                                                placeholder="不包含密钥或敏感业务正文"
                                            />
                                        )}
                                    />
                                </div>
                                <p className="text-xs text-muted-foreground">
                                    提交前系统会按最新配置核对版本；若配置已被他人更新，将提示你重新确认。
                                </p>
                                <DialogFooter>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        disabled={isSubmitting}
                                        onClick={() => onOpenChange(false)}
                                    >
                                        取消
                                    </Button>
                                    <Button
                                        type="submit"
                                        disabled={isSubmitting}
                                        variant={
                                            pendingCommand?.action ===
                                            "EMERGENCY_REVOKE_USER_ROLE"
                                                ? "destructive"
                                                : "default"
                                        }
                                    >
                                        {isSubmitting ? "提交中…" : "确认提交"}
                                    </Button>
                                </DialogFooter>
                            </form>
                        ) : (
                            <DialogFooter>
                                <Button
                                    type="button"
                                    variant="ghost"
                                    onClick={() => {
                                        onApplyOutcome({
                                            outcome: "REJECTED",
                                            code: impact.submissionBlocker!
                                                .code,
                                            message:
                                                impact.submissionBlocker!
                                                    .message,
                                            actionBlockers: [
                                                impact.submissionBlocker!,
                                            ],
                                        })
                                        onOpenChange(false)
                                    }}
                                >
                                    关闭并记录阻断
                                </Button>
                            </DialogFooter>
                        )}
                    </div>
                ) : (
                    <div className="h-24 animate-pulse rounded-lg bg-muted" />
                )}
            </DialogContent>
        </Dialog>
    )
}

export { AccessChangeDialog }
export type { AccessChangeDialogProps }
