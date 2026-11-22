"use client"

import type { ReactNode } from "react"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { RefreshCwIcon, SendIcon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    detailWorkspaceStyles,
    GuardedBusinessAction,
    type DetailWorkspaceBackLink,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"

function SettlementCenterDocumentHeader({
    statement,
    immutableFactsAsOf,
    allowed,
    refreshPending,
    submitBlocker,
    onBack,
    backLabel = "返回对账单列表",
    backVisibility = "standalone-only",
    onRefresh,
    onSubmitReview,
    embedded = false,
    tabs,
}: {
    statement: SettlementDetailView["statement"]
    immutableFactsAsOf: string
    allowed: Set<string>
    refreshPending: boolean
    submitBlocker?: { message: string }
    onBack: () => void
    backLabel?: string
    backVisibility?: DetailWorkspaceBackLink["visibility"]
    onRefresh: () => void
    onSubmitReview: () => void
    embedded?: boolean
    tabs: ReactNode
}) {
    const st = statement
    const billVersion = st.externalBillVersion
        ? String(st.externalBillVersion).replace(/^v/i, "")
        : ""

    return (
        <DetailWorkspaceChrome
            tabs={tabs}
            back={
                embedded
                    ? undefined
                    : {
                          id: "supplier-settlements-center-back",
                          label: backLabel,
                          visibility: backVisibility,
                          onClick: onBack,
                      }
            }
            title={`${st.supplierName} · ${st.periodLabel}`}
            documentNumber={st.statementNo}
            primaryStatus={{ label: st.statusLabel, tone: st.statusTone }}
            version={st.lockVersion}
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="prepared">
                            经办{" "}
                            <span className="font-medium text-foreground">
                                {st.preparedBy?.displayName ?? "—"}
                            </span>
                        </span>,
                        <span key="reviewed">
                            复核{" "}
                            <span className="font-medium text-foreground">
                                {st.reviewedBy?.displayName ?? "待复核人"}
                            </span>
                        </span>,
                        <span key="facts-as-of" className="num">
                            记录 {formatDateTime(immutableFactsAsOf, "default")}
                        </span>,
                        st.externalBillNo ? (
                            <span key="bill" className="num">
                                账单 {st.externalBillNo}
                                {billVersion ? `（第 ${billVersion} 版）` : ""}
                            </span>
                        ) : null,
                    ]}
                />
            }
            secondaryActions={
                <PermissionGuard permission="supplier_settlement_statement:update">
                    {allowed.has("REFRESH_TRIAL") ? (
                        <LoadingButton
                            id="supplier-settlements-center-refresh"
                            loading={refreshPending}
                            type="button"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            size="sm"
                            disabled={refreshPending}
                            onClick={() => void onRefresh()}
                        >
                            {!refreshPending ? (
                                <RefreshCwIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {refreshPending ? "刷新中…" : "刷新试算"}
                        </LoadingButton>
                    ) : null}
                </PermissionGuard>
            }
            primaryAction={
                <PermissionGuard permission="supplier_settlement_statement:submit">
                    {allowed.has("SUBMIT_REVIEW") ? (
                        <Button
                            id="supplier-settlements-center-submit-review"
                            type="button"
                            size="sm"
                            onClick={onSubmitReview}
                        >
                            <SendIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
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
                    ) : undefined}
                </PermissionGuard>
            }
        />
    )
}

export { SettlementCenterDocumentHeader }
