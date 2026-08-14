"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { CircleAlertIcon, CircleCheckIcon, PlusIcon } from "lucide-react"
import { z } from "zod"

import {
    DiscardConfirmDialog,
    EditableLineItemTable,
    FormalActionConfirmDialog,
    MoneyValue,
    PageHeader,
    PageScaffold,
    StickyTotalBar,
    ValidationSummary,
    surfaceInsetClassName,
    surfacePanelClassName,
    type EditableLineItemColumn,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { getErrorMessage } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"
import { toFieldErrors, useAppForm } from "@/components/form"
import { useSelector } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"
import {
    PAYMENT_TERM_OPTIONS,
    WELFARE_SCENARIO_OPTIONS,
    paymentTermLabel,
} from "@/lib/business-options"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { ContractUploadDialog } from "@/features/contracts/contract-upload-dialog"
import { useContractCenterQuery } from "@/features/contracts/queries"
import type { UploadContractPdfResult } from "@/features/contracts/types"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    ContractSearchCombobox,
    MallSearchCombobox,
    SellableSkuSearchCombobox,
    VoucherCategorySearchCombobox,
    entitySelectorKeys,
} from "@/features/entity-selectors"
import {
    useCreateSalesOrderMutation,
    useResolveProcurementRejectionMutation,
    useSaveSalesOrderDraftMutation,
    useSubmitSalesOrderMutation,
} from "@/features/sales-orders/hooks/queries"
import type { SalesOrderDraftResumeData } from "@/features/sales-orders/api/sales-orders"
import type {
    ResolveProcurementRejectionPayload,
    SubmitSalesOrderInput,
} from "@/features/sales-orders/api/sales-orders"
import { prepareProcurementRejectionResolution } from "@/features/sales-orders/api/sales-orders"
import {
    calculateTotals,
    CARD_FORM_OPTIONS,
    createEmptyLine,
    decimalAtMost,
    decimalInput,
    deriveVoucherGiftPreview,
    errorMessage,
    hasMeaningfulLines,
    NATURE_OPTIONS,
    validateSalesOrderContractId,
    validateSalesOrderForm,
} from "@/features/sales-orders/lib/sales-order-create-model"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import type {
    CreateSalesOrderInput,
    SalesOrderCreateIntent,
    SalesOrderDraftLineInput,
    SalesOrderNature,
} from "@/features/sales-orders/types"
import { localOrderNo } from "@/features/sales-orders/api/mappers"

export type SalesOrderEditorPurpose = "create" | "draft" | "resubmit"

