"use client"

import * as React from "react"

import { revalidateLogic, useSelector } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"

import {
    DiscardConfirmDialog,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { useAppForm } from "@/components/form"
import { toast } from "@/components/ui/toast"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { getErrorMessage } from "@/lib/api/errors"
import type { FormalCommandKeyLedger } from "@/lib/formal-command"
import { ContractUploadDialog } from "@/features/contracts/contract-upload-dialog"
import { useContractCenterQuery } from "@/features/contracts/queries"
import type { UploadContractPdfResult } from "@/features/contracts/types"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { entitySelectorKeys } from "@/features/entity-selectors"
import type { SalesOrderDraftResumeData } from "@/features/sales-orders/api/sales-orders"
import {
    createEmptyLine,
    validateSalesOrderForm,
} from "@/features/sales-orders/lib/sales-order-create-model"
import { buildSalesOrderSubmitSnapshot } from "@/features/sales-orders/components/sales-order-submit-confirm-summary"
import type { SalesOrderNature } from "@/features/sales-orders/types"
import { useSalesOrderCreateCommandLedger } from "@/features/sales-orders/hooks/use-sales-order-create-command-ledger"
import { useSalesOrderCreateDefaults } from "@/features/sales-orders/hooks/use-sales-order-create-defaults"
import { useSalesOrderCreateUnloadGuard } from "@/features/sales-orders/hooks/use-sales-order-create-unload-guard"
import { useSalesOrderCreateSubmission } from "@/features/sales-orders/hooks/use-sales-order-create-submission"
import { useSalesLineProcurementResponsibilities } from "@/features/sales-orders/hooks/use-sales-line-procurement-responsibilities"
import { SalesOrderCreateAlerts } from "@/features/sales-orders/components/sales-order-create-alerts"
import { SalesOrderCreateContractSection } from "@/features/sales-orders/components/sales-order-create-contract-section"
import { SalesOrderCreateHeaderFields } from "@/features/sales-orders/components/sales-order-create-header-fields"
import { SalesOrderCreateLineItemsSection } from "@/features/sales-orders/components/sales-order-create-line-items-section"
import { SalesOrderCreateTotalBar } from "@/features/sales-orders/components/sales-order-create-total-bar"
import { SalesOrderCreateSummaryPanel } from "@/features/sales-orders/components/sales-order-create-summary-panel"
import { SalesOrderCreateResubmitDialog } from "@/features/sales-orders/components/sales-order-create-resubmit-dialog"
import { SalesOrderApprovalArea } from "@/features/sales-orders/components/sales-order-approval-area"
import { SalesOrderSubmitConfirmDialog } from "@/features/sales-orders/components/sales-order-submit-confirm-dialog"
import { VoucherSalesOrderApprovalArea } from "@/features/sales-orders/components/voucher-sales-order-approval-area"
import { VoucherSalesOrderSubmitConfirmDialog } from "@/features/sales-orders/components/voucher-sales-order-submit-confirm-dialog"

import type {
    SalesOrderEditorPurpose,
    SalesOrderEditorResult,
} from "@/features/sales-orders/lib/sales-order-create-form-types"

export type {
    SalesOrderEditorPurpose,
    SalesOrderEditorResult,
} from "@/features/sales-orders/lib/sales-order-create-form-types"

export function SalesOrderCreateForm({
    initialCustomerId = "",
    initialContractId = "",
    initialContractRevisionId = "",
    initialNature = "physical_service",
    initialDraft = null,
    purpose = "create",
    chrome = "page",
    onResult,
    onSubmitted,
    commandLedger: commandLedgerProp,
}: {
    initialCustomerId?: string
    initialContractId?: string
    initialContractRevisionId?: string
    initialNature?: SalesOrderNature
    /** 继续编辑 / 驳回改单：已有可编辑内容；新建时为 `null`。 */
    initialDraft?: SalesOrderDraftResumeData | null
    purpose?: SalesOrderEditorPurpose
    /** page：独立建单页；none：嵌在对象中心内，外壳由详情页提供。 */
    chrome?: "page" | "none"
    onResult?: (result: SalesOrderEditorResult) => void
    onSubmitted?: (salesOrderId: string) => void
    commandLedger?: FormalCommandKeyLedger
}) {
    const queryClient = useQueryClient()
    const profileQuery = useAccountProfileQuery()
    const [selectedContractId, setSelectedContractId] = React.useState(
        initialDraft?.contractId || initialContractId,
    )
    const [uploadOpen, setUploadOpen] = React.useState(false)
    const preferredRevisionRef = React.useRef(initialContractRevisionId)
    /** 继续编辑场景下，合同派生 effect 首次运行时不要覆盖已从草稿带回的付款条件。 */
    const skipPaymentTermsResetRef = React.useRef(initialDraft != null)
    const contractQuery = useContractCenterQuery(selectedContractId)

    const commandLedger = useSalesOrderCreateCommandLedger(
        initialDraft?.salesOrderId ?? "new-sales-order",
        commandLedgerProp,
    )
    const submission = useSalesOrderCreateSubmission({
        initialDraft,
        purpose,
        commandLedger,
        onResult,
        onSubmitted,
    })
    const { setDraftSaved } = submission
    const defaultValues = useSalesOrderCreateDefaults({
        initialCustomerId,
        initialContractId,
        initialContractRevisionId,
        initialNature,
        initialDraft,
    })
    const natureLocked = purpose !== "create"
    const [pendingNature, setPendingNature] =
        React.useState<SalesOrderNature | null>(null)
    const procurementResponsibilityRef = React.useRef<{
        allResolved: boolean
        error?: unknown
        isFetching: boolean
    }>({ allResolved: false, isFetching: true })
    const blocksSalesOrderSubmit = React.useCallback(
        (value: { nature: SalesOrderNature }) => {
            if (value.nature !== "physical_service") return false
            const responsibility = procurementResponsibilityRef.current
            if (responsibility.error) {
                toast.add({
                    title: "无法核对采购负责人",
                    description: getErrorMessage(
                        responsibility.error,
                        "采购负责人暂时无法核对，请稍后重试。",
                    ),
                    type: "error",
                    timeout: 5000,
                })
                return true
            }
            if (responsibility.isFetching) {
                toast.add({
                    title: "正在核对采购负责人",
                    description: "请等待核对完成后再提交销售单。",
                    type: "warning",
                    timeout: 4000,
                })
                return true
            }
            if (responsibility.allResolved) return false
            toast.add({
                title: "暂不能提交销售单",
                description: "暂未确定采购负责人，请联系管理员维护采购责任规则",
                type: "warning",
                timeout: 5000,
            })
            return true
        },
        [],
    )

    const form = useAppForm({
        defaultValues,
        // 首次只在提交时整单校验；提交失败后改为随字段变更重跑。
        // 否则表单级 onSubmit 写到未挂载字段（如明细 name、ownerUserId）
        // 的错误不会被 setFieldValue / 兄弟字段变更清掉，提交按钮会一直 disabled。
        validationLogic: revalidateLogic(),
        validators: {
            onDynamic: ({ value }) =>
                validateSalesOrderForm(
                    value,
                    submission.submitIntentRef.current,
                ),
        },
        onSubmit: async ({ value }) => {
            if (submission.submitIntentRef.current === "SUBMIT") {
                if (blocksSalesOrderSubmit(value)) return
                submission.setSubmitConfirmOpen(true)
                return
            }
            await submission.handleSubmit(value, form)
        },
    })

    const dirty = useSelector(form.store, (state) => state.isDirty)
    const nature = useSelector(form.store, (state) => state.values.nature)
    const lineItems = useSelector(form.store, (state) => state.values.lineItems)
    const procurementResponsibilityQuery =
        useSalesLineProcurementResponsibilities({ nature, lines: lineItems })
    procurementResponsibilityRef.current = {
        allResolved: procurementResponsibilityQuery.allResolved,
        error: procurementResponsibilityQuery.error,
        isFetching: procurementResponsibilityQuery.isFetching,
    }
    useSalesOrderCreateUnloadGuard(dirty)

    React.useEffect(() => {
        const contract = contractQuery.data
        if (!contract) return
        const preferredRevision = preferredRevisionRef.current
            ? contract.revisionTimeline.find(
                  (revision) =>
                      revision.revisionId === preferredRevisionRef.current &&
                      revision.isCurrent,
              )
            : undefined
        const revision =
            preferredRevision ??
            contract.revisionTimeline.find((candidate) => candidate.isCurrent)
        form.setFieldValue(
            "requestedContractRevisionId",
            revision?.revisionId ?? contract.currentRevision.revisionId,
        )
        form.setFieldValue(
            "contractRevisionLabel",
            `${contract.contractNo}@v${revision?.revisionNo ?? contract.currentRevision.revisionNo}`,
        )
        form.setFieldValue("customerId", contract.customer.id)
        form.setFieldValue("customerName", contract.customer.displayName)
        form.setFieldValue(
            "settlementPartyId",
            contract.currentRevision.settlementParty.id,
        )
        form.setFieldValue(
            "settlementEntity",
            contract.currentRevision.settlementParty.displayName,
        )
        // 负责销售固定为当前登录用户，不随合同变更覆盖
        if (skipPaymentTermsResetRef.current) {
            // 继续编辑草稿：本次合同 effect 首次运行只是把当前合同重新同步一遍，
            // 已从草稿带回的付款条件不应被合同默认值覆盖；仅跳过这一次。
            skipPaymentTermsResetRef.current = false
        } else {
            const termLabel = contract.currentRevision.paymentTermSnapshot.label
            const termMatch = PAYMENT_TERM_OPTIONS.find(
                (o) => o.label === termLabel || o.value === termLabel,
            )
            form.setFieldValue("paymentTerms", termMatch?.value ?? "CONTRACT")
        }
    }, [contractQuery.data, form])

    /** 负责销售固定为当前登录用户。 */
    React.useEffect(() => {
        const profile = profileQuery.data
        if (!profile) return
        const userId = profile.userid?.trim()
        const displayName = (profile.name || profile.account || "").trim()
        if (!userId || !displayName) return
        form.setFieldValue("ownerUserId", userId)
        form.setFieldValue("ownerName", displayName)
    }, [form, profileQuery.data])

    const handleContractChange = React.useCallback(
        (contractId: string) => {
            preferredRevisionRef.current = ""
            setSelectedContractId(contractId)
            form.setFieldValue("requestedContractRevisionId", "")
            form.setFieldValue("contractRevisionLabel", "")
            form.setFieldValue("customerName", "")
            form.setFieldValue("settlementEntity", "")
        },
        [form],
    )

    const handleUploadSuccess = React.useCallback(
        async (result: UploadContractPdfResult) => {
            await queryClient.invalidateQueries({
                queryKey: entitySelectorKeys.all,
            })
            preferredRevisionRef.current = result.revisionId
            setSelectedContractId(result.contractId)
            form.setFieldValue("contractId", result.contractId)
            form.setFieldValue("requestedContractRevisionId", "")
            form.setFieldValue("contractRevisionLabel", "")
            form.setFieldValue("customerName", "")
            form.setFieldValue("settlementEntity", "")
        },
        [form, queryClient],
    )

    const applyNature = React.useCallback(
        (nature: SalesOrderNature) => {
            form.setFieldValue(
                "taxRatePercent",
                nature === "card_voucher" ? "6.00" : "13.00",
            )
            form.setFieldValue("targetMallId", "")
            form.setFieldValue("receivableDueDate", "")
            form.setFieldValue("lineItems", [createEmptyLine(nature)])
            setDraftSaved(null)
        },
        [form, setDraftSaved],
    )

    const editor = (
        <>
            <SalesOrderCreateAlerts
                profileError={profileQuery.error ?? null}
                formalFailure={submission.formalFailure}
                createError={submission.createMutation.error ?? null}
                draftSaved={submission.draftSaved}
            />

            {submission.approval || submission.draftIdentity ? (
                nature === "card_voucher" ? (
                    <VoucherSalesOrderApprovalArea
                        phase="draft"
                        approval={submission.approval}
                        documentId={submission.draftIdentity?.salesOrderId}
                    />
                ) : (
                    <SalesOrderApprovalArea
                        phase="draft"
                        approval={submission.approval}
                        documentId={submission.draftIdentity?.salesOrderId}
                    />
                )
            ) : null}

            <form
                onSubmit={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    void form.handleSubmit()
                }}
            >
                <div className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_17.5rem] xl:gap-5">
                    <div
                        className={cn(
                            surfacePanelClassName,
                            "min-w-0 overflow-hidden",
                        )}
                    >
                        <section className="border-b border-grid p-4 md:p-5 lg:p-6">
                            <div className="mb-4">
                                <h2 className="font-heading text-sm font-semibold">
                                    单据头
                                </h2>
                            </div>

                            <div className="space-y-5">
                                <SalesOrderCreateContractSection
                                    form={form}
                                    initialCustomerId={initialCustomerId}
                                    contractFetching={contractQuery.isFetching}
                                    onContractChange={handleContractChange}
                                    onUploadClick={() => setUploadOpen(true)}
                                />
                                <SalesOrderCreateHeaderFields
                                    form={form}
                                    natureLocked={natureLocked}
                                    profilePending={profileQuery.isPending}
                                    profileError={profileQuery.isError}
                                    applyNature={applyNature}
                                    onNatureChangeRequest={setPendingNature}
                                />
                            </div>
                        </section>

                        <SalesOrderCreateLineItemsSection
                            form={form}
                            procurementOwners={
                                procurementResponsibilityQuery.byRowKey
                            }
                        />

                        <SalesOrderCreateTotalBar
                            form={form}
                            purpose={purpose}
                            onSaveDraftClick={() => {
                                submission.submitIntentRef.current =
                                    "SAVE_DRAFT"
                            }}
                            onSubmitClick={() => {
                                submission.submitIntentRef.current = "SUBMIT"
                            }}
                        />
                    </div>

                    <aside className="hidden xl:block">
                        <SalesOrderCreateSummaryPanel form={form} />
                    </aside>
                </div>
            </form>

            <ContractUploadDialog
                open={uploadOpen}
                onOpenChange={setUploadOpen}
                initialCustomerId={initialCustomerId}
                onSuccess={(result) => {
                    void handleUploadSuccess(result)
                }}
            />

            <DiscardConfirmDialog
                open={pendingNature != null}
                onOpenChange={(open) => {
                    if (!open) setPendingNature(null)
                }}
                title="切换业务性质？"
                description="业务性质切换后，已填写的销售明细会被清空并重新开始，无法撤销。"
                confirmLabel="清空明细并切换"
                cancelLabel="取消"
                onConfirm={() => {
                    if (pendingNature) applyNature(pendingNature)
                    setPendingNature(null)
                }}
            />

            <SalesOrderCreateResubmitDialog
                open={submission.resubmitConfirmOpen}
                onOpenChange={submission.setResubmitConfirmOpen}
                evidence={submission.resubmitEvidence}
                onEvidenceChange={submission.setResubmitEvidence}
                pending={submission.isResubmitting}
                onConfirm={() => submission.confirmResubmit()}
            />

            {nature === "card_voucher" ? (
                <VoucherSalesOrderSubmitConfirmDialog
                    open={submission.submitConfirmOpen}
                    onOpenChange={submission.setSubmitConfirmOpen}
                    pending={submission.isSubmitting}
                    snapshot={buildSalesOrderSubmitSnapshot(form.state.values)}
                    onConfirm={() => {
                        if (blocksSalesOrderSubmit(form.state.values)) return
                        submission.setSubmitConfirmOpen(false)
                        void submission.handleSubmit(form.state.values, form)
                    }}
                />
            ) : (
                <SalesOrderSubmitConfirmDialog
                    open={submission.submitConfirmOpen}
                    onOpenChange={submission.setSubmitConfirmOpen}
                    pending={submission.isSubmitting}
                    snapshot={buildSalesOrderSubmitSnapshot(form.state.values)}
                    onConfirm={() => {
                        if (blocksSalesOrderSubmit(form.state.values)) return
                        submission.setSubmitConfirmOpen(false)
                        void submission.handleSubmit(form.state.values, form)
                    }}
                />
            )}
        </>
    )

    if (chrome === "none") return editor

    return <PageScaffold className="pb-8">{editor}</PageScaffold>
}
