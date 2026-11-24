import { useEffect, useRef } from "react"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import type { ReviewDecision } from "../lib/review-draft"

export function PortalReviewFooter({
    idPrefix,
    disabled,
    conflict,
    issueCount,
    skuCount,
    pending,
    retryDecision,
    canRecover,
    error,
    returnOpen,
    returnReason,
    returnReasonError,
    returnReasonValid,
    quickPhrases,
    onReturnOpenChange,
    onReturnReasonChange,
    onApprove,
    onReturn,
    onRecover,
    onLocateIssue,
}: {
    idPrefix: string
    disabled: boolean
    conflict: boolean
    issueCount: number
    skuCount?: number
    pending: boolean
    retryDecision: ReviewDecision | null
    canRecover: boolean
    error: unknown
    returnOpen: boolean
    returnReason: string
    returnReasonError?: string
    returnReasonValid: boolean
    quickPhrases: readonly string[]
    onReturnOpenChange: (open: boolean) => void
    onReturnReasonChange: (value: string) => void
    onApprove: () => void
    onReturn: () => void
    onRecover: () => void
    onLocateIssue?: () => void
}) {
    const reasonRef = useRef<HTMLTextAreaElement>(null)
    useEffect(() => {
        if (returnOpen) reasonRef.current?.focus()
    }, [returnOpen])

    const status = pending
        ? "正在提交"
        : retryDecision
          ? "待核对提交结果"
          : conflict
            ? "待核对最新资料"
            : returnOpen
              ? "退回供应商修改"
              : issueCount
                ? "待完善"
                : "可确认通过"

    const summary = (
        <dl
            aria-live="polite"
            className="flex min-w-0 flex-wrap gap-x-4 gap-y-1 text-xs"
        >
            <div className="flex gap-1.5">
                <dt className="text-muted-foreground">审核结论</dt>
                <dd className="font-medium">{status}</dd>
            </div>
            {skuCount !== undefined ? (
                <div className="flex gap-1.5">
                    <dt className="text-muted-foreground">提报规格</dt>
                    <dd className="num font-medium">{skuCount} 项</dd>
                </div>
            ) : null}
        </dl>
    )

    const actions = (
        <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
            {retryDecision ? (
                <>
                    {!pending ? (
                        <p
                            role="status"
                            className="basis-full border-l-2 border-warning-border bg-warning-soft/45 px-3 py-2 text-xs text-warning-soft-foreground"
                        >
                            {retryDecision === "return" ? "退回" : "通过"}
                            结果待核对。核对将沿用原决定及提交内容。
                            {error ? (
                                <span className="block wrap-anywhere">
                                    {getErrorMessage(
                                        error,
                                        "暂未取得提交结果。",
                                    )}
                                </span>
                            ) : null}
                        </p>
                    ) : null}
                    <Button
                        id={`${idPrefix}-check-result`}
                        type="button"
                        size="sm"
                        variant="default"
                        disabled={pending || !canRecover}
                        onClick={onRecover}
                    >
                        {pending ? "正在提交…" : "核对提交结果"}
                    </Button>
                </>
            ) : (
                <>
                    {returnOpen ? (
                        <ReasonTextareaField
                            id="supplier-portal-review-return-reason"
                            textareaRef={reasonRef}
                            mode="utf8-bytes"
                            value={returnReason}
                            error={returnReasonError}
                            onChange={onReturnReasonChange}
                            label="退回原因（供应商可见）"
                            required
                            rows={2}
                            quickPhrases={quickPhrases}
                            disabled={disabled}
                            onCtrlEnterSubmit={onReturn}
                            className="min-w-0 basis-full"
                        />
                    ) : null}
                    {!returnOpen && issueCount > 0 && onLocateIssue ? (
                        <Button
                            id={`${idPrefix}-footer-locate`}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-warning-soft-foreground"
                            disabled={pending}
                            onClick={onLocateIssue}
                        >
                            <span className="num">{issueCount}</span> 项待完善 ·
                            定位
                        </Button>
                    ) : null}
                    <Button
                        id={`${idPrefix}-return`}
                        type="button"
                        size="sm"
                        variant={returnOpen ? "ghost" : "outline"}
                        className={
                            returnOpen
                                ? undefined
                                : "border-destructive-border text-destructive-soft-foreground hover:bg-destructive-soft"
                        }
                        disabled={disabled}
                        aria-expanded={returnOpen}
                        onClick={() => onReturnOpenChange(!returnOpen)}
                    >
                        {returnOpen ? "取消退回" : "退回供应商修改"}
                    </Button>
                    <Button
                        id={`${idPrefix}-submit`}
                        type="button"
                        size="sm"
                        variant={returnOpen ? "destructive" : "default"}
                        disabled={
                            disabled || (returnOpen && !returnReasonValid)
                        }
                        onClick={returnOpen ? onReturn : onApprove}
                    >
                        {returnOpen ? "确认退回修改" : "确认通过并生效"}
                    </Button>
                </>
            )}
        </div>
    )
    return (
        <WorkspaceTaskFooter
            summary={summary}
            fallback={
                <footer className="sticky bottom-0 z-10 flex min-h-12 min-w-0 flex-wrap items-center gap-x-4 gap-y-2 border-t border-border/60 bg-card py-2 shadow-footer">
                    {summary}
                    {actions}
                </footer>
            }
        >
            {actions}
        </WorkspaceTaskFooter>
    )
}
