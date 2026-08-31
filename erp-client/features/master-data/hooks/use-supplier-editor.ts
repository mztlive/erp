"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { useAppForm } from "@/components/form"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    buildResourceFields,
    defaultImmediateEffectiveFrom,
} from "@/features/master-data/lib/resource-fields"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
    useMasterDataCenterQuery,
} from "@/features/master-data/hooks/queries"
import { useSupplierMediaAssets } from "@/features/master-data/hooks/use-supplier-media-assets"
import type {
    MasterDataMutationResult,
    PendingAssetUpload,
    SupplierFields,
} from "@/features/master-data/types"
import {
    createSupplierEditorDefaults,
    hydrateSupplierEditor,
    validateSupplierEditorFields,
} from "@/features/master-data/lib/supplier-editor-model"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

function newIdempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

export function useSupplierEditor(stableId: string) {
    const router = useRouter()
    const isCreate = stableId === "new"
    const detailQuery = useMasterDataCenterQuery(
        "suppliers",
        isCreate ? "" : stableId,
    )
    const createMutation = useCreateMasterDataMutation()
    const reviseMutation = useCreateRevisionMutation()
    const accountQuery = useAccountProfileQuery()

    const data = detailQuery.data
    const lockVersion = data?.lockVersion
    const revisionId = data?.currentRevision.revisionId
    const [formError, setFormError] = React.useState<string | null>(null)
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey(isCreate ? "create-supplier" : "revise-supplier"),
    )
    const {
        rememberMediaFiles,
        mediaUrlsFor,
        mediaAssetIdsFor,
        preparePendingMedia,
    } = useSupplierMediaAssets(data)
    /** 已实际编辑过的敏感字段；用于区分“保留打码值”和“明确清空”。 */
    const editedSensitiveRef = React.useRef(
        new Set<"contactPhone" | "address" | "bankAccount">(),
    )
    const [disableOpen, setDisableOpen] = React.useState(false)
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const [saveReasonOpen, setSaveReasonOpen] = React.useState(false)
    const [reasonDraft, setReasonDraft] = React.useState("")
    const [reasonError, setReasonError] = React.useState<string | null>(null)
    const [pendingNav, setPendingNav] = React.useState<string | null>(null)
    const [activeSection, setActiveSection] = React.useState("basic")
    const errorRef = React.useRef<HTMLDivElement | null>(null)
    const hydratedKeyRef = React.useRef<string | null>(null)
    /** 弹窗确认的变更原因；保证 setFieldValue 与 handleSubmit 之间不丢值。 */
    const pendingChangeReasonRef = React.useRef<string | null>(null)

    const initialFormValues = React.useMemo(
        () =>
            !isCreate && data
                ? hydrateSupplierEditor(data)
                : createSupplierEditorDefaults(isCreate),
        [data, isCreate],
    )

    const form = useAppForm({
        defaultValues: initialFormValues,
        onSubmit: async ({ value }) => {
            setFormError(null)
            setResult(null)

            const hasStoredContactPhone = data?.sensitiveFields.some(
                (field) =>
                    field.label === "联系电话" || field.label === "联系人",
            )
            const validation = validateSupplierEditorFields(value, {
                hasStoredContactPhone,
                originalContactName: initialFormValues.contactName,
                hasStoredBankAccount: data?.sensitiveFields.some(
                    (field) => field.label === "银行账号",
                ),
                originalBankName: initialFormValues.bankName,
            })
            if (validation) {
                setFormError(validation)
                return
            }
            const changeReason = (
                pendingChangeReasonRef.current ?? value.changeReason
            ).trim()
            pendingChangeReasonRef.current = null
            if (changeReason.length < 2) {
                setFormError("请填写本次保存的变更原因")
                return
            }

            let fields = buildResourceFields("suppliers", value)
            let pendingAssetUploads: readonly PendingAssetUpload[] = []
            try {
                const preparedMedia = preparePendingMedia(value)
                const assetMaps = preparedMedia.assetMaps
                pendingAssetUploads = preparedMedia.pendingAssetUploads
                fields = {
                    ...fields,
                    clearContact:
                        !isCreate &&
                        !value.contactName.trim() &&
                        !value.contactPhone.trim() &&
                        (Boolean(initialFormValues.contactName.trim()) ||
                            editedSensitiveRef.current.has("contactPhone")),
                    clearAddress:
                        !isCreate &&
                        !value.address.trim() &&
                        editedSensitiveRef.current.has("address"),
                    clearTaxProfile:
                        !isCreate &&
                        Boolean(initialFormValues.taxNo.trim()) &&
                        !value.taxNo.trim(),
                    clearBankAccount:
                        !isCreate &&
                        !value.bankName.trim() &&
                        !value.bankAccount.trim() &&
                        Boolean(initialFormValues.bankName.trim()),
                    qualificationFileAssetIds: assetMaps.qualification,
                    contractFileAssetIds: assetMaps.contractFile,
                    authorizationFileAssetIds: assetMaps.authorizationFile,
                    foodLicenseFileAssetIds: assetMaps.foodLicense,
                    legalPersonIdCardFileAssetIds: assetMaps.legalPersonIdCard,
                    qualificationCapabilityCodes:
                        data?.supplierQualificationCapabilityCodes,
                } as SupplierFields
            } catch (error) {
                setFormError(
                    getErrorMessage(
                        error,
                        "资质文件上传失败，请检查文件后重试。",
                    ),
                )
                return
            }

            if (!isCreate) {
                if (!data || !revisionId || lockVersion == null) return
                const response = await reviseMutation.mutateAsync({
                    resource: "suppliers",
                    stableId: data.stableId,
                    baseRevisionId: revisionId,
                    expectedLockVersion: lockVersion,
                    expectedPartyVersion: data.partyLockVersion,
                    name: value.name.trim(),
                    effectiveFrom: defaultImmediateEffectiveFrom(),
                    changeReason,
                    fields,
                    idempotencyKey,
                    pendingAssetUploads,
                })
                if (response.outcome === "succeeded") {
                    toast.add({
                        title: masterDataCopy.reviseSuccessTitle,
                        description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
                        type: "success",
                        timeout: 4000,
                    })
                    setIdempotencyKey(newIdempotencyKey("revise-supplier"))
                    hydratedKeyRef.current = null
                    await detailQuery.refetch()
                    return
                }
                setResult(response)
                return
            }

            const response = await createMutation.mutateAsync({
                resource: "suppliers",
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason,
                fields,
                idempotencyKey,
                pendingAssetUploads,
            })
            if (response.outcome === "succeeded") {
                toast.add({
                    title: masterDataCopy.createSuccessTitle,
                    description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
                    type: "success",
                    timeout: 4000,
                })
                router.replace(`/master-data/suppliers/${response.stableId}`)
                return
            }
            setResult(response)
        },
    })

    React.useEffect(() => {
        if (isCreate || !data) return
        const key = `${data.stableId}:${data.lockVersion}:${data.currentRevision.revisionId}`
        if (hydratedKeyRef.current === key) return
        form.reset(hydrateSupplierEditor(data))
        editedSensitiveRef.current.clear()
        hydratedKeyRef.current = key
    }, [data, form, isCreate])

    // 离开确认：返回列表 / 侧栏 / 刷新都受未保存保护
    React.useEffect(() => {
        const onBeforeUnload = (event: BeforeUnloadEvent) => {
            if (form.state.isDirty) {
                event.preventDefault()
            }
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅挂载时注册一次
    }, [])

    // 校验错误出现时滚动到顶部错误条
    React.useEffect(() => {
        if (formError) {
            errorRef.current?.scrollIntoView({
                block: "center",
                behavior: "smooth",
            })
        }
    }, [formError])

    // 成功面板不滞留：表单再次变脏后清掉「已保存」结果（禁止在 Subscribe 渲染里 setState）
    React.useEffect(() => {
        const subscription = form.store.subscribe(() => {
            if (form.store.state.isDirty) {
                setResult((prev) => (prev ? null : prev))
            }
        })
        return () => subscription.unsubscribe()
    }, [form.store])

    const navigateAway = React.useCallback(
        (href: string) => {
            if (form.state.isDirty) {
                setPendingNav(href)
                setDiscardOpen(true)
                return
            }
            router.push(href)
        },
        [form.state.isDirty, router],
    )

    /** 敏感字段映射：label → 打码展示 + 揭示令牌 */
    const sensitiveByLabel = React.useMemo(() => {
        const map = new Map<
            string,
            { maskedValue: string; revealToken?: string }
        >()
        for (const field of data?.sensitiveFields ?? []) {
            map.set(field.label, {
                maskedValue: field.maskedValue,
                revealToken: field.revealToken,
            })
        }
        return map
    }, [data?.sensitiveFields])

    const listHref = "/master-data/suppliers"
    const pending = createMutation.isPending || reviseMutation.isPending
    const granted = accountQuery.data?.permissions
    const canCreate = hasPermission(granted, "supplier:create")
    const hasUpdatePermission = hasPermission(granted, "supplier:update")
    const hasDeletePermission = hasPermission(granted, "supplier:delete")
    const canRevealSensitive = hasPermission(
        granted,
        "supplier_sensitive:reveal",
    )
    const canRevise =
        !isCreate &&
        hasUpdatePermission &&
        (data?.allowedActions.includes("CREATE_REVISION") ?? false)
    const canDisable =
        hasDeletePermission &&
        (data?.allowedActions.includes("DISABLE") ?? false)
    const reviseBlocker = data?.actionBlockers.find(
        (b) => b.action === "CREATE_REVISION",
    )
    const disableBlocker = data?.actionBlockers.find(
        (b) => b.action === "DISABLE",
    )

    return {
        isCreate,
        router,
        detailQuery,
        data,
        form,
        formError,
        setFormError,
        result,
        setResult,
        disableOpen,
        setDisableOpen,
        discardOpen,
        setDiscardOpen,
        saveReasonOpen,
        setSaveReasonOpen,
        reasonDraft,
        setReasonDraft,
        reasonError,
        setReasonError,
        pendingNav,
        setPendingNav,
        activeSection,
        setActiveSection,
        errorRef,
        editedSensitiveRef,
        pendingChangeReasonRef,
        rememberMediaFiles,
        mediaUrlsFor,
        mediaAssetIdsFor,
        initialFormValues,
        navigateAway,
        sensitiveByLabel,
        listHref,
        pending,
        canCreate,
        canRevise,
        canDisable,
        canRevealSensitive,
        reviseBlocker,
        disableBlocker,
    }
}

export type SupplierEditor = ReturnType<typeof useSupplierEditor>
