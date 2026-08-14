"use client"

import {
    DocumentSection,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    emptyLineResult,
    formatOccurredAt,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    FACT_ONLY_NOTICE,
    OVERALL_RESULT_LABEL,
} from "@/features/sales-orders/lib/acceptance-types"
import type { AcceptanceFormApi } from "@/features/sales-orders/hooks/use-acceptance-form"
import type { AcceptanceSelectionApi } from "@/features/sales-orders/hooks/use-acceptance-selection"
import { AcceptanceLineResultEditor } from "@/features/sales-orders/components/acceptance-line-result-editor"

export function AcceptanceEntryForm({
    form,
    salesOrderNo,
    customerLabel,
    selection,
    canPost,
    clientIssues,
    postBlockerMessage,
    draftSavedAt,
    draftVersion,
}: {
    form: AcceptanceFormApi
    salesOrderNo: string
    customerLabel: string
    selection: AcceptanceSelectionApi
    canPost: boolean
    clientIssues: ValidationIssue[]
    postBlockerMessage?: string
    draftSavedAt: string | null
    draftVersion?: number
}) {
    return (
        <DocumentSection
            className="py-0"
            title="本次验收"
            description={
                <>
                    销售单 {salesOrderNo} · {customerLabel}
                    <span className="ms-2 text-2xs uppercase tracking-wide opacity-70">
                        当前销售数据
                    </span>
                </>
            }
        >
            <div className="space-y-4">
                <form
                    id="acceptance-form"
                    className="space-y-4"
                    onSubmit={(e) => {
                        e.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="acceptedAt">
                        {(field) => (
                            <field.DateTimeField
                                label="客户验收时间"
                                disabled={!canPost}
                            />
                        )}
                    </form.AppField>

                    <div className="rounded-lg border border-border bg-muted/30 px-3 py-2 text-sm">
                        <div className="text-xs text-muted-foreground">
                            总体结果（由明细约束）
                        </div>
                        <div className="mt-1 font-medium">
                            {OVERALL_RESULT_LABEL[selection.overallPreview]}
                            {selection.selected.size > 0
                                ? ` · 已选 ${selection.selected.size} 个履约批次`
                                : " · 尚未选择来源"}
                        </div>
                    </div>

                    {[...selection.selectedLines.entries()].map(
                        ([lineId, facts]) => {
                            const result =
                                selection.lineResults.get(lineId) ??
                                emptyLineResult()
                            const unit = facts[0]?.unitCode ?? ""
                            const hasService = facts.some(
                                (f) => f.fulfillmentFactType === "SERVICE",
                            )
                            return (
                                <AcceptanceLineResultEditor
                                    key={lineId}
                                    lineId={lineId}
                                    facts={facts}
                                    result={result}
                                    unit={unit}
                                    hasService={hasService}
                                    canPost={canPost}
                                    selected={selection.selected}
                                    onUpdate={selection.updateLineResult}
                                />
                            )
                        },
                    )}

                    {selection.hasExceptionResult ? (
                        <Alert variant="warning" role="status">
                            <AlertTitle>
                                {OVERALL_RESULT_LABEL[selection.overallPreview]}
                                ：仅记录验收记录
                            </AlertTitle>
                            <AlertDescription>
                                {FACT_ONLY_NOTICE}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    <form.AppField name="comment">
                        {(field) => (
                            <field.TextareaField
                                label="内部备注"
                                placeholder="不写联系人手机号等无关敏感信息"
                                rows={2}
                                disabled={!canPost}
                            />
                        )}
                    </form.AppField>
                </form>

                {clientIssues.length > 0 ? (
                    <ValidationSummary
                        issues={clientIssues}
                        title={`提交前请处理 ${clientIssues.length} 项`}
                    />
                ) : null}

                {postBlockerMessage ? (
                    <p className="text-sm text-destructive" role="alert">
                        {postBlockerMessage}
                    </p>
                ) : null}

                {draftSavedAt ? (
                    <p
                        className="text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        草稿已保存 · {formatOccurredAt(draftSavedAt)}
                        {draftVersion != null ? ` · v${draftVersion}` : null}
                    </p>
                ) : null}
            </div>
        </DocumentSection>
    )
}
