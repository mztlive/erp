"use client"

/**
 * 商品详情页 = 查看 + 编辑（同一页面）。
 * - /master-data/products/new  新建
 * - /master-data/products/:id  查看并直接改，保存即形成新版本
 * 不使用侧边 sheet，也不再有单独的 ?mode=edit。
 */

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import {
    BanIcon,
    CheckCircle2Icon,
    CircleAlertIcon,
    ClipboardCheckIcon,
    SaveIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DiscardConfirmDialog,
    FormalActionResult,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { MasterDataDisableDialog } from "@/features/master-data/master-data-action-dialog"
import {
    applySpecsFromDrafts,
    createProductDefaults,
    hydrateFromCenter,
    newIdempotencyKey,
    PRODUCT_EDITOR_SECTIONS,
    scrollToProductSection,
    type ProductEditorFormValues,
    type ProductEditorSectionId,
    type ProductSpecDraft,
    validateProductEditor,
} from "@/features/master-data/product-editor-model"
import {
    ProductBasicSection,
    ProductEffectiveSection,
    ProductHistorySection,
    ProductMediaSection,
} from "@/features/master-data/product-editor-sections"
import { ProductSkuSection } from "@/features/master-data/product-sku-section"
import {
    ProductInventoryPreviewSheet,
    type ProductInventoryPreviewSku,
} from "@/features/master-data/product-inventory-preview-sheet"
import {
    RegisterSupplyForSkuDialog,
    type FixedSku,
} from "@/features/supplier-offerings/offering-dialogs"
import { uploadFileAssetImage } from "@/features/file-assets/api"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { masterDataCopy } from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import {
    toBrandComboboxItems,
    toCategoryComboboxItems,
} from "@/features/master-data/category-tree-model"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
    useMasterDataCenterQuery,
    useMasterDataListQuery,
    useSkuSupplierCountsQuery,
} from "@/features/master-data/queries"
import type {
    MasterDataMutationResult,
    ProductFields,
    ProductSkuFields,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { useUnitOptionsQuery } from "@/hooks/use-options"

export function ProductDetailPage({ stableId }: { stableId: string }) {
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
    const [activeSection, setActiveSection] =
        React.useState<ProductEditorSectionId>("basic")
    const errorRef = React.useRef<HTMLDivElement | null>(null)
    const checkedSnapshotRef = React.useRef<string | null>(null)
    const stickyHeaderRef = React.useRef<HTMLElement>(null)
    const [stickyHeaderHeight, setStickyHeaderHeight] = React.useState(160)
    const hydratedKeyRef = React.useRef<string | null>(null)
    const [uploadingMedia, setUploadingMedia] = React.useState(false)
    /** 本会话选择但尚未上传的图片文件；保存时按 fileName / SKU 行号上传并回填。 */
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    const pendingSkuFilesRef = React.useRef<Map<number, File>>(new Map())
    const inventoryTriggerRef = React.useRef<HTMLButtonElement | null>(null)
    const rememberPendingFiles = React.useCallback((files: File[]) => {
        for (const file of files) {
            pendingFilesRef.current.set(file.name, file)
        }
    }, [])
    const rememberSkuFile = React.useCallback((index: number, file?: File) => {
        if (file) pendingSkuFilesRef.current.set(index, file)
    }, [])

    /** 把仍是本地 blob 预览的图片上传为文件资产，返回回填后的字段。 */
    const resolvePendingUploads = React.useCallback(
        async (current: ProductFields): Promise<ProductFields> => {
            const uploadIfPending = async (
                fileName: string,
                previewUrl: string | undefined,
                knownAssetId: string | undefined,
            ): Promise<{ url: string; assetId?: string } | null> => {
                const url = previewUrl?.trim()
                if (!url) return null
                if (url.startsWith("blob:")) {
                    const file = pendingFilesRef.current.get(fileName)
                    if (!file) {
                        throw new Error(
                            `找不到待上传图片「${fileName}」的文件内容，请重新选择`,
                        )
                    }
                    const uploaded = await uploadFileAssetImage(file)
                    return { url: uploaded.url, assetId: uploaded.fileAssetId }
                }
                return {
                    url,
                    ...(knownAssetId?.trim() ? { assetId: knownAssetId } : {}),
                }
            }

            const carouselPreviewUrls: Record<string, string> = {}
            const carouselFileAssetIds: Record<string, string> = {}
            for (const fileName of current.carouselImages) {
                const resolved = await uploadIfPending(
                    fileName,
                    current.carouselPreviewUrls[fileName],
                    current.carouselFileAssetIds[fileName],
                )
                if (resolved) {
                    carouselPreviewUrls[fileName] = resolved.url
                    if (resolved.assetId)
                        carouselFileAssetIds[fileName] = resolved.assetId
                }
            }
            const detailPreviewUrls: Record<string, string> = {}
            const detailFileAssetIds: Record<string, string> = {}
            for (const fileName of current.detailImages) {
                const resolved = await uploadIfPending(
                    fileName,
                    current.detailPreviewUrls[fileName],
                    current.detailFileAssetIds[fileName],
                )
                if (resolved) {
                    detailPreviewUrls[fileName] = resolved.url
                    if (resolved.assetId)
                        detailFileAssetIds[fileName] = resolved.assetId
                }
            }
            const skus = [...current.skus]
            for (let index = 0; index < skus.length; index++) {
                const sku = skus[index]
                if (!sku.mainImage) continue
                const previewUrl = sku.mainImagePreviewUrl?.trim()
                if (!previewUrl) continue
                if (!previewUrl.startsWith("blob:")) continue
                const file = pendingSkuFilesRef.current.get(index)
                if (!file) {
                    throw new Error(
                        `找不到待上传主图「${sku.mainImage}」的文件内容，请重新选择`,
                    )
                }
                const uploaded = await uploadFileAssetImage(file)
                skus[index] = {
                    ...sku,
                    mainImagePreviewUrl: uploaded.url,
                    mainImageAssetId: uploaded.fileAssetId,
                }
            }
            return {
                ...current,
                carouselPreviewUrls,
                carouselFileAssetIds,
                detailPreviewUrls,
                detailFileAssetIds,
                skus,
            }
        },
        [],
    )
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

    React.useLayoutEffect(() => {
        const el = stickyHeaderRef.current
        if (!el) return
        const update = () => {
            setStickyHeaderHeight(Math.ceil(el.getBoundingClientRect().height))
        }
        update()
        const observer = new ResizeObserver(update)
        observer.observe(el)
        return () => observer.disconnect()
    }, [isCreate, data?.stableId, data?.lockVersion])

    // 未保存离开保护：刷新 / 关闭标签页 / 返回列表
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

    // 校验错误出现时滚动到错误条（P2-15）
    React.useEffect(() => {
        if (formError) {
            errorRef.current?.scrollIntoView({
                block: "center",
                behavior: "smooth",
            })
        }
    }, [formError])

    // 分区 Tab 随滚动高亮（P2-19 scroll spy）
    React.useEffect(() => {
        if (isCreate) return
        const sections = PRODUCT_EDITOR_SECTIONS.map((s) =>
            document.getElementById(`product-section-${s.id}`),
        ).filter((el): el is HTMLElement => el !== null)
        if (sections.length === 0) return
        const observer = new IntersectionObserver(
            (entries) => {
                for (const entry of entries) {
                    if (entry.isIntersecting) {
                        const id = entry.target.id.replace(
                            "product-section-",
                            "",
                        )
                        setActiveSection(id as ProductEditorSectionId)
                    }
                }
            },
            { rootMargin: "-20% 0px -65% 0px", threshold: 0 },
        )
        for (const section of sections) observer.observe(section)
        return () => observer.disconnect()
    }, [isCreate, data?.stableId])

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
    /** 吸顶卡片总高度；分区锚点需额外留一点空隙避免贴边 */
    const sectionScrollMarginPx = stickyHeaderHeight + 12
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

    if (!isCreate && detailQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title="商品详情"
                    description={masterDataCopy.centerLoading}
                />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (!isCreate && (detailQuery.isError || !data)) {
        return (
            <PageScaffold>
                <PageHeader title="商品详情" />
                <BusinessFailureState
                    error={detailQuery.isError ? detailQuery.error : undefined}
                    description={
                        detailQuery.isError
                            ? masterDataCopy.centerLoadFail
                            : masterDataCopy.centerMissingDesc
                    }
                    action={
                        detailQuery.isError ? (
                            <Button
                                type="button"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        ) : (
                            <Button render={<Link href={listHref} />}>
                                {masterDataCopy.actionBackList}
                            </Button>
                        )
                    }
                />
            </PageScaffold>
        )
    }

    if (isCreate && accountQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="新建商品" description="正在核对创建权限" />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (isCreate && accountQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title="新建商品" />
                <BusinessFailureState
                    error={accountQuery.error}
                    onRetry={() => void accountQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (isCreate && !canCreate) {
        return (
            <PageScaffold>
                <PageHeader title="新建商品" />
                <BusinessFailureState
                    kind="permission"
                    description="当前账号没有创建商品的权限，请联系管理员或有权限的同事。"
                    action={
                        <Button render={<Link href={listHref} />}>
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const formId = "product-detail-form"

    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => {
                const title = isCreate
                    ? masterDataCopy.productCreateTitle
                    : values.name || data?.name || "商品详情"
                const fields = values.fields
                const inventoryPreviewSkus: ProductInventoryPreviewSku[] =
                    fields.productKind === "PHYSICAL"
                        ? fields.skus.flatMap((sku) =>
                              sku.skuId
                                  ? [
                                        {
                                            skuId: sku.skuId,
                                            skuNo: sku.skuNo,
                                            specLabel: sku.specLabel,
                                            baseUnit:
                                                sku.baseUnit || fields.baseUnit,
                                        },
                                    ]
                                  : [],
                          )
                        : []
                const inventoryActionHint =
                    fields.productKind && fields.productKind !== "PHYSICAL"
                        ? "仅实物商品适用公司自有库存台账"
                        : inventoryPreviewSkus.length === 0
                          ? "选择实物商品类型并保存 SKU 后可查看正式库存"
                          : undefined
                const setName = (next: string) =>
                    form.setFieldValue("name", next)
                const setEffectiveFrom = (next: string) =>
                    form.setFieldValue("effectiveFrom", next)
                const setEffectiveTo = (next: string) =>
                    form.setFieldValue("effectiveTo", next)
                const setChangeReason = (next: string) =>
                    form.setFieldValue("changeReason", next)
                const setFields = (next: React.SetStateAction<ProductFields>) =>
                    form.setFieldValue("fields", (previous) =>
                        typeof next === "function" ? next(previous) : next,
                    )
                const setSpecDrafts = (
                    next: React.SetStateAction<readonly ProductSpecDraft[]>,
                ) =>
                    form.setFieldValue("specDrafts", (previous) =>
                        typeof next === "function" ? next(previous) : next,
                    )
                const syncSpecDrafts = (next: readonly ProductSpecDraft[]) => {
                    setSpecDrafts(next)
                    setFields((previous) =>
                        applySpecsFromDrafts(next, previous, values.name),
                    )
                }
                const updateSku = (
                    index: number,
                    patch: Partial<ProductSkuFields>,
                ) => {
                    setFields((previous) => ({
                        ...previous,
                        skus: previous.skus.map((sku, skuIndex) =>
                            skuIndex === index ? { ...sku, ...patch } : sku,
                        ),
                    }))
                }
                const handleSubmit = (event?: React.FormEvent) => {
                    event?.preventDefault()
                    void form.handleSubmit()
                }
                const name = values.name
                const effectiveFrom = values.effectiveFrom
                const effectiveTo = values.effectiveTo
                const changeReason = values.changeReason
                const specDrafts = values.specDrafts
                const activeSpecs = fields.specs.filter(
                    (spec) =>
                        spec.name.trim() &&
                        spec.values.some((value) => value.trim()),
                )
                const applyBatchReferencePrices = () => {
                    const hasAny =
                        values.batchSalePrice.trim() ||
                        values.batchMarketPrice.trim()
                    if (!hasAny) return
                    const hasFilled = values.fields.skus.some(
                        (sku) =>
                            sku.salePrice?.trim() || sku.marketPrice?.trim(),
                    )
                    const message = hasFilled
                        ? `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU，并覆盖已填写的销售价/市场价。确定继续？`
                        : `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU。确定继续？`
                    if (!window.confirm(message)) return
                    setFields((previous) => ({
                        ...previous,
                        skus: previous.skus.map((sku) => ({
                            ...sku,
                            salePrice:
                                values.batchSalePrice.trim() ||
                                sku.salePrice ||
                                undefined,
                            marketPrice:
                                values.batchMarketPrice.trim() ||
                                sku.marketPrice ||
                                undefined,
                        })),
                    }))
                }
                return (
                    <PageScaffold
                        style={
                            {
                                "--product-section-scroll-margin": `${sectionScrollMarginPx}px`,
                            } as React.CSSProperties
                        }
                    >
                        <form
                            id={formId}
                            className="flex flex-col gap-4"
                            onSubmit={handleSubmit}
                        >
                            <header
                                ref={stickyHeaderRef}
                                className={cn(
                                    surfacePanelClassName,
                                    "sticky top-0 z-30 overflow-hidden",
                                )}
                            >
                                <div className="flex flex-col gap-3 p-4 lg:flex-row lg:items-center lg:justify-between">
                                    <div className="min-w-0 space-y-1">
                                        <div className="flex min-w-0 flex-wrap items-center gap-2">
                                            <h1 className="truncate text-lg font-semibold tracking-tight">
                                                {title}
                                            </h1>
                                            {!isCreate && data ? (
                                                <StatusBadge
                                                    tone={data.lifecycleTone}
                                                    label={
                                                        data.lifecycleStatusLabel
                                                    }
                                                />
                                            ) : null}
                                        </div>
                                        {!isCreate && data ? (
                                            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                                                <span>
                                                    单号{" "}
                                                    <span className="num text-foreground">
                                                        {data.stableNo}
                                                    </span>
                                                </span>
                                                <span className="num rounded-md bg-muted px-1.5 py-0.5 text-tiny text-foreground">
                                                    版本{" "}
                                                    {
                                                        data.currentRevision
                                                            .revisionNo
                                                    }
                                                </span>
                                                <span className="num">
                                                    {formatEffectiveRange(
                                                        data.currentRevision
                                                            .effectiveFrom,
                                                        data.currentRevision
                                                            .effectiveTo,
                                                    )}
                                                </span>
                                                <span className="inline-flex items-center gap-1.5">
                                                    <span>
                                                        {
                                                            masterDataCopy.centerVersionState
                                                        }
                                                    </span>
                                                    <StatusBadge
                                                        tone={
                                                            data.revisionTiming ===
                                                            "FUTURE"
                                                                ? "warning"
                                                                : "info"
                                                        }
                                                        label={
                                                            data.revisionTimingLabel
                                                        }
                                                    />
                                                </span>
                                            </div>
                                        ) : (
                                            <p className="text-sm text-muted-foreground">
                                                {
                                                    masterDataCopy.productCreateDesc
                                                }
                                            </p>
                                        )}
                                    </div>

                                    <div className="flex shrink-0 flex-wrap items-center gap-2">
                                        {!isCreate && data ? (
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={!canDisable}
                                                title={
                                                    !hasUpdatePermission
                                                        ? "当前账号没有维护商品资料的权限。"
                                                        : disableBlocker?.message
                                                }
                                                onClick={() =>
                                                    setDisableOpen(true)
                                                }
                                            >
                                                <BanIcon
                                                    data-icon="inline-start"
                                                    aria-hidden
                                                />
                                                {masterDataCopy.actionDisable}
                                            </Button>
                                        ) : null}
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            onClick={() =>
                                                navigateAway(listHref)
                                            }
                                        >
                                            返回列表
                                        </Button>
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={!canRevise || pending}
                                            onClick={() =>
                                                runLocalCheck(values)
                                            }
                                        >
                                            <ClipboardCheckIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            填写检查
                                        </Button>
                                        <Button
                                            type="submit"
                                            size="sm"
                                            disabled={!canRevise || pending}
                                        >
                                            <SaveIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            {isCreate
                                                ? masterDataCopy.createSubmit
                                                : masterDataCopy.reviseSubmit}
                                        </Button>
                                    </div>
                                </div>

                                {!isCreate && data?.productConstraints ? (
                                    <div className="border-t border-border/60 bg-muted/40 px-4 py-2.5 text-xs">
                                        <p>
                                            基础单位{" "}
                                            <span className="num">
                                                {
                                                    data.productConstraints
                                                        .baseUnit
                                                }
                                            </span>
                                            {" · "}
                                            SKU{" "}
                                            <span className="num">
                                                {
                                                    data.productConstraints
                                                        .skuCount
                                                }
                                            </span>{" "}
                                            个
                                            {data.productConstraints
                                                .hasFormalReferences
                                                ? " · 已被业务单据引用"
                                                : null}
                                        </p>
                                        <p className="mt-1 text-muted-foreground">
                                            {masterDataCopy.centerSpecNote}
                                        </p>
                                    </div>
                                ) : null}

                                <div className="border-t border-border/60 p-2 sm:px-3 sm:pb-3">
                                    <nav
                                        aria-label="商品编辑分区"
                                        className={cn(
                                            "grid grid-cols-2 gap-0.5 rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10",
                                            isCreate
                                                ? "sm:grid-cols-4"
                                                : "sm:grid-cols-5",
                                        )}
                                    >
                                        {PRODUCT_EDITOR_SECTIONS.filter(
                                            (section) =>
                                                !isCreate ||
                                                section.id !== "history",
                                        ).map((section) => {
                                            const active =
                                                activeSection === section.id
                                            return (
                                                <Button
                                                    key={section.id}
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    className={cn(
                                                        "relative h-7 rounded-md text-sm",
                                                        active
                                                            ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10 hover:bg-card"
                                                            : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                                    )}
                                                    aria-current={
                                                        active
                                                            ? "location"
                                                            : undefined
                                                    }
                                                    onClick={() => {
                                                        setActiveSection(
                                                            section.id,
                                                        )
                                                        scrollToProductSection(
                                                            section.id,
                                                        )
                                                    }}
                                                >
                                                    {section.label}
                                                </Button>
                                            )
                                        })}
                                    </nav>
                                </div>
                            </header>

                            <div className="flex min-w-0 flex-col gap-4">
                                {!isCreate && !canRevise ? (
                                    <Alert variant="info">
                                        <AlertTitle>你只能查看</AlertTitle>
                                        <AlertDescription>
                                            {reviseBlocker
                                                ? masterDataCopy.centerUpdateBlocked(
                                                      reviseBlocker.message,
                                                  )
                                                : "当前账号没有维护商品资料的权限；需要修改请联系有权限的同事。"}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                {result?.outcome === "blocked" ? (
                                    <FormalActionResult
                                        status="blocked"
                                        title={
                                            isCreate
                                                ? masterDataCopy.createBlockedTitle
                                                : masterDataCopy.reviseBlockedTitle
                                        }
                                        description={result.message}
                                        facts={
                                            result.detail
                                                ? [
                                                      {
                                                          label: "说明",
                                                          value: result.detail,
                                                      },
                                                  ]
                                                : undefined
                                        }
                                    />
                                ) : null}

                                {result?.outcome === "conflict" ? (
                                    <FormalActionResult
                                        status="blocked"
                                        title={
                                            masterDataCopy.reviseConflictTitle
                                        }
                                        description={
                                            result.message ||
                                            masterDataCopy.reviseConflictHint
                                        }
                                    />
                                ) : null}

                                {formError ? (
                                    <div ref={errorRef}>
                                        <Alert variant="destructive">
                                            <CircleAlertIcon aria-hidden />
                                            <AlertTitle>
                                                {formErrorTitle}
                                            </AlertTitle>
                                            <AlertDescription>
                                                {formError}
                                            </AlertDescription>
                                        </Alert>
                                    </div>
                                ) : null}

                                {checkPassed &&
                                checkedSnapshotRef.current ===
                                    JSON.stringify({
                                        ...values,
                                        fields,
                                    }) ? (
                                    <Alert variant="success">
                                        <CheckCircle2Icon aria-hidden />
                                        <AlertTitle>填写检查通过</AlertTitle>
                                        <AlertDescription>
                                            必填项完整，保存时仍以系统校验结果为准。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}

                                <div
                                    className={cn(
                                        surfacePanelClassName,
                                        "overflow-hidden",
                                    )}
                                >
                                    <ProductBasicSection
                                        isCreate={isCreate}
                                        canRevise={canRevise}
                                        name={name}
                                        setName={setName}
                                        fields={fields}
                                        setFields={setFields}
                                        unitOptions={unitOptionsQuery.data}
                                        categoryOptions={categoryOptions}
                                        brandOptions={brandOptions}
                                        categoryLoading={
                                            categoryListQuery.isPending
                                        }
                                        brandLoading={brandListQuery.isPending}
                                    />

                                    <ProductMediaSection
                                        canRevise={canRevise}
                                        fields={fields}
                                        setFields={setFields}
                                        rememberPendingFiles={
                                            rememberPendingFiles
                                        }
                                    />
                                    <ProductSkuSection
                                        isCreate={isCreate}
                                        canRevise={canRevise}
                                        name={name}
                                        fields={fields}
                                        specDrafts={specDrafts}
                                        activeSpecs={activeSpecs}
                                        inventoryPreviewSkus={
                                            inventoryPreviewSkus
                                        }
                                        syncSpecDrafts={syncSpecDrafts}
                                        updateSku={updateSku}
                                        batchSalePrice={values.batchSalePrice}
                                        batchMarketPrice={
                                            values.batchMarketPrice
                                        }
                                        setBatchSalePrice={(next) =>
                                            form.setFieldValue(
                                                "batchSalePrice",
                                                next,
                                            )
                                        }
                                        setBatchMarketPrice={(next) =>
                                            form.setFieldValue(
                                                "batchMarketPrice",
                                                next,
                                            )
                                        }
                                        onApplyBatchReferencePrices={
                                            applyBatchReferencePrices
                                        }
                                        inventoryActionHint={
                                            inventoryActionHint
                                        }
                                        onOpenInventory={openInventoryPreview}
                                        rememberSkuFile={rememberSkuFile}
                                        supplierCounts={
                                            supplierCountsQuery.data
                                        }
                                        supplierCountsPending={
                                            supplierCountsQuery.isPending
                                        }
                                        supplierCountsError={
                                            supplierCountsQuery.isError
                                                ? supplierCountsQuery.error
                                                : null
                                        }
                                        onRegisterSupply={setSupplierDialogSku}
                                        stableId={stableId}
                                    />

                                    <ProductEffectiveSection
                                        isCreate={isCreate}
                                        canRevise={canRevise}
                                        effectiveFrom={effectiveFrom}
                                        effectiveTo={effectiveTo}
                                        changeReason={changeReason}
                                        setEffectiveFrom={setEffectiveFrom}
                                        setEffectiveTo={setEffectiveTo}
                                        setChangeReason={setChangeReason}
                                    />

                                    {!isCreate ? (
                                        <ProductHistorySection data={data} />
                                    ) : null}
                                </div>
                            </div>
                        </form>

                        {!isCreate && data ? (
                            <MasterDataDisableDialog
                                open={disableOpen}
                                onOpenChange={setDisableOpen}
                                resource="products"
                                target={data}
                            />
                        ) : null}
                        <RegisterSupplyForSkuDialog
                            key={supplierDialogSku?.skuId ?? "register-supply"}
                            open={Boolean(supplierDialogSku)}
                            onOpenChange={(open) => {
                                if (!open) setSupplierDialogSku(undefined)
                            }}
                            fixedSku={supplierDialogSku}
                        />
                        <ProductInventoryPreviewSheet
                            open={inventoryOpen}
                            onOpenChange={handleInventoryOpenChange}
                            productName={title}
                            productKind={fields.productKind}
                            skus={inventoryPreviewSkus}
                            initialSkuId={inventoryInitialSkuId}
                        />
                        <DiscardConfirmDialog
                            open={discardOpen}
                            onOpenChange={setDiscardOpen}
                            title="放弃未保存的更改？"
                            description="本次修改尚未保存，离开后将丢失。"
                            confirmLabel="放弃更改"
                            cancelLabel="继续编辑"
                            onConfirm={() => {
                                setDiscardOpen(false)
                                if (pendingNav) {
                                    setPendingNav(null)
                                    router.push(pendingNav)
                                }
                            }}
                        />
                    </PageScaffold>
                )
            }}
        </form.Subscribe>
    )
}
