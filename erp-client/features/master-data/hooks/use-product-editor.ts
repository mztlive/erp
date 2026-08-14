"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { useAppForm } from "@/components/form"
import { toast } from "@/components/ui/toast"
import {
    applySpecsFromDrafts,
    createProductDefaults,
    hydrateFromCenter,
    newIdempotencyKey,
    type ProductEditorFormValues,
    validateProductEditor,
} from "@/features/master-data/lib/product-editor-model"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    toBrandComboboxItems,
    toCategoryComboboxItems,
} from "@/features/master-data/lib/category-tree-model"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
    useMasterDataCenterQuery,
    useMasterDataListQuery,
    useSkuSupplierCountsQuery,
} from "@/features/master-data/hooks/queries"
import { useProductDirtyGuard } from "@/features/master-data/hooks/use-product-dirty-guard"
import { useProductSectionSpy } from "@/features/master-data/hooks/use-product-section-spy"
import { useProductStickyHeader } from "@/features/master-data/hooks/use-product-sticky-header"
import { useProductUploads } from "@/features/master-data/hooks/use-product-uploads"
import type { MasterDataMutationResult } from "@/features/master-data/types"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { useUnitOptionsQuery } from "@/hooks/use-options"
import type { FixedSku } from "@/features/supplier-offerings/types"