export type SalesOrderEditorResult = {
    status: "succeeded" | "blocked" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

const parseEvidenceReferenceIds = (value: string): string[] =>
    Array.from(
        new Set(
            value
                .split(/[\s,，;；]+/)
                .map((item) => item.trim())
                .filter(Boolean),
        ),
    )

const HEADER_VALIDATION_FIELDS = [
    { name: "contractId", label: "有效合同", targetId: "contractId" },
    { name: "ownerName", label: "负责销售", targetId: "ownerName" },
    { name: "welfareScene", label: "福利场景", targetId: "welfareScene" },
    { name: "paymentTerms", label: "付款条件", targetId: "paymentTerms" },
    {
        name: "fulfillmentDeadline",
        label: "履约期限",
        targetId: "fulfillmentDeadline",
    },
    { name: "targetMallId", label: "目标商城", targetId: "targetMallId" },
    {
        name: "receivableDueDate",
        label: "应收到期日",
        targetId: "receivableDueDate",
    },
    { name: "taxRatePercent", label: "税率", targetId: "taxRatePercent" },
    { name: "customerName", label: "客户", targetId: "contractId" },
    { name: "settlementEntity", label: "结算主体", targetId: "contractId" },
] as const

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
    const router = useRouter()
    const queryClient = useQueryClient()
    const createMutation = useCreateSalesOrderMutation()
    const saveDraftMutation = useSaveSalesOrderDraftMutation()
    const submitMutation = useSubmitSalesOrderMutation()
    const resubmitMutation = useResolveProcurementRejectionMutation()
    const commandScope = initialDraft?.salesOrderId ?? "new-sales-order"
    const commandLedgerRef = React.useRef<{
        scope: string
        ledger: FormalCommandKeyLedger
    } | null>(null)
    if (commandLedgerRef.current?.scope !== commandScope) {
        commandLedgerRef.current = {
            scope: commandScope,
            ledger: new FormalCommandKeyLedger(),
        }
    }
    const commandLedger = commandLedgerProp ?? commandLedgerRef.current.ledger
    const natureLocked = purpose !== "create"
    const [resubmitConfirmOpen, setResubmitConfirmOpen] = React.useState(false)
    const [resubmitEvidence, setResubmitEvidence] = React.useState("")
    const profileQuery = useAccountProfileQuery()
    const [selectedContractId, setSelectedContractId] = React.useState(
        initialDraft?.contractId || initialContractId,
    )
    const [uploadOpen, setUploadOpen] = React.useState(false)
    const preferredRevisionRef = React.useRef(initialContractRevisionId)
    const submitIntentRef = React.useRef<SalesOrderCreateIntent>("SAVE_DRAFT")
    const contractQuery = useContractCenterQuery(selectedContractId)
    /** 继续编辑场景下，合同派生 effect 首次运行时不要覆盖已从草稿带回的付款条件。 */
    const skipPaymentTermsResetRef = React.useRef(initialDraft != null)
    const [formalFailure, setFormalFailure] = React.useState<{
        unknown: boolean
        description: string
    } | null>(null)

    /** 继续编辑场景：草稿在后端的身份与乐观锁版本，保存草稿从"新建"切到"更新"。 */
    const [draftIdentity, setDraftIdentity] = React.useState<{
        salesOrderId: string
        documentNumber: string
        version: number
    } | null>(
        initialDraft
            ? {
                  salesOrderId: initialDraft.salesOrderId,
                  documentNumber: initialDraft.documentNumber,
                  version: initialDraft.version,
              }
            : null,
    )

    /**
     * 必须稳定：createEmptyLine 每次生成新 rowKey。
     * useAppForm 在 layout effect 里会 deep-compare defaultValues，
     * 未 touch 时若每次渲染都变，会 setState → 重渲染 → 死循环
     *（Maximum update depth exceeded @ Field）。
     */
    const defaultValues = React.useMemo(() => {
        const nature = initialDraft?.nature ?? initialNature
        return {
            contractId: initialDraft?.contractId || initialContractId,
            requestedContractRevisionId: initialContractRevisionId,
            contractRevisionLabel: "",
            customerId: initialCustomerId,
            customerName: "",
            settlementPartyId: "",
            settlementEntity: "",
            nature,
            ownerUserId: "",
            ownerName: "",
            welfareScene: initialDraft?.welfareScene ?? "",
            paymentTerms: initialDraft?.paymentTerms ?? "",
            fulfillmentDeadline: initialDraft?.fulfillmentDeadline ?? "",
            targetMallId: initialDraft?.targetMallId ?? "",
            receivableDueDate: initialDraft?.receivableDueDate ?? "",
            taxRatePercent:
                initialDraft?.taxRatePercent ??
                (nature === "card_voucher" ? "6.00" : "13.00"),
            remark: initialDraft?.remark ?? "",
            lineItems:
                initialDraft && initialDraft.lineItems.length > 0
                    ? initialDraft.lineItems
                    : [createEmptyLine(nature)],
        } satisfies CreateSalesOrderFormValues
        // initialDraft 由外层等查询完成后才挂载本组件，渲染期间引用稳定，可以放心入依赖数组。
    }, [
        initialContractId,
        initialContractRevisionId,
        initialCustomerId,
        initialNature,
        initialDraft,
    ])

    const form = useAppForm({
        defaultValues,
        validators: {
            onSubmit: ({ value }) =>
                validateSalesOrderForm(value, submitIntentRef.current),
        },
        onSubmit: async ({ value }) => {
            const draftContent = {
                nature: value.nature,
                ownerUserId: value.ownerUserId,
                ownerName: value.ownerName,
                welfareScene: value.welfareScene,
                paymentTerms:
                    paymentTermLabel(value.paymentTerms) || value.paymentTerms,
                fulfillmentDeadline: value.fulfillmentDeadline,
                targetMallId: value.targetMallId,
                receivableDueDate: value.receivableDueDate,
                taxRatePercent: value.taxRatePercent,
                remark: value.remark,
                lineItems: value.lineItems,
            }

            // 已经落过库的草稿：后续保存/提交都基于既有记录续接，不再新建。
            if (draftIdentity) {
                if (
                    purpose === "resubmit" &&
                    commandLedger.peek("procurement-rejection-resolution")
                ) {
                    setResubmitConfirmOpen(true)
                    return
                }

                let command =
                    commandLedger.peek<
                        Omit<SubmitSalesOrderInput, "idempotencyKey">
                    >("submit-existing")
                if (submitIntentRef.current === "SAVE_DRAFT" && !command) {
                    const saved = await saveDraftMutation.mutateAsync({
                        ...draftContent,
                        salesOrderId: draftIdentity.salesOrderId,
                        version: draftIdentity.version,
                        contract: {
                            contractId: value.contractId,
                            requestedContractRevisionId:
                                value.requestedContractRevisionId,
                        },
                    })
                    setDraftIdentity({
                        salesOrderId: draftIdentity.salesOrderId,
                        documentNumber: draftIdentity.documentNumber,
                        version: saved.version,
                    })
                    setDraftSaved({
                        documentNumber: draftIdentity.documentNumber,
                        savedAt: new Date(),
                    })
                    return
                }

                if (!command) {
                    const saved = await saveDraftMutation.mutateAsync({
                        ...draftContent,
                        salesOrderId: draftIdentity.salesOrderId,
                        version: draftIdentity.version,
                        contract: {
                            contractId: value.contractId,
                            requestedContractRevisionId:
                                value.requestedContractRevisionId,
                        },
                    })
                    setDraftIdentity({
                        salesOrderId: draftIdentity.salesOrderId,
                        documentNumber: draftIdentity.documentNumber,
                        version: saved.version,
                    })
                    if (purpose === "resubmit") {
                        setResubmitConfirmOpen(true)
                        return
                    }
                    command = commandLedger.acquire(
                        "submit-existing",
                        `sales:${draftIdentity.salesOrderId}:submit`,
                        {
                            salesOrderId: draftIdentity.salesOrderId,
                            version: saved.version,
                        },
                    )
                }
                if (!command) return
                try {
                    await submitMutation.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                    commandLedger.settle("submit-existing", "succeeded")
                    setFormalFailure(null)
                } catch (error) {
                    const settlement = classifyFormalCommandError(error)
                    commandLedger.settle("submit-existing", settlement)
                    setFormalFailure({
                        unknown: settlement === "unknown",
                        description:
                            settlement === "unknown"
                                ? "当前输入已保留，请使用本次操作重试；确认前不要再次提交。"
                                : getErrorMessage(
                                      error,
                                      "提交未完成，请重试。",
                                  ),
                    })
                    throw error
                }
                form.reset()
                onSubmitted?.(draftIdentity.salesOrderId)
                if (!onSubmitted) {
                    router.push(`/sales/orders/${draftIdentity.salesOrderId}`)
                }
                return
            }

            let command =
                commandLedger.peek<
                    Omit<CreateSalesOrderInput, "idempotencyKey">
                >("create")
            if (!command) {
                command = commandLedger.acquire("create", "sales:create", {
                    orderNo: localOrderNo(),
                    contract: {
                        contractId: value.contractId,
                        requestedContractRevisionId:
                            value.requestedContractRevisionId,
                    },
                    ...draftContent,
                    intent: submitIntentRef.current,
                })
            }
            if (!command) return
            let result
            try {
                result = await createMutation.mutateAsync({
                    ...command.payload,
                    idempotencyKey: command.idempotencyKey,
                })
                commandLedger.settle("create", "succeeded")
                setFormalFailure(null)
            } catch (error) {
                const settlement = classifyFormalCommandError(error)
                commandLedger.settle("create", settlement)
                setFormalFailure({
                    unknown: settlement === "unknown",
                    description:
                        settlement === "unknown"
                            ? "当前整单输入已保留，请使用本次操作重试；确认前不要再次创建。"
                            : getErrorMessage(error, "销售单未创建，请重试。"),
                })
                throw error
            }
            if (command.payload.intent === "SAVE_DRAFT") {
                setDraftIdentity({
                    salesOrderId: result.salesOrderId,
                    documentNumber: result.documentNumber,
                    version: result.workingCopyVersion ?? 1,
                })
                setDraftSaved({
                    documentNumber: result.documentNumber,
                    savedAt: new Date(),
                })
                return
            }
            form.reset()
            onSubmitted?.(result.salesOrderId)
            if (!onSubmitted) {
                router.push(`/sales/orders/${result.salesOrderId}`)
            }
        },
    })

    const dirty = useSelector(form.store, (state) => state.isDirty)
    const [draftSaved, setDraftSaved] = React.useState<{
        documentNumber: string
        savedAt: Date
    } | null>(null)
    const [pendingNature, setPendingNature] =
        React.useState<SalesOrderNature | null>(null)

    React.useEffect(() => {
        if (!dirty) return
        const onBeforeUnload = (e: BeforeUnloadEvent) => {
            e.preventDefault()
            e.returnValue = "当前输入尚未提交，刷新后将丢失。"
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    }, [dirty])

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
        [form],
    )

    /** 明细表根路径校验错误（如卡券仅一条）在明细区汇总展示。 */
    const lineItemIssues = useSelector(form.store, (state) => {
        return toFieldErrors(state.fieldMeta.lineItems?.errors ?? [])
            .filter((error) => Boolean(error?.message))
            .map((error, index) => ({
                id: `line-items-${index}`,
                label: "销售明细",
                message: error!.message!,
                targetId: "sales-line-items-section",
            }))
    })

    /** 提交失败后汇总单据头错误，避免只拦提交却看不到原因。 */
    const headerIssues = useSelector(form.store, (state) => {
        if (state.submissionAttempts === 0) return []
        return HEADER_VALIDATION_FIELDS.flatMap((field) => {
            const meta = state.fieldMeta[field.name]
            return toFieldErrors(meta?.errors ?? [])
                .filter((error) => Boolean(error?.message))
                .map((error, index) => ({
                    id: `${field.name}-${index}`,
                    label: field.label,
                    message: error!.message!,
                    targetId: field.targetId,
                }))
        })
    })

    const editor = (
        <>
            {profileQuery.isError ? (
                <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>当前用户信息加载失败</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(
                            profileQuery.error,
                            "无法获取当前登录用户，请刷新后重试。",
                        )}
                    </AlertDescription>
                </Alert>
            ) : null}

            {formalFailure ? (
                <Alert
                    variant={formalFailure.unknown ? "warning" : "destructive"}
                >
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                        {formalFailure.unknown
                            ? "处理结果待确认"
                            : "操作未完成"}
                    </AlertTitle>
                    <AlertDescription>
                        {formalFailure.description}
                    </AlertDescription>
                </Alert>
            ) : createMutation.isError ? (
                <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>销售单未创建</AlertTitle>
                    <AlertDescription>
                        {errorMessage(createMutation.error)}
                    </AlertDescription>
                </Alert>
            ) : null}

            {draftSaved ? (
                <Alert variant="success">
                    <CircleCheckIcon aria-hidden="true" />
                    <AlertTitle>草稿已保存</AlertTitle>
                    <AlertDescription>
                        销售单 {draftSaved.documentNumber} 已保存为草稿（
                        {draftSaved.savedAt.toLocaleTimeString("zh-CN")}
                        ）。当前内容仍保留在本页，可继续完善后提交；草稿也会出现在销售单列表中。
                    </AlertDescription>
                </Alert>
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
                        <section className="border-b border-border/30 p-4 md:p-5 lg:p-6">
                            <div className="mb-4">
                                <h2 className="font-heading text-sm font-semibold">
                                    单据头
                                </h2>
                            </div>

                            <div className="space-y-5">
                                <form.Subscribe
                                    selector={(state) => ({
                                        contractRevisionLabel:
                                            state.values.contractRevisionLabel,
                                        customerName: state.values.customerName,
                                        settlementEntity:
                                            state.values.settlementEntity,
                                    })}
                                >
                                    {({
                                        contractRevisionLabel,
                                        customerName,
                                        settlementEntity,
                                    }) => (
                                        <div className="space-y-3">
                                            <form.AppField
                                                name="contractId"
                                                validators={{
                                                    onSubmit: ({ value }) =>
                                                        validateSalesOrderContractId(
                                                            value,
                                                        ),
                                                }}
                                            >
                                                {(field) => {
                                                    const isInvalid =
                                                        field.state.meta
                                                            .isTouched &&
                                                        !field.state.meta
                                                            .isValid
                                                    const errors =
                                                        toFieldErrors(
                                                            field.state.meta
                                                                .errors,
                                                        )
                                                    return (
                                                        <Field
                                                            id="contractId"
                                                            tabIndex={-1}
                                                            data-invalid={
                                                                isInvalid ||
                                                                undefined
                                                            }
                                                        >
                                                            <FieldLabel htmlFor="contractId">
                                                                有效合同
                                                            </FieldLabel>
                                                            <div className="flex items-start gap-2">
                                                                <div className="min-w-0 flex-1">
                                                                    <ContractSearchCombobox
                                                                        value={
                                                                            field
                                                                                .state
                                                                                .value ||
                                                                            undefined
                                                                        }
                                                                        onValueChange={(
                                                                            id,
                                                                        ) => {
                                                                            const next =
                                                                                id ??
                                                                                ""
                                                                            field.handleChange(
                                                                                next,
                                                                            )
                                                                            handleContractChange(
                                                                                next,
                                                                            )
                                                                        }}
                                                                        customerId={
                                                                            initialCustomerId ||
                                                                            undefined
                                                                        }
                                                                        selectableOnly
                                                                        placeholder="搜索合同编号或客户"
                                                                        emptyLabel="暂无可用合同，请点加号上传"
                                                                    />
                                                                </div>
                                                                <Button
                                                                    type="button"
                                                                    variant="outline"
                                                                    size="icon"
                                                                    className="shrink-0"
                                                                    aria-label="上传合同 PDF"
                                                                    title="上传合同 PDF"
                                                                    onClick={() =>
                                                                        setUploadOpen(
                                                                            true,
                                                                        )
                                                                    }
                                                                >
                                                                    <PlusIcon aria-hidden="true" />
                                                                </Button>
                                                            </div>
                                                            {isInvalid ? (
                                                                <FieldError
                                                                    errors={
                                                                        errors
                                                                    }
                                                                />
                                                            ) : null}
                                                        </Field>
                                                    )
                                                }}
                                            </form.AppField>
                                            {contractRevisionLabel ||
                                            customerName ||
                                            settlementEntity ? (
                                                <div
                                                    className={cn(
                                                        surfaceInsetClassName,
                                                        "flex flex-wrap items-center gap-2 px-3 py-2.5 text-xs",
                                                    )}
                                                >
                                                    {contractRevisionLabel ? (
                                                        <Badge
                                                            variant="outline"
                                                            className="font-normal"
                                                        >
                                                            {
                                                                contractRevisionLabel
                                                            }
                                                        </Badge>
                                                    ) : null}
                                                    {customerName ? (
                                                        <span className="text-muted-foreground">
                                                            客户{" "}
                                                            <span className="text-foreground">
                                                                {customerName}
                                                            </span>
                                                        </span>
                                                    ) : null}
                                                    {settlementEntity ? (
                                                        <span className="text-muted-foreground">
                                                            · 结算{" "}
                                                            <span className="text-foreground">
                                                                {
                                                                    settlementEntity
                                                                }
                                                            </span>
                                                        </span>
                                                    ) : null}
                                                    {contractQuery.isFetching ? (
                                                        <span className="text-muted-foreground">
                                                            加载中…
                                                        </span>
                                                    ) : null}
                                                </div>
                                            ) : (
                                                <p className="text-xs leading-relaxed text-muted-foreground">
                                                    选择合同后自动带出版本、客户与结算主体；无合同时点加号上传
                                                    PDF。
                                                </p>
                                            )}
                                        </div>
                                    )}
                                </form.Subscribe>

                                <div className="grid gap-x-4 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
                                    <form.AppField name="nature">
                                        {(field) => (
                                            <field.SelectField
                                                label="业务性质"
                                                options={NATURE_OPTIONS}
                                                disabled={natureLocked}
                                                description={
                                                    natureLocked
                                                        ? "建单后不能改"
                                                        : undefined
                                                }
                                                onValueChange={(value) => {
                                                    const nature =
                                                        value as SalesOrderNature
                                                    if (
                                                        natureLocked ||
                                                        nature ===
                                                            field.state.value
                                                    )
                                                        return
                                                    const lines =
                                                        form.state.values
                                                            .lineItems
                                                    if (
                                                        hasMeaningfulLines(
                                                            lines,
                                                        )
                                                    ) {
                                                        setPendingNature(nature)
                                                        return
                                                    }
                                                    applyNature(nature)
                                                }}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="ownerName">
                                        {(field) => (
                                            <field.TextField
                                                label="负责销售"
                                                disabled
                                                placeholder={
                                                    profileQuery.isPending
                                                        ? "加载当前用户…"
                                                        : profileQuery.isError
                                                          ? "无法获取登录用户"
                                                          : "当前登录用户"
                                                }
                                                description="固定为当前登录用户，不可更改"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name="welfareScene"
                                        validators={{
                                            onBlur: z
                                                .string()
                                                .trim()
                                                .min(1, "请选择福利场景")
                                                .refine(
                                                    (value) =>
                                                        WELFARE_SCENARIO_OPTIONS.some(
                                                            (o) =>
                                                                o.value ===
                                                                value,
                                                        ),
                                                    "请选择有效的福利场景",
                                                ),
                                        }}
                                    >
                                        {(field) => (
                                            <field.SelectField
                                                label="福利场景"
                                                options={
                                                    WELFARE_SCENARIO_OPTIONS
                                                }
                                                placeholder="选择福利场景"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name="paymentTerms"
                                        validators={{
                                            onBlur: z
                                                .string()
                                                .min(1, "请选择付款条件"),
                                        }}
                                    >
                                        {(field) => (
                                            <field.SelectField
                                                label="付款条件"
                                                options={PAYMENT_TERM_OPTIONS}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name="fulfillmentDeadline"
                                        validators={{
                                            onBlur: z
                                                .string()
                                                .min(1, "请选择履约期限"),
                                        }}
                                    >
                                        {(field) => (
                                            <field.DateField label="履约期限" />
                                        )}
                                    </form.AppField>
                                    <form.Subscribe
                                        selector={(state) =>
                                            state.values.nature
                                        }
                                    >
                                        {(nature) =>
                                            nature === "card_voucher" ? (
                                                <>
                                                    <form.AppField
                                                        name="targetMallId"
                                                        validators={{
                                                            onBlur: z
                                                                .string()
                                                                .trim()
                                                                .min(
                                                                    1,
                                                                    "请选择目标商城",
                                                                ),
                                                        }}
                                                    >
                                                        {(field) => {
                                                            const isInvalid =
                                                                field.state.meta
                                                                    .isTouched &&
                                                                !field.state
                                                                    .meta
                                                                    .isValid
                                                            const errors =
                                                                toFieldErrors(
                                                                    field.state
                                                                        .meta
                                                                        .errors,
                                                                )
                                                            return (
                                                                <Field
                                                                    data-invalid={
                                                                        isInvalid ||
                                                                        undefined
                                                                    }
                                                                >
                                                                    <FieldLabel htmlFor="targetMallId">
                                                                        目标商城
                                                                    </FieldLabel>
                                                                    <MallSearchCombobox
                                                                        purpose="form"
                                                                        value={
                                                                            field
                                                                                .state
                                                                                .value ||
                                                                            undefined
                                                                        }
                                                                        onValueChange={(
                                                                            id,
                                                                        ) =>
                                                                            field.handleChange(
                                                                                id ??
                                                                                    "",
                                                                            )
                                                                        }
                                                                        onBlur={
                                                                            field.handleBlur
                                                                        }
                                                                        placeholder="选择执行投影目标商城"
                                                                        emptyLabel="暂无启用中的商城"
                                                                    />
                                                                    {isInvalid ? (
                                                                        <FieldError
                                                                            errors={
                                                                                errors
                                                                            }
                                                                        />
                                                                    ) : null}
                                                                </Field>
                                                            )
                                                        }}
                                                    </form.AppField>
                                                    <form.AppField
                                                        name="receivableDueDate"
                                                        validators={{
                                                            onBlur: z
                                                                .string()
                                                                .min(
                                                                    1,
                                                                    "请选择应收到期日",
                                                                ),
                                                        }}
                                                    >
                                                        {(field) => (
                                                            <field.DateField
                                                                label="应收到期日"
                                                                description="运营通过后按此日期形成应收；该日期不能早于提交日"
                                                            />
                                                        )}
                                                    </form.AppField>
                                                </>
                                            ) : null
                                        }
                                    </form.Subscribe>
                                    <form.AppField
                                        name="taxRatePercent"
                                        validators={{
                                            onBlur: decimalInput(
                                                "税率",
                                                6,
                                            ).refine(
                                                (value) =>
                                                    decimalAtMost(
                                                        value,
                                                        "100",
                                                        6,
                                                    ),
                                                "税率不能超过 100%",
                                            ),
                                        }}
                                    >
                                        {(field) => (
                                            <field.TextField
                                                label="税率（%）"
                                                type="number"
                                                inputClassName="num"
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            </div>
                        </section>

                        <section
                            id="sales-line-items-section"
                            className="border-b border-border/30 p-4 md:p-5 lg:p-6"
                        >
                            <div className="mb-4 flex items-center justify-between gap-2">
                                <h2 className="font-heading text-sm font-semibold">
                                    销售明细
                                </h2>
                                <form.Subscribe
                                    selector={(state) => state.values.nature}
                                >
                                    {(nature) => (
                                        <Badge
                                            variant="outline"
                                            className="font-normal"
                                        >
                                            {nature === "card_voucher"
                                                ? "卡券 · 仅一条"
                                                : "实物/服务 · 可多行"}
                                        </Badge>
                                    )}
                                </form.Subscribe>
                            </div>

                            <form.Subscribe selector={(state) => state.values}>
                                {(values) => {
                                    const nature = values.nature
                                    const columns: EditableLineItemColumn<SalesOrderDraftLineInput>[] =
                                        [
                                            {
                                                id: "item",
                                                header: "销售项目",
                                                renderValue: ({ item }) =>
                                                    item.name,
                                                renderEditor: ({ rowIndex }) =>
                                                    nature ===
                                                    "card_voucher" ? (
                                                        <div className="min-w-52">
                                                            <form.AppField
                                                                name={`lineItems[${rowIndex}].sku`}
                                                            >
                                                                {(field) => (
                                                                    <VoucherCategorySearchCombobox
                                                                        value={
                                                                            field
                                                                                .state
                                                                                .value ||
                                                                            undefined
                                                                        }
                                                                        onValueChange={(
                                                                            id,
                                                                        ) => {
                                                                            // 提交 voucher_category_sku_id 用 SKU 稳定 id
                                                                            field.handleChange(
                                                                                id ??
                                                                                    "",
                                                                            )
                                                                        }}
                                                                        onItemChange={(
                                                                            category,
                                                                        ) => {
                                                                            form.setFieldValue(
                                                                                `lineItems[${rowIndex}].skuRevisionId`,
                                                                                category?.revisionId ??
                                                                                    "",
                                                                            )
                                                                            form.setFieldValue(
                                                                                `lineItems[${rowIndex}].name`,
                                                                                category?.name ??
                                                                                    "",
                                                                            )
                                                                            form.setFieldValue(
                                                                                `lineItems[${rowIndex}].unit`,
                                                                                "张",
                                                                            )
                                                                        }}
                                                                        selectedItem={
                                                                            values
                                                                                .lineItems[
                                                                                rowIndex
                                                                            ]
                                                                                ?.sku
                                                                                ? {
                                                                                      productId:
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .sku,
                                                                                      revisionId:
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .skuRevisionId,
                                                                                      sku: values
                                                                                          .lineItems[
                                                                                          rowIndex
                                                                                      ]
                                                                                          .sku,
                                                                                      name:
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .name ||
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .sku,
                                                                                      baseUnit:
                                                                                          "张",
                                                                                  }
                                                                                : undefined
                                                                        }
                                                                        placeholder="搜索卡券类目"
                                                                        emptyLabel="暂无可用的卡券类目"
                                                                    />
                                                                )}
                                                            </form.AppField>
                                                        </div>
                                                    ) : (
                                                        <div className="min-w-48">
                                                            <form.AppField
                                                                name={`lineItems[${rowIndex}].sku`}
                                                            >
                                                                {(field) => (
                                                                    <SellableSkuSearchCombobox
                                                                        value={
                                                                            field
                                                                                .state
                                                                                .value ||
                                                                            undefined
                                                                        }
                                                                        onValueChange={(
                                                                            id,
                                                                        ) => {
                                                                            // 公司商品池稳定身份 = sku_id
                                                                            field.handleChange(
                                                                                id ??
                                                                                    "",
                                                                            )
                                                                        }}
                                                                        onItemChange={(
                                                                            product,
                                                                        ) => {
                                                                            form.setFieldValue(
                                                                                `lineItems[${rowIndex}].skuRevisionId`,
                                                                                product?.revisionId ??
                                                                                    "",
                                                                            )
                                                                            form.setFieldValue(
                                                                                `lineItems[${rowIndex}].name`,
                                                                                product?.name ??
                                                                                    "",
                                                                            )
                                                                            form.setFieldValue(
                                                                                `lineItems[${rowIndex}].unit`,
                                                                                product?.baseUnit ??
                                                                                    "",
                                                                            )
                                                                        }}
                                                                        excludeProductKind="VOUCHER"
                                                                        selectedItem={
                                                                            values
                                                                                .lineItems[
                                                                                rowIndex
                                                                            ]
                                                                                ?.sku
                                                                                ? {
                                                                                      productId:
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .sku,
                                                                                      revisionId:
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .skuRevisionId,
                                                                                      sku: values
                                                                                          .lineItems[
                                                                                          rowIndex
                                                                                      ]
                                                                                          .sku,
                                                                                      name:
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .name ||
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .sku,
                                                                                      baseUnit:
                                                                                          values
                                                                                              .lineItems[
                                                                                              rowIndex
                                                                                          ]
                                                                                              .unit,
                                                                                  }
                                                                                : undefined
                                                                        }
                                                                        placeholder="搜索 SKU 或商品名称"
                                                                        emptyLabel="暂无可用的实物/服务 SKU（已排除卡券）"
                                                                    />
                                                                )}
                                                            </form.AppField>
                                                        </div>
                                                    ),
                                            },
                                            {
                                                id: "quantity",
                                                header: "数量 / 单位",
                                                numeric: true,
                                                renderValue: ({ item }) =>
                                                    `${item.quantity} ${item.unit}`,
                                                renderEditor: ({
                                                    rowIndex,
                                                }) => {
                                                    const line =
                                                        values.lineItems[
                                                            rowIndex
                                                        ]
                                                    const unitLocked =
                                                        nature ===
                                                            "card_voucher" ||
                                                        Boolean(
                                                            line?.sku?.trim(),
                                                        )
                                                    return (
                                                        <div className="flex min-w-32 items-center gap-2">
                                                            <div className="w-20 shrink-0">
                                                                <form.AppField
                                                                    name={`lineItems[${rowIndex}].quantity`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.TextField
                                                                            label="数量"
                                                                            hideLabel
                                                                            type="number"
                                                                            inputClassName="num"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>
                                                            <form.AppField
                                                                name={`lineItems[${rowIndex}].unit`}
                                                            >
                                                                {(field) => (
                                                                    <span
                                                                        className="min-w-8 shrink-0 text-sm text-muted-foreground"
                                                                        title={
                                                                            unitLocked
                                                                                ? nature ===
                                                                                  "card_voucher"
                                                                                    ? "卡券单位固定为张"
                                                                                    : "单位随 SKU 基础单位带出，不可改"
                                                                                : "选择 SKU 后带出基础单位"
                                                                        }
                                                                    >
                                                                        {field
                                                                            .state
                                                                            .value ||
                                                                            "—"}
                                                                    </span>
                                                                )}
                                                            </form.AppField>
                                                        </div>
                                                    )
                                                },
                                            },
                                            {
                                                id: "unitPrice",
                                                header: "含税单价",
                                                numeric: true,
                                                align: "end",
                                                renderValue: ({ item }) =>
                                                    item.unitPriceGross,
                                                renderEditor: ({
                                                    rowIndex,
                                                }) => (
                                                    <form.AppField
                                                        name={`lineItems[${rowIndex}].unitPriceGross`}
                                                    >
                                                        {(field) => (
                                                            <field.TextField
                                                                label="含税单价"
                                                                hideLabel
                                                                type="number"
                                                                inputClassName="num min-w-24 text-right"
                                                            />
                                                        )}
                                                    </form.AppField>
                                                ),
                                            },
                                            ...(nature === "card_voucher"
                                                ? ([
                                                      {
                                                          id: "faceValue",
                                                          header: "面值",
                                                          numeric: true,
                                                          align: "end",
                                                          renderValue: ({
                                                              item,
                                                          }) =>
                                                              item.faceValue ||
                                                              "—",
                                                          renderEditor: ({
                                                              rowIndex,
                                                          }) => (
                                                              <div className="min-w-20">
                                                                  <form.AppField
                                                                      name={`lineItems[${rowIndex}].faceValue`}
                                                                  >
                                                                      {(
                                                                          field,
                                                                      ) => (
                                                                          <field.TextField
                                                                              label="面值"
                                                                              hideLabel
                                                                              type="number"
                                                                              placeholder="0.00"
                                                                              className="gap-0"
                                                                              inputClassName="num min-w-20 text-right"
                                                                          />
                                                                      )}
                                                                  </form.AppField>
                                                              </div>
                                                          ),
                                                      },
                                                      {
                                                          id: "gift",
                                                          header: "配赠",
                                                          numeric: true,
                                                          align: "end",
                                                          renderValue: ({
                                                              item,
                                                          }) => {
                                                              const gift =
                                                                  deriveVoucherGiftPreview(
                                                                      item.faceValue,
                                                                      item.unitPriceGross,
                                                                      item.quantity,
                                                                  )
                                                              return gift
                                                                  ? `${gift.giftRatePercent}%`
                                                                  : "—"
                                                          },
                                                          renderEditor: ({
                                                              rowIndex,
                                                          }) => {
                                                              const line =
                                                                  values
                                                                      .lineItems[
                                                                      rowIndex
                                                                  ]
                                                              const gift =
                                                                  deriveVoucherGiftPreview(
                                                                      line?.faceValue ??
                                                                          "",
                                                                      line?.unitPriceGross ??
                                                                          "",
                                                                      line?.quantity ??
                                                                          "",
                                                                  )
                                                              return (
                                                                  <span
                                                                      className="num flex h-8 min-w-16 items-center justify-end text-sm tabular-nums text-muted-foreground"
                                                                      title={
                                                                          gift
                                                                              ? `配赠率 ${gift.giftRatePercent}%（金额 ${gift.giftAmount}）。配赠 = 面值小计 − 成交金额，系统计算不可改。`
                                                                              : "配赠率 = 配赠金额 / 成交金额；填入面值、单价与数量后自动计算"
                                                                      }
                                                                  >
                                                                      {gift
                                                                          ? `${gift.giftRatePercent}%`
                                                                          : "—"}
                                                                  </span>
                                                              )
                                                          },
                                                      },
                                                      {
                                                          id: "cardForm",
                                                          header: "卡形态",
                                                          renderValue: ({
                                                              item,
                                                          }) =>
                                                              item.cardForm ||
                                                              "—",
                                                          renderEditor: ({
                                                              rowIndex,
                                                          }) => (
                                                              <div className="min-w-24">
                                                                  <form.AppField
                                                                      name={`lineItems[${rowIndex}].cardForm`}
                                                                  >
                                                                      {(
                                                                          field,
                                                                      ) => (
                                                                          <field.SelectField
                                                                              label="卡形态"
                                                                              hideLabel
                                                                              options={
                                                                                  CARD_FORM_OPTIONS
                                                                              }
                                                                              allowClear={
                                                                                  false
                                                                              }
                                                                              className="gap-0"
                                                                              inputClassName="w-full"
                                                                          />
                                                                      )}
                                                                  </form.AppField>
                                                              </div>
                                                          ),
                                                      },
                                                  ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])
                                                : ([
                                                      {
                                                          id: "fulfillment",
                                                          header: "交付日期",
                                                          renderValue: ({
                                                              item,
                                                          }) =>
                                                              item.dueDate ||
                                                              "—",
                                                          renderEditor: ({
                                                              rowIndex,
                                                          }) => (
                                                              <div className="min-w-32">
                                                                  <form.AppField
                                                                      name={`lineItems[${rowIndex}].dueDate`}
                                                                  >
                                                                      {(
                                                                          field,
                                                                      ) => (
                                                                          <field.DateField
                                                                              label="交付日期"
                                                                              hideLabel
                                                                          />
                                                                      )}
                                                                  </form.AppField>
                                                              </div>
                                                          ),
                                                      },
                                                  ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])),
                                            {
                                                id: "amount",
                                                header: "含税小计",
                                                numeric: true,
                                                align: "end",
                                                renderValue: ({ item }) => (
                                                    <MoneyValue
                                                        value={
                                                            calculateTotals(
                                                                [item],
                                                                values.taxRatePercent,
                                                            ).gross
                                                        }
                                                    />
                                                ),
                                            },
                                        ]

                                    return (
                                        <>
                                            <EditableLineItemTable
                                                items={values.lineItems}
                                                columns={columns}
                                                getRowId={(item) => item.rowKey}
                                                caption="销售单创建明细"
                                                emptyContent="至少需要一条销售明细。"
                                                addLabel="新增销售明细"
                                                addDisabledReason={
                                                    nature === "card_voucher"
                                                        ? "卡券销售单每个版本必须恰好一条明细"
                                                        : undefined
                                                }
                                                onAddItem={
                                                    nature ===
                                                    "physical_service"
                                                        ? () =>
                                                              form.pushFieldValue(
                                                                  "lineItems",
                                                                  createEmptyLine(
                                                                      nature,
                                                                  ),
                                                              )
                                                        : undefined
                                                }
                                                onRemoveItem={(
                                                    _item,
                                                    _rowId,
                                                    rowIndex,
                                                ) => {
                                                    void form.removeFieldValue(
                                                        "lineItems",
                                                        rowIndex,
                                                    )
                                                }}
                                                getRemoveDisabledReason={() =>
                                                    values.lineItems.length ===
                                                    1
                                                        ? "销售单至少保留一条明细"
                                                        : nature ===
                                                            "card_voucher"
                                                          ? "卡券销售单必须保留唯一明细"
                                                          : undefined
                                                }
                                            />

                                            {lineItemIssues.length > 0 ? (
                                                <ValidationSummary
                                                    className="mt-4"
                                                    issues={lineItemIssues}
                                                    title={`明细共 ${lineItemIssues.length} 项待处理`}
                                                />
                                            ) : null}
                                        </>
                                    )
                                }}
                            </form.Subscribe>

                            <div className="mt-5">
                                <form.AppField name="remark">
                                    {(field) => (
                                        <field.TextareaField
                                            label="内部说明"
                                            placeholder="补充客户确认、交付或内部协同说明（可选）"
                                            rows={2}
                                        />
                                    )}
                                </form.AppField>
                            </div>
                        </section>

                        {headerIssues.length > 0 ? (
                            <ValidationSummary
                                className="border-t border-border/30 px-4 pt-4 md:px-5 lg:px-6"
                                issues={headerIssues}
                                title={`单据头共 ${headerIssues.length} 项待处理`}
                            />
                        ) : null}

                        <form.Subscribe selector={(state) => state.values}>
                            {(values) => {
                                const totals = calculateTotals(
                                    values.lineItems,
                                    values.taxRatePercent,
                                )
                                const flowNote =
                                    purpose === "resubmit"
                                        ? "将按当前整单内容生成新一版，再交给采购确认。商品或价格需相对被驳回版本有改动。"
                                        : values.nature === "card_voucher"
                                          ? "提交后进入销售领导 → 运营两级审批，运营通过后生效并形成应收。"
                                          : "提交后内容锁定并进入采购二次确认；生效以确认通过为准。"
                                return (
                                    <StickyTotalBar
                                        className="rounded-none border-0 border-t border-border/30 px-4 py-4 shadow-none md:px-5 md:py-4"
                                        items={[
                                            {
                                                id: "gross",
                                                label: "含税金额",
                                                value: (
                                                    <MoneyValue
                                                        value={totals.gross}
                                                        taxBasis="gross"
                                                    />
                                                ),
                                            },
                                            {
                                                id: "net",
                                                label: "不含税金额",
                                                value: (
                                                    <MoneyValue
                                                        value={totals.net}
                                                        taxBasis="net"
                                                    />
                                                ),
                                            },
                                            {
                                                id: "tax",
                                                label: "税额",
                                                value: (
                                                    <MoneyValue
                                                        value={totals.tax}
                                                    />
                                                ),
                                            },
                                        ]}
                                        note={
                                            <>
                                                税率{" "}
                                                {values.taxRatePercent || "0"}%
                                                预估。{flowNote}
                                            </>
                                        }
                                        actions={
                                            <form.AppForm>
                                                <form.SubmitButton
                                                    variant="outline"
                                                    label="保存草稿"
                                                    pendingLabel="正在保存草稿…"
                                                    onClick={() => {
                                                        submitIntentRef.current =
                                                            "SAVE_DRAFT"
                                                    }}
                                                />
                                                <form.SubmitButton
                                                    label={
                                                        purpose === "resubmit"
                                                            ? "再报给采购"
                                                            : "提交"
                                                    }
                                                    pendingLabel={
                                                        purpose === "resubmit"
                                                            ? "正在准备重提…"
                                                            : "正在提交…"
                                                    }
                                                    onClick={() => {
                                                        submitIntentRef.current =
                                                            "SUBMIT"
                                                    }}
                                                >
                                                    {purpose === "resubmit" ? (
                                                        "再报给采购"
                                                    ) : (
                                                        <>
                                                            <PlusIcon
                                                                data-icon="inline-start"
                                                                aria-hidden="true"
                                                            />
                                                            提交
                                                        </>
                                                    )}
                                                </form.SubmitButton>
                                            </form.AppForm>
                                        }
                                    />
                                )
                            }}
                        </form.Subscribe>
                    </div>

                    <aside className="hidden xl:block">
                        <form.Subscribe selector={(state) => state.values}>
                            {(values) => {
                                const totals = calculateTotals(
                                    values.lineItems,
                                    values.taxRatePercent,
                                )
                                const natureLabel =
                                    values.nature === "card_voucher"
                                        ? "卡券"
                                        : "实物/服务"
                                const nextStep =
                                    values.nature === "card_voucher"
                                        ? "提交后进入销售领导 → 运营两级审批"
                                        : "提交后进入采购二次确认"
                                return (
                                    <div
                                        className={cn(
                                            surfacePanelClassName,
                                            "sticky top-14 space-y-4 p-4",
                                        )}
                                    >
                                        <div>
                                            <h2 className="font-heading text-sm font-semibold">
                                                本单摘要
                                            </h2>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                随填写实时更新
                                            </p>
                                        </div>
                                        <dl className="space-y-2.5 text-xs">
                                            <div className="flex justify-between gap-2">
                                                <dt className="text-muted-foreground">
                                                    合同
                                                </dt>
                                                <dd className="max-w-[10rem] truncate text-right font-medium">
                                                    {values.contractRevisionLabel ||
                                                        "未选择"}
                                                </dd>
                                            </div>
                                            <div className="flex justify-between gap-2">
                                                <dt className="text-muted-foreground">
                                                    客户
                                                </dt>
                                                <dd className="max-w-[10rem] truncate text-right font-medium">
                                                    {values.customerName || "—"}
                                                </dd>
                                            </div>
                                            <div className="flex justify-between gap-2">
                                                <dt className="text-muted-foreground">
                                                    结算
                                                </dt>
                                                <dd className="max-w-[10rem] truncate text-right font-medium">
                                                    {values.settlementEntity ||
                                                        "—"}
                                                </dd>
                                            </div>
                                            <div className="flex justify-between gap-2">
                                                <dt className="text-muted-foreground">
                                                    业务性质
                                                </dt>
                                                <dd className="font-medium">
                                                    {natureLabel}
                                                </dd>
                                            </div>
                                            <div className="flex justify-between gap-2">
                                                <dt className="text-muted-foreground">
                                                    明细行
                                                </dt>
                                                <dd className="font-medium">
                                                    {values.lineItems.length} 行
                                                </dd>
                                            </div>
                                            <div className="border-t border-border/30 pt-3">
                                                <div className="flex justify-between gap-2">
                                                    <dt className="text-muted-foreground">
                                                        含税预估
                                                    </dt>
                                                    <dd className="num font-semibold">
                                                        <MoneyValue
                                                            value={totals.gross}
                                                            taxBasis="gross"
                                                        />
                                                    </dd>
                                                </div>
                                                <div className="mt-2 flex justify-between gap-2">
                                                    <dt className="text-muted-foreground">
                                                        税额
                                                    </dt>
                                                    <dd className="num">
                                                        <MoneyValue
                                                            value={totals.tax}
                                                        />
                                                    </dd>
                                                </div>
                                            </div>
                                        </dl>
                                        <p
                                            className={cn(
                                                surfaceInsetClassName,
                                                "px-2.5 py-2 text-xs leading-relaxed text-muted-foreground",
                                            )}
                                        >
                                            {nextStep}
                                        </p>
                                    </div>
                                )
                            }}
                        </form.Subscribe>
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

            <FormalActionConfirmDialog
                open={resubmitConfirmOpen}
                onOpenChange={setResubmitConfirmOpen}
                title="再报给采购"
                description={
                    <div className="space-y-2 text-left">
                        <p>
                            请登记已上传的客户重新确认依据，再核对本次重提影响。
                        </p>
                        <label
                            className="block space-y-1"
                            htmlFor="resubmit-customer-evidence-ids"
                        >
                            <span className="text-xs font-medium text-foreground">
                                客户确认依据 ID
                            </span>
                            <Input
                                id="resubmit-customer-evidence-ids"
                                value={resubmitEvidence}
                                onChange={(event) =>
                                    setResubmitEvidence(event.target.value)
                                }
                                placeholder="粘贴已登记的文件 ID；多个以逗号分隔"
                                autoComplete="off"
                            />
                        </label>
                    </div>
                }
                actionLabel="重提"
                confirmLabel="确认再报"
                fromStatus={{ label: "采购未通过", tone: "warning" }}
                toStatus={{ label: "待二次确认", tone: "info" }}
                lockedFields={["销售单号", "业务性质", "被驳回的那一版"]}
                effects={[
                    "按当前整单内容生成新一版提交",
                    "采购会收到新的确认待办",
                    "以前被驳回的记录仍保留",
                ]}
                nextDepartment="采购"
                pending={resubmitMutation.isPending}
                onConfirm={async () => {
                    if (!draftIdentity) return
                    const evidenceIds =
                        parseEvidenceReferenceIds(resubmitEvidence)
                    if (evidenceIds.length === 0) {
                        throw new Error("请至少填写一项客户重新确认依据 ID")
                    }
                    let command =
                        commandLedger.peek<ResolveProcurementRejectionPayload>(
                            "procurement-rejection-resolution",
                        )
                    if (
                        command &&
                        command.payload.action !== "RESUBMIT_CHANGED_TERMS"
                    ) {
                        onResult?.({
                            status: "unknown",
                            title: "处理结果待确认",
                            description:
                                "另一项处理的结果仍待确认，请先使用原操作重试。",
                            reference: draftIdentity.documentNumber,
                        })
                        throw new Error(
                            "另一项处理的结果仍待确认，请先使用原操作重试。",
                        )
                    }
                    try {
                        if (!command) {
                            const payload =
                                await prepareProcurementRejectionResolution({
                                    salesOrderId: draftIdentity.salesOrderId,
                                    action: "RESUBMIT_CHANGED_TERMS",
                                    customerReconfirmationEvidenceIds:
                                        evidenceIds,
                                })
                            command = commandLedger.acquire(
                                "procurement-rejection-resolution",
                                `sales:${draftIdentity.salesOrderId}:procurement-resubmit`,
                                payload,
                            )
                        }
                        if (!command) return
                        const outcome = await resubmitMutation.mutateAsync({
                            ...command.payload,
                            idempotencyKey: command.idempotencyKey,
                        })
                        commandLedger.settle(
                            "procurement-rejection-resolution",
                            "succeeded",
                        )
                        onResult?.({
                            status: "succeeded",
                            title: "已改完并再报给采购",
                            description: outcome.detail,
                            reference: outcome.reference,
                            nextResponsible: "采购重新确认",
                        })
                        onSubmitted?.(draftIdentity.salesOrderId)
                    } catch (error) {
                        const settlement = command
                            ? classifyFormalCommandError(error)
                            : "failed"
                        commandLedger.settle(
                            "procurement-rejection-resolution",
                            settlement,
                        )
                        onResult?.({
                            status:
                                settlement === "unknown"
                                    ? "unknown"
                                    : "blocked",
                            title:
                                settlement === "unknown"
                                    ? "处理结果待确认"
                                    : "还不能再报给采购",
                            description: getErrorMessage(
                                error,
                                settlement === "unknown"
                                    ? "当前输入已保留，请使用本次操作重试。"
                                    : "请确认已改商品或价格后再试。",
                            ),
                            reference: draftIdentity.documentNumber,
                        })
                        throw error
                    }
                }}
            />
        </>
    )

    if (chrome === "none") return editor

    return (
        <PageScaffold className="pb-8">
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "sales", label: "销售", href: "/sales/orders" },
                    { id: "orders", label: "销售单", href: "/sales/orders" },
                    {
                        id: "create",
                        label: initialDraft
                            ? initialDraft.documentNumber
                            : "新建",
                        current: true,
                    },
                ]}
            />
            {editor}
        </PageScaffold>
    )
}
