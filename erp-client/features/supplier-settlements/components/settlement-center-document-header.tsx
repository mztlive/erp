"use client"

import { ArrowLeftIcon, RefreshCwIcon, SendIcon } from "lucide-react"

import {
    DocumentHeader,
    GuardedBusinessAction,
    PageHeader,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"

function SettlementCenterDocumentHeader({
    statement,
    immutableFactsAsOf,
    allowed,
    refreshPending,
    submitBlocker,
    onBack,
    onRefresh,
    onSubmitReview,
    embedded = false,
}: {
    statement: SettlementDetailView["statement"]
    immutableFactsAsOf: string
    allowed: Set<string>
    refreshPending: boolean
    submitBlocker?: { message: string }
    onBack: () => void
    onRefresh: () => void
    onSubmitReview: () => void
    embedded?: boolean
}) {
    const st = statement
    return (
        <>
            {embedded ? null : (
                <PageHeader
                    variant="object-chrome"
                    actions={
                        <Button
                            id="supplier-settlements-center-back"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon className="size-4" />
                            返回列表
                        </Button>
                    }
                />
            )}
            <DocumentHeader
                density="compact"
                title={`${st.supplierName} · ${st.periodLabel}`}
                documentNumber={st.statementNo}
                primaryStatus={{ label: st.statusLabel, tone: st.statusTone }}
                version={st.lockVersion}
                meta={
                    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                        <span>
                            经办{" "}
                            <span className="font-medium text-foreground">
                                {st.preparedBy?.displayName ?? "—"}
                            </span>
                        </span>
                        <span className="text-border" aria-hidden="true">
                            ·
                        </span>
                        <span>
                            复核{" "}
                            <span className="font-medium text-foreground">
                                {st.reviewedBy?.displayName ?? "待复核人"}
                            </span>
                        </span>
                        <span className="text-border" aria-hidden="true">
                            ·
                        </span>
                        <span className="text-muted-foreground">
                            记录 {formatDateTime(immutableFactsAsOf, "default")}
                        </span>
                    </span>
                }
                primaryAction={
                    <div className="flex flex-wrap gap-2">
                        {allowed.has("REFRESH_TRIAL") ? (
                            <Button
                                id="supplier-settlements-center-refresh"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={refreshPending}
                                onClick={() => void onRefresh()}
                            >
                                <RefreshCwIcon className="size-3.5" />
                                刷新试算
                            </Button>
                        ) : null}
                        {allowed.has("SUBMIT_REVIEW") ? (
                            <Button
                                id="supplier-settlements-center-submit-review"
                                type="button"
                                size="sm"
                                onClick={onSubmitReview}
                            >
                                <SendIcon className="size-3.5" />
                                提交复核
                            </Button>
                        ) : submitBlocker ? (
                            <GuardedBusinessAction
                                id="supplier-settlements-center-submit-review-disabled"
                                type="button"
                                size="sm"
                                disabled
                                reason={submitBlocker.message}
                            >
                                提交复核
                            </GuardedBusinessAction>
                        ) : null}
                    </div>
                }
            />
        </>
    )
}

export { SettlementCenterDocumentHeader }
