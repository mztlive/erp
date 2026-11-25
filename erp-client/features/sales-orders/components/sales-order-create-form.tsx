"use client"

import * as React from "react"
import { useInquirySalesDraftPersistence } from "../hooks/use-inquiry-sales-draft-persistence"
import { inquirySaleScope } from "../lib/inquiry-sales-storage"
import type { CreateSalesOrderFormValues } from "../lib/sales-order-create-model"
import {
    inquirySourceForLines,
    type InquirySalesBasis,
} from "../api/inquiry-basis"
import { SalesOrderInquirySource } from "./sales-order-inquiry-source"
import { SalesOrderInquiryPickerDialog } from "./sales-order-inquiry-picker-dialog"
import { hasPermission } from "@/lib/permissions"
import type { SalesOrderSelectionBasis } from "@/features/sales-orders/api/selection-basis"
import { SalesOrderSelectionSource } from "./sales-order-selection-source"
import { useRouter } from "@/lib/navigation"

import { revalidateLogic, useSelector } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"
import {
    ArrowLeftRightIcon,
    ChevronDownIcon,
    ChevronUpIcon,
    ClipboardListIcon,
    MessageSquareTextIcon,
} from "lucide-react"

import {
    DetailFormGrid,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DiscardConfirmDialog,
    detailWorkspaceStyles,
} from "@/components/business"
import { applyApiFormError, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { SalesOrderCreateContractLoading } from "./sales-order-create-contract-loading"
import { SalesOrderCreateContractSummary } from "./sales-order-create-contract-summary"
import {
    PAYMENT_TERM_OPTIONS,
    welfareScenarioLabel,
} from "@/lib/business-options"
import { getErrorMessage } from "@/lib/api/errors"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    isCommandOutcomeUnknown,
    type FormalCommandKeyLedger,
} from "@/lib/formal-command"
import { ContractImportWorkspace } from "@/features/contracts/components/contract-import-workspace"
import { useContractFilePicker } from "@/features/contracts/hooks/use-contract-file-picker"
import { useContractCenterQuery } from "@/features/contracts/queries"
import type { UploadContractPdfResult } from "@/features/contracts/types"
import { useCustomerCenterQuery } from "@/features/customers/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { entitySelectorKeys } from "@/features/entity-selectors"
import type { SalesOrderDraftResumeData } from "@/features/sales-orders/api/sales-orders"
import {
    calculateGrossProfit,
    createEmptyLine,
    validateSalesOrderForm,
} from "@/features/sales-orders/lib/sales-order-create-model"
import { buildSalesOrderSubmitSnapshot } from "@/features/sales-orders/components/sales-order-submit-confirm-summary"
import type { SalesOrderNature } from "@/features/sales-orders/types"
import { useSalesOrderCreateCommandLedger } from "@/features/sales-orders/hooks/use-sales-order-create-command-ledger"
import { useSalesOrderCreateDefaults } from "@/features/sales-orders/hooks/use-sales-order-create-defaults"
import { useSalesOrderCreateSubmission } from "@/features/sales-orders/hooks/use-sales-order-create-submission"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { useSalesLineProcurementResponsibilities } from "@/features/sales-orders/hooks/use-sales-line-procurement-responsibilities"
import { useSalesLineImagesQuery } from "@/features/sales-orders/hooks/use-sales-line-images-query"
import { SalesOrderCreateAlerts } from "@/features/sales-orders/components/sales-order-create-alerts"
import { SalesOrderCreateContractSection } from "@/features/sales-orders/components/sales-order-create-contract-section"
import { SalesOrderContractPickerDialog } from "./sales-order-contract-picker-dialog"
import { SalesOrderCreateEntry } from "@/features/sales-orders/components/sales-order-create-entry"
import { SalesOrderCreateHeaderFields } from "@/features/sales-orders/components/sales-order-create-header-fields"
import { SalesOrderCreateLineItemsSection } from "@/features/sales-orders/components/sales-order-create-line-items-section"
import { SalesOrderCreateTotalBar } from "@/features/sales-orders/components/sales-order-create-total-bar"
import type { DeliveryWorkspaceTab } from "./sales-delivery-workspace-sheet"
import type { SalesLineFilterKey } from "../lib/sales-order-create-line-health"
import { SalesOrderNegativeProfitConfirmDialog } from "@/features/sales-orders/components/sales-order-negative-profit-confirm-dialog"
import { SalesOrderApprovalArea } from "@/features/sales-orders/components/sales-order-approval-area"
import { SalesOrderSubmitConfirmDialog } from "@/features/sales-orders/components/sales-order-submit-confirm-dialog"
import { VoucherSalesOrderApprovalArea } from "@/features/sales-orders/components/voucher-sales-order-approval-area"
import { VoucherSalesOrderSubmitConfirmDialog } from "@/features/sales-orders/components/voucher-sales-order-submit-confirm-dialog"

import type { SalesOrderEditorPurpose } from "@/features/sales-orders/lib/sales-order-create-form-types"

export type { SalesOrderEditorPurpose } from "@/features/sales-orders/lib/sales-order-create-form-types"

