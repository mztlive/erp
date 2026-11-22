"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    SUPPLIER_PORTAL_SUB_TABS,
    type SupplierPortalSubTabId,
    type SupplierPrimarySectionId,
} from "@/features/master-data/components/supplier/supplier-editor-fields"

import { useAppForm, applyApiFormError } from "@/components/form"
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
import { newIdempotencyKey } from "@/lib/idempotency-key"
import {
    createSupplierEditorDefaults,
    hydrateSupplierEditor,
    supplierSectionForValidationError,
    type SupplierProfileSectionKey,
    type SupplierSensitiveFieldKey,
    validateSupplierEditorFields,
} from "@/features/master-data/lib/supplier-editor-model"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

const supplierFieldAliases = {
    legal_name: "company",
    "contact.contact_name": "contactName",
    "contact.mobile": "contactPhone",
    "address.address": "address",
    "bank_account.bank_name": "bankName",
    "bank_account.account_number": "bankAccount",
}

const PROFILE_LEGACY_SECTIONS = new Set<string>([
    "profile",
    "basic",
    "commercial",
    "contract",
    "invoice",
])

function resolvePrimarySectionParam(
    raw: string | null,
): SupplierPrimarySectionId {
    if (!raw || PROFILE_LEGACY_SECTIONS.has(raw)) return "profile"
    if (raw === "delivery") return "delivery"
    if (
        raw === "portal" ||
        raw === "portal-accounts" ||
        raw === "portal-catalog" ||
        raw === "portal-applications"
    ) {
        return "portal"
    }
    if (raw === "history") return "history"
    return "profile"
}

function resolvePortalSubTabParam(raw: string | null): SupplierPortalSubTabId {
    if (
        raw === "portal-accounts" ||
        raw === "portal-catalog" ||
        raw === "portal-applications"
    ) {
        return raw
    }
    return "portal-accounts"
}

function resolveLegacyAnchorParam(
    raw: string | null,
): SupplierProfileSectionKey | null {
    if (raw === "commercial" || raw === "contract" || raw === "invoice") {
        return raw
    }
    return null
}

