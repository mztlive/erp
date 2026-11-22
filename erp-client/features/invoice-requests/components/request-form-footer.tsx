"use client"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { formatCurrencyFixed } from "@/lib/fixed-decimal"

import {
    invoiceExceedsAvailable,
    invoiceOverAmount,
    invoiceRemainingAfterRequest,
} from "../line-amounts"

/**
 * 开票申请独立工作面底部吸底核对栏：
 * 对齐进项发票登记 `InvoiceSessionFooter`，常驻展示可申请额度、本次申请合计、剩余额度与阻断原因。
 */
export function InvoiceRequestFormFooter({
    availableAmount,
    linesTotal,
    statusMessage,
    isErrorStatus,
    locked,
    isEditing,
    uncertain,
    saveUncertain,
    versionAdvanced,
    submitPending,
    savePending,
    saveStateFetching,
    canSubmit,
    onCancel,
    onSave,
    onResolveUncertain,
}: {
    availableAmount?: string | null
    linesTotal: string | null
    statusMessage: string
    isErrorStatus: boolean
    locked: boolean
    isEditing: boolean
    uncertain: boolean
    saveUncertain: boolean
    versionAdvanced: boolean
    submitPending: boolean
    savePending: boolean
    saveStateFetching: boolean
    canSubmit: boolean
    onCancel: () => void
    onSave: () => void
    onResolveUncertain: () => void
}) {
    const exceeds = invoiceExceedsAvailable(linesTotal, availableAmount)
    const overAmount = invoiceOverAmount(linesTotal, availableAmount)
    const remaining = invoiceRemainingAfterRequest(linesTotal, availableAmount)

    return (
        <aside
            aria-label="本次开票申请核对"
            className="sticky bottom-0 z-20 mt-auto -mx-(--scaffold-px) border-t border-border/70 bg-card/95 px-(--scaffold-px) py-3 backdrop-blur-xs"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2.5">
                <div className="flex min-w-0 flex-wrap items-center gap-x-5 gap-y-1.5">
                    <div className="flex items-baseline gap-2">
                        <span className="text-xs text-muted-foreground">
                            可申请
                        </span>
                        {availableAmount != null ? (
                            <MoneyValue
                                value={availableAmount}
                                display="exact"
                                className="text-value-compact font-semibold tracking-tight"
                            />
                        ) : (
                            <span className="num text-value-compact font-semibold text-muted-foreground">
                                —
                            </span>
                        )}
                    </div>

                    <div className="flex items-baseline gap-2 border-l border-border/70 pl-5">
                        <span className="text-xs text-muted-foreground">
                            本次申请
                        </span>
                        <span id="invoice-request-amount">
                            {linesTotal ? (
                                exceeds ? (
                                    <span className="num text-value-compact font-semibold tracking-tight text-destructive">
                                        {formatCurrencyFixed(linesTotal, {
                                            maxScale: 2,
                                            minimumFractionDigits: 2,
                                        })}
                                    </span>
                                ) : (
                                    <MoneyValue
                                        value={linesTotal}
                                        display="exact"
                                        className="text-value-compact font-semibold tracking-tight"
                                    />
                                )
                            ) : (
                                <span className="num text-value-compact font-semibold text-muted-foreground">
                                    —
                                </span>
                            )}
                        </span>
                    </div>

                    {availableAmount != null ? (
                        <div className="flex items-baseline gap-2 border-l border-border/70 pl-5">
                            <span className="text-xs text-muted-foreground">
                                {exceeds ? "超出额度" : "提交后剩余"}
                            </span>
                            {exceeds && overAmount ? (
                                <span className="num text-value-compact font-semibold tracking-tight text-destructive">
                                    {formatCurrencyFixed(overAmount, {
                                        maxScale: 2,
                                        minimumFractionDigits: 2,
                                    })}
                                </span>
                            ) : remaining !== null ? (
                                <MoneyValue
                                    value={remaining}
                                    display="exact"
                                    className="text-value-compact font-semibold tracking-tight"
                                />
                            ) : (
                                <span className="num text-value-compact font-semibold text-muted-foreground">
                                    —
                                </span>
                            )}
                        </div>
                    ) : null}

                    {statusMessage ? (
                        <span
                            role="status"
                            className={
                                isErrorStatus || exceeds
                                    ? "border-l border-border/70 pl-5 text-xs text-destructive"
                                    : "border-l border-border/70 pl-5 text-xs text-muted-foreground"
                            }
                        >
                            {statusMessage}
                        </span>
                    ) : null}
                </div>

                <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
                    <Button
                        id="invoice-request-form-cancel"
                        type="button"
                        variant="outline"
                        disabled={locked}
                        onClick={onCancel}
                    >
                        取消
                    </Button>
                    {isEditing && !uncertain ? (
                        <LoadingButton
                            id="invoice-request-save"
                            type="button"
                            variant="outline"
                            loading={savePending}
                            disabled={
                                savePending ||
                                submitPending ||
                                saveStateFetching ||
                                (saveUncertain && versionAdvanced)
                            }
                            onClick={onSave}
                        >
                            {saveUncertain ? "重新保存" : "保存修改"}
                        </LoadingButton>
                    ) : null}
                    {uncertain ? (
                        <LoadingButton
                            id="invoice-request-resolve"
                            type="button"
                            loading={submitPending}
                            disabled={submitPending}
                            onClick={onResolveUncertain}
                        >
                            核对提交结果
                        </LoadingButton>
                    ) : (
                        <LoadingButton
                            id="invoice-request-submit"
                            type="submit"
                            loading={submitPending}
                            disabled={!canSubmit}
                        >
                            提交审批
                        </LoadingButton>
                    )}
                </div>
            </div>
        </aside>
    )
}
