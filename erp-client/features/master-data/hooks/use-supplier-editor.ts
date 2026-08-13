"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { useAppForm } from "@/components/form"
import { toast } from "@/components/ui/toast"
import { uploadFileAssetImage } from "@/features/file-assets/api"
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
import type {
    MasterDataMutationResult,
    SupplierFields,
} from "@/features/master-data/types"
import {
    createSupplierEditorDefaults,
    hydrateSupplierEditor,
    validateSupplierEditorFields,
    type SupplierEditorFormValues,
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
    /** 已登记资质附件：字段 key → fileName → { assetId, url }（回显链接 + 再次保存不重复上传）。 */
    const mediaAssetMaps = React.useMemo(() => {
        const maps: Record<
            string,
            Record<string, { assetId: string; url: string }>
        > = {}
        for (const [key, entries] of Object.entries(data?.mediaAssets ?? {})) {
            const map: Record<string, { assetId: string; url: string }> = {}
            for (const entry of entries) {
                map[entry.fileName] = { assetId: entry.assetId, url: entry.url }
            }
            maps[key] = map
        }
        return maps
    }, [data])
    /** 本会话选择但尚未上传的资质文件；保存时按文件名上传并回填 asset id。 */
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    /** 保存失败后保留本会话已上传资产，重试时不重复上传。 */
    const uploadedAssetMapsRef = React.useRef<
        Record<string, Record<string, { assetId: string; url: string }>>
    >({})
    /** 已实际编辑过的敏感字段；用于区分“保留打码值”和“明确清空”。 */
    const editedSensitiveRef = React.useRef(
        new Set<"contactPhone" | "address" | "bankAccount">(),
    )
    const rememberMediaFiles = React.useCallback((files: File[]) => {
        for (const file of files) {
            pendingFilesRef.current.set(file.name, file)
        }
    }, [])
    const mediaUrlsFor = React.useCallback(
        (fieldKey: string): Readonly<Record<string, string>> => {
            const entries = mediaAssetMaps[fieldKey] ?? {}
            return Object.fromEntries(
                Object.entries(entries).map(([name, info]) => [name, info.url]),
            )
        },
        [mediaAssetMaps],
    )
    const mediaAssetIdsFor = React.useCallback(
        (fieldKey: string): Readonly<Record<string, string>> => {
            const entries = mediaAssetMaps[fieldKey] ?? {}
            return Object.fromEntries(
                Object.entries(entries).map(([name, info]) => [
                    name,
                    info.assetId,
                ]),
            )
        },
        [mediaAssetMaps],
    )
    /** 上传仍为本地待传的资质文件，返回 fileName → asset id 映射（按字段）。 */
    const resolvePendingMedia = React.useCallback(
        async (
            values: SupplierEditorFormValues,
        ): Promise<Record<string, Record<string, string>>> => {
            const mediaFields = [
                "qualification",
                "contractFile",
                "authorizationFile",
                "foodLicense",
                "legalPersonIdCard",
            ] as const
            const out: Record<string, Record<string, string>> = {}
            for (const key of mediaFields) {
                const names = (values[key] ?? "")
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean)
                const existing = mediaAssetMaps[key] ?? {}
                const uploadedInSession =
                    uploadedAssetMapsRef.current[key] ?? {}
                const map: Record<string, string> = {}
                for (const name of names) {
                    const known = existing[name] ?? uploadedInSession[name]
                    if (known?.assetId) {
                        map[name] = known.assetId
                        continue
                    }
                    const file = pendingFilesRef.current.get(name)
                    if (!file) continue
                    const sensitivityClass =
                        key === "legalPersonIdCard"
                            ? "highly_sensitive"
                            : "sensitive"
                    const uploaded = await uploadFileAssetImage(
                        file,
                        "attachment",
                        sensitivityClass,
                    )
                    map[name] = uploaded.fileAssetId
                    uploadedAssetMapsRef.current[key] = {
                        ...uploadedAssetMapsRef.current[key],
                        [name]: {
                            assetId: uploaded.fileAssetId,
                            url: uploaded.url,
                        },
                    }
                }
                out[key] = map
            }
            return out
        },
        [mediaAssetMaps],
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
            try {
                const assetMaps = await resolvePendingMedia(value)
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
                setFormError(getErrorMessage(error, "资质文件上传失败"))
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