function replaceSectionUrl(section: string | null) {
    const url = new URL(window.location.href)
    if (!section || section === "profile") url.searchParams.delete("section")
    else url.searchParams.set("section", section)
    window.history.replaceState(
        window.history.state,
        "",
        `${url.pathname}${url.search}${url.hash}`,
    )
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
    const granted = accountQuery.isSuccess
        ? accountQuery.data.permissions
        : undefined
    const currentUserId = accountQuery.data?.userid ?? ""

    const data = detailQuery.data
    const lockVersion = data?.lockVersion
    const revisionId = data?.currentRevision.revisionId
    const [formError, setFormError] = React.useState<string | null>(null)
    const [formErrorTitle, setFormErrorTitle] = React.useState("填写不完整")
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
    /** 已实际编辑过的敏感字段；用于区分“保留打码值”和“明确清空”，并实时驱动脏计数与回显。 */
    const editedSensitiveRef = React.useRef(
        new Set<SupplierSensitiveFieldKey>(),
    )
    const [editedSensitive, setEditedSensitive] = React.useState<
        ReadonlySet<SupplierSensitiveFieldKey>
    >(() => new Set())
    const markSensitiveEdited = React.useCallback(
        (field: SupplierSensitiveFieldKey) => {
            if (editedSensitiveRef.current.has(field)) return
            editedSensitiveRef.current.add(field)
            setEditedSensitive(new Set(editedSensitiveRef.current))
        },
        [],
    )
    const clearEditedSensitive = React.useCallback(() => {
        if (editedSensitiveRef.current.size === 0) return
        editedSensitiveRef.current.clear()
        setEditedSensitive(new Set())
    }, [])

    const [disableOpen, setDisableOpen] = React.useState(false)
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const [saveReasonOpen, setSaveReasonOpen] = React.useState(false)
    const [reasonDraft, setReasonDraft] = React.useState("")
    const [reasonError, setReasonError] = React.useState<string | null>(null)
    const [pendingNav, setPendingNav] = React.useState<string | null>(null)
    const searchParams = useSearchParams()
    const requestedSection = searchParams.get("section")
    const canReadDeliveryTemplates = hasPermission(
        granted,
        "supplier_offering:list",
    )
    const canEditDeliveryTemplates = hasPermission(
        granted,
        "supplier_offering:update",
    )
    const canReadPortalAccounts = hasPermission(
        granted,
        "supplier_portal_account:list",
    )
    const canReadPortalCatalog = hasPermission(
        granted,
        "supplier_portal_catalog:list",
    )
    const canReadPortalApplications = hasPermission(
        granted,
        "supplier_portal_request:list",
    )
    const allowedPortalSubTabs = React.useMemo(
        () =>
            SUPPLIER_PORTAL_SUB_TABS.filter((item) => {
                if (item.id === "portal-accounts") return canReadPortalAccounts
                if (item.id === "portal-catalog") return canReadPortalCatalog
                if (item.id === "portal-applications")
                    return canReadPortalApplications
                return false
            }),
        [
            canReadPortalAccounts,
            canReadPortalCatalog,
            canReadPortalApplications,
        ],
    )
    const canReadPortal = allowedPortalSubTabs.length > 0

    const [sectionState, setSectionState] =
        React.useState<SupplierPrimarySectionId>(() =>
            resolvePrimarySectionParam(requestedSection),
        )
    const [portalSubTabState, setPortalSubTabState] =
        React.useState<SupplierPortalSubTabId>(() =>
            resolvePortalSubTabParam(requestedSection),
        )
    const [pendingAnchor, setPendingAnchor] =
        React.useState<SupplierProfileSectionKey | null>(() =>
            resolveLegacyAnchorParam(requestedSection),
        )

    React.useEffect(() => {
        setSectionState(resolvePrimarySectionParam(requestedSection))
        const subTab = resolvePortalSubTabParam(requestedSection)
        if (
            requestedSection === "portal-accounts" ||
            requestedSection === "portal-catalog" ||
            requestedSection === "portal-applications"
        ) {
            setPortalSubTabState(subTab)
        }
        const anchor = resolveLegacyAnchorParam(requestedSection)
        if (anchor) setPendingAnchor(anchor)
    }, [requestedSection])

    const portalSubTab: SupplierPortalSubTabId = allowedPortalSubTabs.some(
        (item) => item.id === portalSubTabState,
    )
        ? portalSubTabState
        : (allowedPortalSubTabs[0]?.id ?? "portal-accounts")

    const activeSection: SupplierPrimarySectionId = React.useMemo(() => {
        if (isCreate) return "profile"
        if (sectionState === "delivery" && !canReadDeliveryTemplates) {
            return "profile"
        }
        if (sectionState === "portal" && !canReadPortal) {
            return "profile"
        }
        return sectionState
    }, [canReadDeliveryTemplates, canReadPortal, isCreate, sectionState])

    React.useEffect(() => {
        if (!pendingAnchor || activeSection !== "profile") return
        const targetAnchor = pendingAnchor
        const frame = window.requestAnimationFrame(() => {
            document
                .getElementById(`supplier-section-${targetAnchor}`)
                ?.scrollIntoView({ block: "start", behavior: "smooth" })
            setPendingAnchor(null)
        })
        return () => window.cancelAnimationFrame(frame)
    }, [activeSection, data?.stableId, pendingAnchor])

    const setActiveSection = React.useCallback(
        (next: SupplierPrimarySectionId) => {
            setSectionState(next)
            if (next === "portal") {
                replaceSectionUrl(portalSubTab)
                return
            }
            replaceSectionUrl(next === "profile" ? null : next)
        },
        [portalSubTab],
    )

    const setPortalSubTab = React.useCallback(
        (next: SupplierPortalSubTabId) => {
            setSectionState("portal")
            setPortalSubTabState(next)
            replaceSectionUrl(next)
        },
        [],
    )

    const focusProfileSection = React.useCallback(
        (anchor: SupplierProfileSectionKey) => {
            setSectionState("profile")
            replaceSectionUrl(null)
            setPendingAnchor(anchor)
        },
        [],
    )

    const errorRef = React.useRef<HTMLDivElement | null>(null)
    const hydratedKeyRef = React.useRef<string | null>(null)
    /** 弹窗确认的变更原因；保证 setFieldValue 与 handleSubmit 之间不丢值。 */
    const pendingChangeReasonRef = React.useRef<string | null>(null)
    /** 由表单层同步的真实未保存态，供离开拦截使用。 */
    const dirtyRef = React.useRef(false)

    const initialFormValues = React.useMemo(() => {
        if (!isCreate && data) return hydrateSupplierEditor(data)
        const defaults = createSupplierEditorDefaults(isCreate)
        return {
            ...defaults,
            maintainerUserId: isCreate ? currentUserId : "",
            capabilityOwnerUserId: isCreate ? currentUserId : "",
        }
    }, [currentUserId, data, isCreate])

    const form = useAppForm({
        defaultValues: initialFormValues,
        onSubmit: async ({ value }) => {
            setFormErrorTitle("填写不完整")
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
                setSaveReasonOpen(false)
                setFormError(validation)
                focusProfileSection(
                    supplierSectionForValidationError(validation),
                )
                return
            }
            const changeReason = (
                pendingChangeReasonRef.current ?? value.changeReason
            ).trim()
            pendingChangeReasonRef.current = null
            if (changeReason.length < 2) {
                setReasonError("请填写本次保存的变更原因")
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
                applyApiFormError(form, error, supplierFieldAliases)
                const failure = getErrorPresentation(
                    error,
                    "资质文件上传失败，请检查文件后重试。",
                )
                setFormErrorTitle(failure.title)
                setFormError(failure.description)
                return
            }

            try {
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
                        setSaveReasonOpen(false)
                        setReasonError(null)
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
                    if (response.error)
                        applyApiFormError(
                            form,
                            response.error,
                            supplierFieldAliases,
                        )
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
                    setSaveReasonOpen(false)
                    setReasonError(null)
                    toast.add({
                        title: masterDataCopy.createSuccessTitle,
                        description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
                        type: "success",
                        timeout: 4000,
                    })
                    router.replace(
                        `/master-data/suppliers/${response.stableId}`,
                    )
                    return
                }
                setResult(response)
                if (response.error)
                    applyApiFormError(
                        form,
                        response.error,
                        supplierFieldAliases,
                    )
            } catch (error) {
                const failure = getErrorPresentation(
                    error,
                    "保存失败，请稍后重试。",
                )
                setFormErrorTitle(failure.title)
                setFormError(failure.description)
            }
        },
    })

    React.useEffect(() => {
        if (isCreate || !data) return
        const key = `${data.stableId}:${data.lockVersion}:${data.currentRevision.revisionId}`
        if (hydratedKeyRef.current === key) return
        form.reset(hydrateSupplierEditor(data))
        clearEditedSensitive()
        hydratedKeyRef.current = key
    }, [clearEditedSensitive, data, form, isCreate])

    // 账号晚于用户录入加载时，只补内部责任字段，不重置已填写的资料。
    React.useEffect(() => {
        if (!currentUserId) return
        if (isCreate) {
            form.setFieldValue("maintainerUserId", currentUserId, {
                dontUpdateMeta: true,
            })
        }
        // 已有能力保持原负责人；首次补录能力默认由当前用户负责。
        if (isCreate || (data && !initialFormValues.capabilityOwnerUserId)) {
            form.setFieldValue("capabilityOwnerUserId", currentUserId, {
                dontUpdateMeta: true,
            })
        }
    }, [
        currentUserId,
        data,
        form,
        initialFormValues.capabilityOwnerUserId,
        isCreate,
    ])

    // 校验错误出现时滚动到顶部错误条
    React.useEffect(() => {
        if (formError && !saveReasonOpen) {
            errorRef.current?.scrollIntoView({
                block: "center",
                behavior: "smooth",
            })
        }
    }, [formError, saveReasonOpen])

    const navigateAway = React.useCallback(
        (href: string) => {
            if (dirtyRef.current) {
                setPendingNav(href)
                setDiscardOpen(true)
                return
            }
            router.push(href)
        },
        [router],
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
        accountQuery,
        router,
        detailQuery,
        data,
        form,
        formError,
        formErrorTitle,
        setFormError,
        setFormErrorTitle,
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
        portalSubTab,
        setPortalSubTab,
        allowedPortalSubTabs,
        canReadPortal,
        focusProfileSection,
        errorRef,
        editedSensitive,
        editedSensitiveRef,
        markSensitiveEdited,
        clearEditedSensitive,
        pendingChangeReasonRef,
        dirtyRef,
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
        canReadDeliveryTemplates,
        canEditDeliveryTemplates,
        canReadPortalAccounts,
        canReadPortalCatalog,
        canReadPortalApplications,
        reviseBlocker,
        disableBlocker,
    }
}

export type SupplierEditor = ReturnType<typeof useSupplierEditor>