export function SalesOrderCreateForm({
    initialInquiryValues,
    initialInquiryBasis,
    initialSelectionBasis,
    initialCustomerId = "",
    initialContractId = "",
    initialContractRevisionId = "",
    initialNature = "physical_service",
    initialDraft = null,
    purpose = "create",
    chrome = "page",
    onSubmitted,
    onDirtyStateChange,
    commandLedger: commandLedgerProp,
}: {
    initialInquiryValues?: CreateSalesOrderFormValues
    initialInquiryBasis?: InquirySalesBasis
    initialSelectionBasis?: SalesOrderSelectionBasis
    initialCustomerId?: string
    initialContractId?: string
    initialContractRevisionId?: string
    initialNature?: SalesOrderNature
    /** 继续编辑时已有的可编辑内容；新建时为 `null`。 */
    initialDraft?: SalesOrderDraftResumeData | null
    purpose?: SalesOrderEditorPurpose
    /** page：独立建单页；none：嵌在对象中心内，外壳由详情页提供。 */
    chrome?: "page" | "none"
    onSubmitted?: (salesOrderId: string) => void
    onDirtyStateChange?: (state: { dirty: boolean; busy: boolean }) => void
    commandLedger?: FormalCommandKeyLedger
}) {
    const embeddedDraft = chrome === "none" && purpose === "draft"
    const source =
        initialDraft?.selectionSource ?? initialSelectionBasis?.source
    const inquirySource = initialDraft?.inquirySource ?? initialInquiryBasis
    const sourceCustomerId =
        initialInquiryBasis?.customerId ??
        initialDraft?.inquirySource?.customer_id ??
        initialSelectionBasis?.customerId ??
        (source ? initialDraft?.customerId : undefined)
    const router = useRouter()
    const [leaveConfirmOpen, setLeaveConfirmOpen] = React.useState(false)
    const [inquiryPickerOpen, setInquiryPickerOpen] = React.useState(false)
    const [negativeProfit, setNegativeProfit] = React.useState<string | null>(
        null,
    )
    const [headerCollapsed, setHeaderCollapsed] = React.useState(false)
    const expandHeader = React.useCallback(() => setHeaderCollapsed(false), [])
    const [remarkOpen, setRemarkOpen] = React.useState(
        Boolean(initialDraft?.remark || initialInquiryValues?.remark),
    )
    const [lineFilter, setLineFilter] =
        React.useState<SalesLineFilterKey>("all")
    const [deliverySheetRequest, setDeliverySheetRequest] = React.useState<{
        nonce: number
        tab: DeliveryWorkspaceTab
        rowKey?: string | null
    } | null>(null)
    const queryClient = useQueryClient()
    const profileQuery = useAccountProfileQuery()
    const [selectedContractId, setSelectedContractId] = React.useState(
        initialDraft?.contractId ||
            initialInquiryValues?.contractId ||
            initialContractId,
    )
    const [readyContractId, setReadyContractId] = React.useState("")
    const [contractPickerOpen, setContractPickerOpen] = React.useState(false)
    const [importedContractId, setImportedContractId] = React.useState("")
    const focusSalesFieldsRef = React.useRef(false)
    const [uploadOpen, setUploadOpen] = React.useState(false)
    const [uploadFile, setUploadFile] = React.useState<File | null>(null)
    const picker = useContractFilePicker((file) => {
        setUploadFile(file)
        setUploadOpen(true)
    })
    const [entryOpen, setEntryOpen] = React.useState(
        purpose === "create" &&
            !initialDraft &&
            !initialContractId &&
            !initialSelectionBasis &&
            !initialInquiryBasis,
    )
    const wasImportOpen = React.useRef(false)
    React.useEffect(() => {
        if (!uploadOpen && !wasImportOpen.current) return
        wasImportOpen.current = uploadOpen
        const frame = requestAnimationFrame(() => {
            const target = uploadOpen
                ? "contract-import-workspace-title"
                : entryOpen
                  ? "sales-orders-create-entry-resume-import"
                  : "sales-create-contract-title"
            const focusTarget =
                document.getElementById(target) ??
                document.getElementById("sales-orders-create-customer")
            focusTarget?.focus({ preventScroll: true })
        })
        return () => cancelAnimationFrame(frame)
    }, [uploadOpen, entryOpen])
    const preferredRevisionRef = React.useRef(
        initialDraft?.requestedContractRevisionId ||
            initialInquiryValues?.requestedContractRevisionId ||
            initialContractRevisionId,
    )
    /** 已同步的合同不会因后台刷新覆盖用户修改的结算主体。 */
    const synchronizedContractRef = React.useRef("")
    const keepDraftTermsRef = React.useRef(
        initialDraft != null || initialInquiryValues != null,
    )
    const contractQuery = useContractCenterQuery(selectedContractId)

    const commandLedger = useSalesOrderCreateCommandLedger(
        initialDraft?.salesOrderId ??
            (inquirySource
                ? `inquiry-${inquirySource.inquiry_id}-${inquirySource.feedback_round}`
                : source
                  ? `selection-${source.proposal_id}`
                  : "new-sales-order"),
        commandLedgerProp,
        initialInquiryBasis ? (profileQuery.data?.userid ?? "") : undefined,
    )
    const submission = useSalesOrderCreateSubmission({
        inquiryBasis: initialInquiryBasis,
        selectionProposalId: source?.proposal_id,
        initialDraft,
        commandLedger,
        onSubmitted,
    })
    const { setDraftSaved } = submission
    const defaultValues = useSalesOrderCreateDefaults({
        initialInquiryValues,
        initialInquiryBasis,
        initialSelectionBasis,
        initialCustomerId,
        initialContractId,
        initialContractRevisionId,
        initialNature,
        initialDraft,
    })
    const natureLocked =
        purpose !== "create" || Boolean(source) || Boolean(inquirySource)
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
            if (responsibility.error && !responsibility.allResolved) {
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
                if (
                    initialInquiryBasis &&
                    !submission.draftIdentity &&
                    !commandLedger.peek("create")
                ) {
                    inquirySourceForLines(initialInquiryBasis, value.lineItems)
                }
                submission.setSubmitConfirmOpen(true)
                return
            }
            await submission.handleSubmit(value, form)
        },
    })

    useInquirySalesDraftPersistence(
        form,
        initialInquiryBasis ? profileQuery.data?.userid : undefined,
        initialInquiryBasis
            ? inquirySaleScope(
                  initialInquiryBasis.inquiry_id,
                  initialInquiryBasis.feedback_round,
              )
            : "",
    )

    const resubmissionReasonRef = React.useRef<string | undefined>(undefined)
    const submitConfirmed = async (values: CreateSalesOrderFormValues) => {
        try {
            await submission.handleSubmit(
                values,
                form,
                resubmissionReasonRef.current,
            )
        } catch (error) {
            if (!isCommandOutcomeUnknown(error)) applyApiFormError(form, error)
        }
    }

    /** 负毛利在提交预览后再确认一次；待完善或非负毛利直接提交。 */
    const handleSubmitConfirm = (resubmissionReason?: string) => {
        resubmissionReasonRef.current = resubmissionReason
        const values = form.state.values
        if (blocksSalesOrderSubmit(values)) return
        const grossProfit =
            values.nature === "physical_service"
                ? calculateGrossProfit(values.lineItems)
                : null
        submission.setSubmitConfirmOpen(false)
        if (grossProfit !== null && compareDecimal(grossProfit, "0", 2) < 0) {
            setNegativeProfit(grossProfit)
            return
        }
        void submitConfirmed(values)
    }

    const handleNegativeProfitConfirm = () => {
        if (blocksSalesOrderSubmit(form.state.values)) return
        setNegativeProfit(null)
        void submitConfirmed(form.state.values)
    }

    const dirty = useSelector(
        form.store,
        (state) =>
            state.isDirty &&
            (!submission.savedValues ||
                JSON.stringify(state.values) !==
                    JSON.stringify(submission.savedValues)),
    )
    const nature = useSelector(form.store, (state) => state.values.nature)
    const orderBasis = useSelector(
        form.store,
        (state) => state.values.orderBasis,
    )
    const contractResolved = Boolean(
        selectedContractId &&
        readyContractId === selectedContractId &&
        contractQuery.data?.contractId === selectedContractId,
    )
    const waitingForContract =
        orderBasis === "contract" &&
        Boolean(selectedContractId) &&
        !contractResolved
    const sourceCustomerMismatch = Boolean(
        sourceCustomerId &&
        contractQuery.data &&
        contractQuery.data.customer.id !== sourceCustomerId,
    )
    const revisionUnavailable =
        sourceCustomerMismatch ||
        Boolean(
            contractQuery.data &&
            preferredRevisionRef.current &&
            (!contractQuery.data.revisionTimeline.some(
                (revision) =>
                    revision.revisionId === preferredRevisionRef.current,
            ) ||
                (!initialDraft &&
                    preferredRevisionRef.current !==
                        contractQuery.data.currentRevision.revisionId)),
        )
    const contractLoadFailed =
        contractQuery.isError ||
        (contractQuery.isSuccess &&
            (!contractQuery.data || revisionUnavailable))
    const showContractSummary = contractResolved
    const selectedCustomerId = useSelector(
        form.store,
        (state) => state.values.customerId,
    )
    const selectedSettlementPartyId = useSelector(
        form.store,
        (state) => state.values.settlementPartyId,
    )
    const contractDefaultSettlementId =
        contractQuery.data?.currentRevision.signingCompanyPartyId ||
        contractQuery.data?.currentRevision.settlementParty.id
    const settlementSnapshot =
        initialDraft &&
        initialDraft.settlementPartyId === selectedSettlementPartyId &&
        initialDraft.settlementEntity
            ? {
                  partyId: selectedSettlementPartyId,
                  displayName: initialDraft.settlementEntity,
                  partyCode: "",
              }
            : contractQuery.data &&
                contractDefaultSettlementId === selectedSettlementPartyId
              ? {
                    partyId: selectedSettlementPartyId,
                    displayName: contractQuery.data.currentRevision
                        .signingCompanyPartyId
                        ? contractQuery.data.currentRevision
                              .signingCompanyName ||
                          contractQuery.data.currentRevision.settlementParty
                              .displayName
                        : contractQuery.data.currentRevision.settlementParty
                              .displayName,
                    partyCode: "",
                }
              : undefined
    const customerQuery = useCustomerCenterQuery(
        orderBasis === "evidence" ? selectedCustomerId : "",
    )
    const lineItems = useSelector(form.store, (state) => state.values.lineItems)
    const skuDetails = useSalesLineImagesQuery(
        nature === "physical_service" &&
            !entryOpen &&
            !uploadOpen &&
            !waitingForContract
            ? lineItems.map((line) => line.sku)
            : [],
    )
    const procurementResponsibilityQuery =
        useSalesLineProcurementResponsibilities({ nature, lines: lineItems })
    procurementResponsibilityRef.current = {
        allResolved: procurementResponsibilityQuery.allResolved,
        error: procurementResponsibilityQuery.error,
        isFetching: procurementResponsibilityQuery.isFetching,
    }
    const hasUnsavedChanges = dirty || uploadOpen
    useWorkspaceDirtyState({
        dirty: chrome === "page" && hasUnsavedChanges,
        busy: chrome === "page" && submission.isSubmitting,
    })
    React.useEffect(() => {
        onDirtyStateChange?.({
            dirty: hasUnsavedChanges,
            busy: submission.isSubmitting,
        })
    }, [hasUnsavedChanges, onDirtyStateChange, submission.isSubmitting])

    React.useEffect(() => {
        const contract = contractQuery.data
        if (!selectedContractId || contract?.contractId !== selectedContractId)
            return
        if (
            synchronizedContractRef.current === selectedContractId ||
            revisionUnavailable
        )
            return
        synchronizedContractRef.current = selectedContractId
        const preferredRevision = preferredRevisionRef.current
            ? contract.revisionTimeline.find(
                  (revision) =>
                      revision.revisionId === preferredRevisionRef.current,
              )
            : undefined
        const revision =
            preferredRevision ??
            contract.revisionTimeline.find((candidate) => candidate.isCurrent)
        form.setFieldValue(
            "requestedContractRevisionId",
            revision?.revisionId ?? contract.currentRevision.revisionId,
            { dontUpdateMeta: true },
        )
        form.setFieldValue(
            "contractRevisionLabel",
            `${contract.contractNo}@v${revision?.revisionNo ?? contract.currentRevision.revisionNo}`,
            { dontUpdateMeta: true },
        )
        form.setFieldValue("customerId", contract.customer.id, {
            dontUpdateMeta: true,
        })
        form.setFieldValue("customerName", contract.customer.displayName, {
            dontUpdateMeta: true,
        })
        // 续编沿用已保存的商业条款及结算主体；新选合同使用合同默认值。
        if (keepDraftTermsRef.current) {
            keepDraftTermsRef.current = false
        } else {
            form.setFieldValue(
                "settlementPartyId",
                contract.currentRevision.signingCompanyPartyId ||
                    contract.currentRevision.settlementParty.id,
                { dontUpdateMeta: true },
            )
            form.setFieldValue(
                "settlementEntity",
                contract.currentRevision.signingCompanyPartyId
                    ? contract.currentRevision.signingCompanyName ||
                          contract.currentRevision.settlementParty.displayName
                    : contract.currentRevision.settlementParty.displayName,
                { dontUpdateMeta: true },
            )
            form.setFieldValue(
                "invoiceType",
                contract.currentRevision.invoiceRequirementSnapshot.titleType,
                { dontUpdateMeta: true },
            )
            form.setFieldValue(
                "taxRatePercent",
                contract.currentRevision.taxPoint
                    ?.trim()
                    .replace(/[%％]$/, "")
                    .trim() ?? "",
                { dontUpdateMeta: true },
            )
            const termLabel = contract.currentRevision.paymentTermSnapshot.label
            const termMatch = PAYMENT_TERM_OPTIONS.find(
                (o) => o.label === termLabel || o.value === termLabel,
            )
            form.setFieldValue("paymentTerms", termMatch?.value ?? "CONTRACT", {
                dontUpdateMeta: true,
            })
        }
        setReadyContractId(selectedContractId)
    }, [
        contractQuery.data,
        form,
        selectedContractId,
        readyContractId,
        revisionUnavailable,
    ])

    React.useEffect(() => {
        if (!contractResolved || uploadOpen || !focusSalesFieldsRef.current)
            return
        focusSalesFieldsRef.current = false
        const frame = requestAnimationFrame(() => {
            document
                .getElementById("sales-orders-create-header-welfare-scene")
                ?.focus({ preventScroll: true })
        })
        return () => cancelAnimationFrame(frame)
    }, [contractResolved, uploadOpen])

    React.useEffect(() => {
        if (orderBasis !== "evidence") return
        const customer = customerQuery.data
        if (!customer || customer.customerId !== selectedCustomerId) return
        form.setFieldValue("customerName", customer.currentRevision.legalName, {
            dontUpdateMeta: true,
        })
        if (!form.state.values.paymentTerms) {
            form.setFieldValue(
                "paymentTerms",
                customer.currentRevision.defaultPaymentTerm || "CONTRACT",
                { dontUpdateMeta: true },
            )
        }
    }, [customerQuery.data, form, orderBasis, selectedCustomerId])

    /** 负责销售固定为当前登录用户。 */
    React.useEffect(() => {
        const profile = profileQuery.data
        if (!profile) return
        const userId = profile.userid?.trim()
        const displayName = (profile.name || profile.account || "").trim()
        if (!userId || !displayName) return
        form.setFieldValue("ownerUserId", userId, { dontUpdateMeta: true })
        form.setFieldValue("ownerName", displayName, { dontUpdateMeta: true })
    }, [form, profileQuery.data])

    const handleContractChange = React.useCallback(
        (contractId: string) => {
            setReadyContractId("")
            setImportedContractId("")
            focusSalesFieldsRef.current = Boolean(contractId)
            preferredRevisionRef.current = ""
            synchronizedContractRef.current = ""
            keepDraftTermsRef.current = false
            setSelectedContractId(contractId)
            form.setFieldValue("requestedContractRevisionId", "")
            form.setFieldValue("contractRevisionLabel", "")
            if (contractId) {
                form.setFieldValue("settlementEntity", "")
                form.setFieldValue("settlementPartyId", "")
            }
        },
        [form],
    )

    const handleOrderBasisChange = (basis: "contract" | "evidence") => {
        if (basis === form.state.values.orderBasis) return
        // 已保存单据的开单依据沿用原单；上传期间不卸载凭证控件。
        if (submission.draftIdentity || form.state.values.evidenceUploadPending)
            return
        if (basis === "contract") {
            setContractPickerOpen(true)
            return
        }
        form.setFieldValue("orderBasis", basis)
        form.setFieldValue("contractId", "")
        handleContractChange("")
        form.setFieldValue("paymentTerms", "")
    }

    const handleUploadSuccess = React.useCallback(
        (result: UploadContractPdfResult) => {
            setReadyContractId("")
            setImportedContractId(result.contractId)
            focusSalesFieldsRef.current = true
            preferredRevisionRef.current = result.revisionId
            synchronizedContractRef.current = ""
            keepDraftTermsRef.current = false
            setSelectedContractId(result.contractId)
            form.setFieldValue("orderBasis", "contract")
            form.setFieldValue("contractId", result.contractId)
            form.setFieldValue("requestedContractRevisionId", "")
            form.setFieldValue("contractRevisionLabel", "")
            form.setFieldValue("customerName", "")
            form.setFieldValue("settlementEntity", "")
            form.setFieldValue("settlementPartyId", "")
            setEntryOpen(false)
            void queryClient.invalidateQueries({
                queryKey: entitySelectorKeys.all,
            })
        },
        [form, queryClient],
    )

    const applyNature = React.useCallback(
        (nature: SalesOrderNature) => {
            // 仅更新尚未核对的新单默认税率；识别、手工填写、合同与草稿条款保持不变。
            if (
                !form.getFieldMeta("taxRatePercent")?.isTouched &&
                !form.state.values.contractId &&
                !initialDraft
            ) {
                form.setFieldValue(
                    "taxRatePercent",
                    nature === "card_voucher" ? "6.00" : "13.00",
                    { dontUpdateMeta: true },
                )
            }
            form.setFieldValue("receivableDueDate", "")
            form.setFieldValue("nature", nature)
            form.setFieldValue(
                "lineItems",
                nature === "card_voucher" ? [createEmptyLine(nature)] : [],
            )
            setDraftSaved(null)
        },
        [form, initialDraft, setDraftSaved],
    )

    const owner = (
        <>
            <form.AppField name="ownerUserId">{() => null}</form.AppField>
            <form.AppField name="ownerName">
                {(field) => (
                    <span
                        id="sales-orders-create-header-owner-name"
                        tabIndex={-1}
                        className="inline-flex min-w-0 flex-wrap items-baseline gap-1.5 text-xs outline-none"
                        aria-live="polite"
                    >
                        <span className="text-muted-foreground">负责销售</span>
                        <span className="break-words font-medium text-foreground">
                            {profileQuery.isPending
                                ? "加载当前用户…"
                                : profileQuery.isError
                                  ? "无法获取登录用户"
                                  : field.state.value || "当前用户未就绪"}
                        </span>
                    </span>
                )}
            </form.AppField>
        </>
    )

    const approvalArea =
        submission.approval || submission.draftIdentity ? (
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
        ) : null

    const canChooseInquiry =
        purpose === "create" &&
        nature === "physical_service" &&
        !initialDraft &&
        !submission.draftIdentity &&
        !source &&
        !inquirySource &&
        !uploadOpen &&
        [
            "sales_inquiry:list",
            "sales_inquiry:convert_sales",
            "sales_order:create",
        ].every((permission) =>
            hasPermission(profileQuery.data?.permissions ?? [], permission),
        )
    const inquirySwitchDisabled =
        submission.isSubmitting || Boolean(submission.formalFailure?.unknown)
    const canChangeEntry =
        !entryOpen &&
        !uploadOpen &&
        purpose === "create" &&
        !submission.draftIdentity &&
        !source &&
        !inquirySource

    const chromeSecondaryActions =
        !entryOpen && !uploadOpen && (canChooseInquiry || canChangeEntry) ? (
            <>
                {canChooseInquiry ? (
                    <Button
                        id="sales-orders-create-from-inquiry"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        disabled={inquirySwitchDisabled}
                        onClick={() => setInquiryPickerOpen(true)}
                    >
                        <ClipboardListIcon
                            data-icon="inline-start"
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        从询价单带入
                    </Button>
                ) : null}
                {canChangeEntry ? (
                    <form.Subscribe
                        selector={(state) => state.values.evidenceUploadPending}
                    >
                        {(uploading) => (
                            <Button
                                id="sales-orders-create-change-entry"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={uploading || submission.isSubmitting}
                                onClick={() => setEntryOpen(true)}
                            >
                                <ArrowLeftRightIcon
                                    data-icon="inline-start"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                更换开单方式
                            </Button>
                        )}
                    </form.Subscribe>
                ) : null}
            </>
        ) : undefined

    const headerAlerts = (
        <>
            {!uploadOpen && source ? (
                <SalesOrderSelectionSource source={source} />
            ) : null}
            {!uploadOpen && inquirySource ? (
                <SalesOrderInquirySource source={inquirySource} />
            ) : null}
            <SalesOrderCreateAlerts
                formalFailure={submission.formalFailure}
                createError={submission.createMutation.error ?? null}
                draftSaved={submission.draftSaved}
            />
            {!uploadOpen && !embeddedDraft ? approvalArea : null}
        </>
    )

    const editor = (
        <>
            {inquiryPickerOpen ? (
                <SalesOrderInquiryPickerDialog
                    key={selectedCustomerId}
                    customerId={selectedCustomerId}
                    discardsChanges={
                        dirty ||
                        Boolean(uploadFile) ||
                        Boolean(selectedContractId)
                    }
                    onClose={() => setInquiryPickerOpen(false)}
                    onConfirm={(inquiryId, round) => {
                        setInquiryPickerOpen(false)
                        router.push(
                            `/sales/orders?mode=create&inquiryId=${encodeURIComponent(inquiryId)}&round=${round}`,
                        )
                    }}
                />
            ) : null}

            {chrome === "none" ? (
                <div className="space-y-2.5 pt-4 empty:hidden">
                    {headerAlerts}
                </div>
            ) : null}

            <div className={uploadOpen ? "hidden" : "contents"}>
                {entryOpen ? (
                    <SalesOrderCreateEntry
                        onSelectInquiry={
                            canChooseInquiry
                                ? () => setInquiryPickerOpen(true)
                                : undefined
                        }
                        inquiryDisabled={inquirySwitchDisabled}
                        selectedFileName={uploadFile?.name}
                        onResumeImport={() => setUploadOpen(true)}
                        onUpload={(file) => {
                            setUploadFile(file)
                            setUploadOpen(true)
                        }}
                        onSelectBasis={(basis) => {
                            if (basis === "contract") {
                                setContractPickerOpen(true)
                            } else {
                                handleOrderBasisChange(basis)
                                setEntryOpen(false)
                            }
                        }}
                    />
                ) : waitingForContract ? (
                    <SalesOrderCreateContractLoading
                        customerMismatch={sourceCustomerMismatch}
                        failed={contractLoadFailed}
                        imported={importedContractId === selectedContractId}
                        revisionUnavailable={revisionUnavailable}
                        fetching={contractQuery.isFetching}
                        canChange={!submission.draftIdentity}
                        onRetry={() => void contractQuery.refetch()}
                        onChange={() => setContractPickerOpen(true)}
                    />
                ) : (
                    <form
                        className="flex min-w-0 flex-1 flex-col"
                        noValidate
                        onSubmit={(event) => {
                            event.preventDefault()
                            event.stopPropagation()
                            // useAppForm 已将提交错误绑定到表单。
                            void form.handleSubmit().catch(() => undefined)
                        }}
                    >
                        <div
                            id={
                                embeddedDraft
                                    ? "sales-orders-draft-content-scroll"
                                    : undefined
                            }
                            className="min-w-0 flex-1"
                        >
                            {/* 顶部双栏紧凑工作头：左侧合同/客户依据，右侧本次销售信息与内部说明，桌面端满宽利用零死白 */}
                            <div className="border-b border-border/60">
                                {headerCollapsed ? (
                                    <form.Subscribe
                                        selector={(state) => ({
                                            customerName:
                                                state.values.customerName,
                                            contractRevisionLabel:
                                                state.values
                                                    .contractRevisionLabel,
                                            nature: state.values.nature,
                                            welfareScene:
                                                state.values.welfareScene,
                                            settlementEntity:
                                                state.values.settlementEntity,
                                            taxRatePercent:
                                                state.values.taxRatePercent,
                                            remark: state.values.remark,
                                        })}
                                    >
                                        {(summary) => (
                                            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5 py-2.5">
                                                <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1 text-xs text-muted-foreground">
                                                    <span className="text-body-compact font-semibold text-foreground">
                                                        {summary.customerName ||
                                                            "未选择客户"}
                                                    </span>
                                                    {summary.contractRevisionLabel ? (
                                                        <span className="num">
                                                            {
                                                                summary.contractRevisionLabel
                                                            }
                                                        </span>
                                                    ) : (
                                                        <span>
                                                            先开单，后补合同
                                                        </span>
                                                    )}
                                                    <span
                                                        aria-hidden="true"
                                                        className="text-muted-foreground/40"
                                                    >
                                                        ·
                                                    </span>
                                                    <span>
                                                        {summary.nature ===
                                                        "card_voucher"
                                                            ? "卡券"
                                                            : "实物与服务"}
                                                    </span>
                                                    {summary.welfareScene ? (
                                                        <>
                                                            <span
                                                                aria-hidden="true"
                                                                className="text-muted-foreground/40"
                                                            >
                                                                ·
                                                            </span>
                                                            <span>
                                                                {welfareScenarioLabel(
                                                                    summary.welfareScene,
                                                                )}
                                                            </span>
                                                        </>
                                                    ) : null}
                                                    {summary.settlementEntity ? (
                                                        <>
                                                            <span
                                                                aria-hidden="true"
                                                                className="text-muted-foreground/40"
                                                            >
                                                                ·
                                                            </span>
                                                            <span>
                                                                结算：
                                                                {
                                                                    summary.settlementEntity
                                                                }
                                                            </span>
                                                        </>
                                                    ) : null}
                                                    {summary.taxRatePercent ? (
                                                        <>
                                                            <span
                                                                aria-hidden="true"
                                                                className="text-muted-foreground/40"
                                                            >
                                                                ·
                                                            </span>
                                                            <span className="num">
                                                                税率{" "}
                                                                {
                                                                    summary.taxRatePercent
                                                                }
                                                                %
                                                            </span>
                                                        </>
                                                    ) : null}
                                                    {summary.remark ? (
                                                        <span className="rounded-sm bg-muted px-1.5 py-0.5 text-tiny text-muted-foreground">
                                                            已填说明
                                                        </span>
                                                    ) : null}
                                                </div>
                                                <Button
                                                    id="sales-orders-create-header-expand"
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    className={
                                                        detailWorkspaceStyles.subtleButton
                                                    }
                                                    onClick={() =>
                                                        setHeaderCollapsed(
                                                            false,
                                                        )
                                                    }
                                                >
                                                    <ChevronDownIcon
                                                        data-icon="inline-start"
                                                        className="size-3.5"
                                                        aria-hidden="true"
                                                    />
                                                    展开基本资料
                                                </Button>
                                            </div>
                                        )}
                                    </form.Subscribe>
                                ) : null}

                                <div
                                    className={
                                        headerCollapsed
                                            ? "hidden"
                                            : "grid min-w-0 grid-cols-1 gap-y-4 py-4 lg:grid-cols-12 lg:gap-y-0"
                                    }
                                >
                                    {orderBasis === "contract" ? (
                                        <section
                                            aria-labelledby="sales-create-contract-title"
                                            className="min-w-0 space-y-3 lg:col-span-5 lg:pr-6"
                                        >
                                            <div className="flex flex-wrap items-center justify-between gap-2">
                                                <div className="flex min-w-0 flex-wrap items-center gap-2">
                                                    <h2
                                                        id="sales-create-contract-title"
                                                        tabIndex={-1}
                                                        className={`${detailWorkspaceStyles.sectionTitle} outline-none`}
                                                    >
                                                        {showContractSummary
                                                            ? "合同信息"
                                                            : "客户与合同"}
                                                    </h2>
                                                    {showContractSummary ? (
                                                        <StatusBadge
                                                            tone="success"
                                                            appearance="quiet"
                                                            label={
                                                                importedContractId ===
                                                                selectedContractId
                                                                    ? "已归档并带入"
                                                                    : "已关联合同"
                                                            }
                                                        />
                                                    ) : null}
                                                </div>
                                                {chrome === "none"
                                                    ? owner
                                                    : null}
                                            </div>
                                            <DetailFormGrid layout="stack">
                                                {showContractSummary &&
                                                contractQuery.data ? (
                                                    <SalesOrderCreateContractSummary
                                                        form={form}
                                                        contract={
                                                            contractQuery.data
                                                        }
                                                        canChange={
                                                            !submission.draftIdentity &&
                                                            !submission.isSubmitting
                                                        }
                                                        onChange={() =>
                                                            setContractPickerOpen(
                                                                true,
                                                            )
                                                        }
                                                    />
                                                ) : (
                                                    <SalesOrderCreateContractSection
                                                        form={form}
                                                        sourceCustomerLocked={Boolean(
                                                            source,
                                                        )}
                                                        customerLocked={
                                                            submission.draftIdentity !=
                                                            null
                                                        }
                                                        onContractChange={
                                                            handleContractChange
                                                        }
                                                        onOrderBasisChange={
                                                            handleOrderBasisChange
                                                        }
                                                        onUploadClick={
                                                            picker.selectFile
                                                        }
                                                    />
                                                )}
                                            </DetailFormGrid>
                                        </section>
                                    ) : (
                                        <section
                                            aria-labelledby="sales-create-evidence-title"
                                            className="min-w-0 space-y-3 lg:col-span-5 lg:pr-6"
                                        >
                                            <div className="flex flex-wrap items-center justify-between gap-2">
                                                <div className="flex min-w-0 flex-wrap items-center gap-2">
                                                    <h2
                                                        id="sales-create-evidence-title"
                                                        tabIndex={-1}
                                                        className={`${detailWorkspaceStyles.sectionTitle} outline-none`}
                                                    >
                                                        客户与开单材料
                                                    </h2>
                                                    <StatusBadge
                                                        tone="warning"
                                                        appearance="quiet"
                                                        label="先开单，后补合同"
                                                    />
                                                </div>
                                                {chrome === "none"
                                                    ? owner
                                                    : null}
                                            </div>
                                            <DetailFormGrid layout="stack">
                                                <SalesOrderCreateContractSection
                                                    form={form}
                                                    sourceCustomerLocked={Boolean(
                                                        source,
                                                    )}
                                                    customerLocked={
                                                        submission.draftIdentity !=
                                                        null
                                                    }
                                                    onContractChange={
                                                        handleContractChange
                                                    }
                                                    onOrderBasisChange={
                                                        handleOrderBasisChange
                                                    }
                                                    onUploadClick={
                                                        picker.selectFile
                                                    }
                                                />
                                            </DetailFormGrid>
                                        </section>
                                    )}

                                    <section
                                        aria-labelledby="sales-create-settlement-title"
                                        className="min-w-0 space-y-3 border-t border-border/60 pt-4 lg:col-span-7 lg:border-t-0 lg:border-l lg:border-border/60 lg:pt-0 lg:pl-6"
                                    >
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <h2
                                                id="sales-create-settlement-title"
                                                className={
                                                    detailWorkspaceStyles.sectionTitle
                                                }
                                            >
                                                本次销售信息
                                            </h2>
                                            <div className="flex shrink-0 items-center gap-1">
                                                <form.AppField name="remark">
                                                    {(field) => (
                                                        <Button
                                                            id="sales-orders-create-remark-toggle"
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            aria-expanded={
                                                                remarkOpen ||
                                                                field.state.meta
                                                                    .errors
                                                                    .length > 0
                                                            }
                                                            className={
                                                                detailWorkspaceStyles.quietButton
                                                            }
                                                            onClick={() =>
                                                                setRemarkOpen(
                                                                    (v) => !v,
                                                                )
                                                            }
                                                        >
                                                            <MessageSquareTextIcon
                                                                data-icon="inline-start"
                                                                className="size-3.5"
                                                                aria-hidden="true"
                                                            />
                                                            {field.state.value
                                                                ? "内部说明（已填）"
                                                                : "添加内部说明"}
                                                        </Button>
                                                    )}
                                                </form.AppField>
                                                <Button
                                                    id="sales-orders-create-header-collapse"
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    className={
                                                        detailWorkspaceStyles.quietButton
                                                    }
                                                    onClick={() =>
                                                        setHeaderCollapsed(true)
                                                    }
                                                >
                                                    <ChevronUpIcon
                                                        data-icon="inline-start"
                                                        className="size-3.5"
                                                        aria-hidden="true"
                                                    />
                                                    收起
                                                </Button>
                                            </div>
                                        </div>

                                        <DetailFormGrid
                                            layout="stack"
                                            className="space-y-3"
                                        >
                                            <SalesOrderCreateHeaderFields
                                                compact={false}
                                                form={form}
                                                natureLocked={natureLocked}
                                                contractResolved={
                                                    showContractSummary
                                                }
                                                settlementSnapshot={
                                                    settlementSnapshot
                                                }
                                                applyNature={applyNature}
                                                onNatureChangeRequest={
                                                    setPendingNature
                                                }
                                            />
                                            <form.AppField name="remark">
                                                {(field) => (
                                                    <Collapsible
                                                        open={
                                                            remarkOpen ||
                                                            field.state.meta
                                                                .errors.length >
                                                                0
                                                        }
                                                        onOpenChange={
                                                            setRemarkOpen
                                                        }
                                                    >
                                                        <CollapsibleContent
                                                            keepMounted
                                                        >
                                                            <div className="border-t border-border/50 pt-3">
                                                                <field.TextareaField
                                                                    id="sales-orders-create-remark"
                                                                    label="内部说明（选填）"
                                                                    placeholder="补充客户确认、交付或内部协同说明"
                                                                    rows={2}
                                                                />
                                                            </div>
                                                        </CollapsibleContent>
                                                    </Collapsible>
                                                )}
                                            </form.AppField>
                                        </DetailFormGrid>
                                    </section>
                                </div>
                            </div>

                            <SalesOrderCreateLineItemsSection
                                inquirySource={inquirySource}
                                inquiryLocked={Boolean(inquirySource)}
                                skuDetails={skuDetails}
                                draftId={initialDraft?.salesOrderId}
                                form={form}
                                procurementOwners={
                                    procurementResponsibilityQuery.byRowKey
                                }
                                procurementFetching={
                                    procurementResponsibilityQuery.isFetching
                                }
                                procurementError={
                                    procurementResponsibilityQuery.isError &&
                                    !procurementResponsibilityQuery.data
                                }
                                activeFilter={lineFilter}
                                onActiveFilterChange={setLineFilter}
                                deliverySheetRequest={deliverySheetRequest}
                            />
                        </div>

                        <SalesOrderCreateTotalBar
                            form={form}
                            isSubmitting={submission.isSubmitting}
                            onSaveDraftClick={() => {
                                submission.submitIntentRef.current =
                                    "SAVE_DRAFT"
                            }}
                            onSubmitClick={() => {
                                submission.submitIntentRef.current = "SUBMIT"
                            }}
                            onExpandHeader={expandHeader}
                            onOpenDeliveryOverview={() =>
                                setDeliverySheetRequest({
                                    nonce: Date.now(),
                                    tab: inquirySource ? "product" : "overview",
                                })
                            }
                            onLocateIncompleteLines={() => {
                                setLineFilter("incomplete")
                                document
                                    .getElementById("sales-line-items-section")
                                    ?.scrollIntoView({
                                        behavior: "smooth",
                                        block: "start",
                                    })
                            }}
                        />
                    </form>
                )}
            </div>
            {contractPickerOpen ? (
                <SalesOrderContractPickerDialog
                    customerId={sourceCustomerId}
                    initialContractId={selectedContractId}
                    onClose={() => setContractPickerOpen(false)}
                    onConfirm={(contractId) => {
                        if (
                            contractId !== selectedContractId ||
                            !contractResolved
                        ) {
                            handleContractChange(contractId)
                        }
                        form.setFieldValue("orderBasis", "contract")
                        form.setFieldValue("contractId", contractId)
                        setContractPickerOpen(false)
                        setEntryOpen(false)
                    }}
                />
            ) : null}
            <div
                id="sales-orders-create-contract-import"
                className={!uploadOpen ? "hidden" : "min-w-0 py-5"}
            >
                <input
                    id="sales-orders-create-contract-upload-input"
                    type="file"
                    {...picker.inputProps}
                />
                <ContractImportWorkspace
                    presentation="page"
                    backLabel={entryOpen ? "返回开单方式" : "返回销售单"}
                    open={uploadOpen}
                    onOpenChange={setUploadOpen}
                    autoAccept
                    initialFile={uploadFile}
                    initialCustomerId={selectedCustomerId || undefined}
                    onSuccess={handleUploadSuccess}
                />
            </div>

            <DiscardConfirmDialog
                id="sales-orders-create-switch-nature"
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

            {nature === "card_voucher" ? (
                <VoucherSalesOrderSubmitConfirmDialog
                    open={submission.submitConfirmOpen}
                    onOpenChange={submission.setSubmitConfirmOpen}
                    pending={submission.isSubmitting}
                    snapshot={buildSalesOrderSubmitSnapshot(
                        form.state.values,
                        skuDetails.unitNames,
                    )}
                    approval={submission.approval}
                    resubmitting={
                        initialDraft?.rejected ||
                        submission.approval?.instance?.status === "REJECTED"
                    }
                    onConfirm={handleSubmitConfirm}
                />
            ) : (
                <SalesOrderSubmitConfirmDialog
                    open={submission.submitConfirmOpen}
                    onOpenChange={submission.setSubmitConfirmOpen}
                    pending={submission.isSubmitting}
                    snapshot={buildSalesOrderSubmitSnapshot(
                        form.state.values,
                        skuDetails.unitNames,
                    )}
                    approval={submission.approval}
                    resubmitting={
                        initialDraft?.rejected ||
                        submission.approval?.instance?.status === "REJECTED"
                    }
                    onConfirm={handleSubmitConfirm}
                />
            )}

            {negativeProfit === null ? null : (
                <SalesOrderNegativeProfitConfirmDialog
                    grossProfit={negativeProfit}
                    open
                    onOpenChange={(open) => {
                        if (!open) setNegativeProfit(null)
                    }}
                    onConfirm={handleNegativeProfitConfirm}
                />
            )}
        </>
    )

    if (chrome === "none") return editor

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={purpose === "create" ? "新建销售单" : "编辑销售单"}
                documentNumber={submission.draftIdentity?.documentNumber}
                version={submission.draftIdentity?.version}
                primaryStatus={
                    submission.draftIdentity
                        ? { label: "草稿", tone: "neutral" }
                        : undefined
                }
                titleExtra={
                    dirty ? (
                        <DetailWorkspaceDirtyBadge label="未保存" />
                    ) : undefined
                }
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            owner,
                            !entryOpen && !uploadOpen
                                ? orderBasis === "contract"
                                    ? "关联合同开单"
                                    : "先开单，后补合同"
                                : null,
                        ]}
                    />
                }
                secondaryActions={chromeSecondaryActions}
                back={{
                    id: "sales-orders-create-back-to-list",
                    label: "销售单列表",
                    onClick: () => {
                        if (dirty || uploadOpen) {
                            setLeaveConfirmOpen(true)
                            return
                        }
                        router.push("/sales/orders")
                    },
                }}
            />
            <DetailWorkspaceBody
                className="flex flex-col"
                alerts={headerAlerts}
            >
                {editor}
            </DetailWorkspaceBody>
            <DiscardConfirmDialog
                id="sales-orders-create-leave"
                open={leaveConfirmOpen}
                onOpenChange={setLeaveConfirmOpen}
                confirmLabel="放弃更改并返回"
                onConfirm={() => {
                    setLeaveConfirmOpen(false)
                    router.push("/sales/orders")
                }}
            />
        </DetailWorkspacePage>
    )
}