export function useProductEditor(stableId: string) {
    const router = useRouter()
    const isCreate = stableId === "new"
    const accountQuery = useAccountProfileQuery()
    const detailQuery = useMasterDataCenterQuery(
        "products",
        isCreate ? "" : stableId,
    )
    const categoryListQuery = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "enabled",
        revisionTiming: "current",
    })
    const brandListQuery = useMasterDataListQuery({
        resource: "brands",
        lifecycleStatus: "enabled",
        revisionTiming: "current",
    })
    const categoryOptions = React.useMemo(
        () => toCategoryComboboxItems(categoryListQuery.data?.rows ?? []),
        [categoryListQuery.data?.rows],
    )
    const brandOptions = React.useMemo(
        () => toBrandComboboxItems(brandListQuery.data?.rows ?? []),
        [brandListQuery.data?.rows],
    )
    const unitOptionsQuery = useUnitOptionsQuery()
    const createMutation = useCreateMasterDataMutation()
    const reviseMutation = useCreateRevisionMutation()

    const data = detailQuery.data
    const supplierCountsQuery = useSkuSupplierCountsQuery(
        data?.productDetail?.skus.flatMap((sku) =>
            sku.skuId ? [sku.skuId] : [],
        ) ?? [],
    )
    const lockVersion = data?.lockVersion
    const revisionId = data?.currentRevision.revisionId
    const [formError, setFormError] = React.useState<string | null>(null)
    const [formErrorTitle, setFormErrorTitle] = React.useState("填写检查未通过")
    const [checkPassed, setCheckPassed] = React.useState(false)
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey(isCreate ? "create-product" : "revise-product"),
    )
    const [disableOpen, setDisableOpen] = React.useState(false)
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const [pendingNav, setPendingNav] = React.useState<string | null>(null)
    const [supplierDialogSku, setSupplierDialogSku] = React.useState<FixedSku>()
    const [inventoryOpen, setInventoryOpen] = React.useState(false)
    const [inventoryInitialSkuId, setInventoryInitialSkuId] =
        React.useState<string>()
    const errorRef = React.useRef<HTMLDivElement | null>(null)
    const checkedSnapshotRef = React.useRef<string | null>(null)
    const hydratedKeyRef = React.useRef<string | null>(null)
    const inventoryTriggerRef = React.useRef<HTMLButtonElement | null>(null)

    const {
        uploadingMedia,
        setUploadingMedia,
        rememberPendingFiles,
        rememberSkuFile,
        resolvePendingUploads,
    } = useProductUploads()
    const { activeSection, setActiveSection } = useProductSectionSpy(
        isCreate,
        data?.stableId,
    )
    const { stickyHeaderRef, stickyHeaderHeight, sectionScrollMarginPx } =
        useProductStickyHeader(isCreate, data?.stableId, data?.lockVersion)
    useProductDirtyGuard(() => form.state.isDirty)

    const initialFormValues = React.useMemo(
        () =>
            !isCreate && data
                ? hydrateFromCenter(data)
                : createProductDefaults(isCreate),
        [data, isCreate],
    )

    const form = useAppForm({
        defaultValues: initialFormValues,
        onSubmit: async ({ value }) => {
            setFormError(null)
            setCheckPassed(false)
            setResult(null)

            const nextFields = applySpecsFromDrafts(
                value.specDrafts,
                value.fields,
                value.name,
            )
            const validation = validateProductEditor(value, nextFields)
            if (validation) {
                setFormErrorTitle("填写检查未通过")
                setFormError(validation)
                return
            }

            try {
                // 先把仍为本地 blob 的图片上传为文件资产，再携带真实 URL/asset id 保存
                setUploadingMedia(true)
                const resolvedFields = await resolvePendingUploads(nextFields)
                if (!isCreate) {
                    if (!data || !revisionId || lockVersion == null) return
                    const response = await reviseMutation.mutateAsync({
                        resource: "products",
                        stableId: data.stableId,
                        baseRevisionId: revisionId,
                        expectedLockVersion: lockVersion,
                        name: value.name.trim(),
                        effectiveFrom: value.effectiveFrom,
                        effectiveTo: value.effectiveTo.trim() || undefined,
                        changeReason: value.changeReason.trim(),
                        fields: resolvedFields,
                        idempotencyKey,
                    })
                    if (response.outcome === "succeeded") {
                        toast.add({
                            title: masterDataCopy.reviseSuccessTitle,
                            description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
                            type: "success",
                            timeout: 4000,
                        })
                        setIdempotencyKey(newIdempotencyKey("revise-product"))
                        hydratedKeyRef.current = null
                        await detailQuery.refetch()
                        return
                    }
                    setResult(response)
                    return
                }

                const response = await createMutation.mutateAsync({
                    resource: "products",
                    name: value.name.trim(),
                    effectiveFrom: value.effectiveFrom,
                    effectiveTo: value.effectiveTo.trim() || undefined,
                    changeReason: value.changeReason.trim(),
                    fields: resolvedFields,
                    idempotencyKey,
                })
                if (response.outcome === "succeeded") {
                    toast.add({
                        title: masterDataCopy.createSuccessTitle,
                        description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
                        type: "success",
                        timeout: 4000,
                    })
                    router.replace(`/master-data/products/${response.stableId}`)
                    return
                }
                setResult(response)
            } catch (error) {
                const failure = getErrorPresentation(
                    error,
                    "保存失败，请稍后重试。",
                )
                setFormErrorTitle(failure.title)
                setFormError(failure.description)
            } finally {
                setUploadingMedia(false)
            }
        },
    })

    React.useEffect(() => {
        if (isCreate || !data) return
        const key = `${data.stableId}:${data.lockVersion}:${data.currentRevision.revisionId}`
        if (hydratedKeyRef.current === key) return
        form.reset(hydrateFromCenter(data))
        hydratedKeyRef.current = key
    }, [data, form, isCreate])

    // 校验错误出现时滚动到错误条（P2-15）
    React.useEffect(() => {
        if (formError) {
            errorRef.current?.scrollIntoView({
                block: "center",
                behavior: "smooth",
            })
        }
    }, [formError])

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

    const openInventoryPreview = React.useCallback(
        (skuId: string | undefined, trigger: HTMLButtonElement) => {
            inventoryTriggerRef.current = trigger
            setInventoryInitialSkuId(skuId)
            setInventoryOpen(true)
        },
        [],
    )

    const handleInventoryOpenChange = React.useCallback((open: boolean) => {
        setInventoryOpen(open)
        if (!open) {
            globalThis.requestAnimationFrame(() =>
                inventoryTriggerRef.current?.focus(),
            )
        }
    }, [])

    const listHref = "/master-data/products"
    const pending =
        createMutation.isPending || reviseMutation.isPending || uploadingMedia
    const granted = accountQuery.data?.permissions
    const canCreate = hasPermission(granted, "product:create")
    const hasUpdatePermission = hasPermission(granted, "product:update")
    const canRevise = isCreate
        ? canCreate
        : hasUpdatePermission &&
          (data?.allowedActions.includes("CREATE_REVISION") ?? false)
    const canDisable =
        hasUpdatePermission &&
        (data?.allowedActions.includes("DISABLE") ?? false)
    const reviseBlocker = data?.actionBlockers.find(
        (b) => b.action === "CREATE_REVISION",
    )
    const disableBlocker = data?.actionBlockers.find(
        (b) => b.action === "DISABLE",
    )

    const runLocalCheck = (values: ProductEditorFormValues) => {
        setFormError(null)
        setCheckPassed(false)
        setResult(null)
        const nextFields = applySpecsFromDrafts(
            values.specDrafts,
            values.fields,
            values.name,
        )
        form.setFieldValue("fields", nextFields)
        const validation = validateProductEditor(values, nextFields)
        if (validation) {
            setFormErrorTitle("填写检查未通过")
            setFormError(validation)
            return
        }
        // 记录检查通过时的内容快照；后续任何字段变更都会让「通过」态失效
        checkedSnapshotRef.current = JSON.stringify({
            ...values,
            fields: nextFields,
        })
        setCheckPassed(true)
    }

    return {
        isCreate,
        router,
        accountQuery,
        detailQuery,
        categoryListQuery,
        brandListQuery,
        categoryOptions,
        brandOptions,
        unitOptionsQuery,
        data,
        supplierCountsQuery,
        form,
        formError,
        formErrorTitle,
        checkPassed,
        setCheckPassed,
        result,
        setResult,
        disableOpen,
        setDisableOpen,
        discardOpen,
        setDiscardOpen,
        pendingNav,
        setPendingNav,
        supplierDialogSku,
        setSupplierDialogSku,
        inventoryOpen,
        inventoryInitialSkuId,
        activeSection,
        setActiveSection,
        errorRef,
        checkedSnapshotRef,
        stickyHeaderRef,
        stickyHeaderHeight,
        rememberPendingFiles,
        rememberSkuFile,
        navigateAway,
        openInventoryPreview,
        handleInventoryOpenChange,
        listHref,
        sectionScrollMarginPx,
        pending,
        canCreate,
        hasUpdatePermission,
        canRevise,
        canDisable,
        reviseBlocker,
        disableBlocker,
        runLocalCheck,
        setFormError,
        setFormErrorTitle,
    }
}

export type ProductEditor = ReturnType<typeof useProductEditor>
