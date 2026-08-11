"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { DownloadIcon, PlusIcon } from "lucide-react"
import type { PaginationState } from "@tanstack/react-table"
import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FormalActionResult,
    MetricFilterItem,
    MetricItem,
    MetricStrip,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    buildMasterDataExportCsv,
    downloadCsv,
} from "@/features/master-data/queries"
import {
    masterDataCopy,
    lifecycleFilterLabel,
    revisionTimingFilterLabel,
} from "@/features/master-data/copy"
import { resourceLabel } from "@/features/master-data/data"
import { useMasterDataColumns } from "@/features/master-data/master-data-columns"
import { MasterDataListToolbar } from "@/features/master-data/master-data-list-toolbar"
import { MasterDataPreviewSheet } from "@/features/master-data/master-data-preview-sheet"
import {
    CREATE_PERMISSION_BY_RESOURCE,
    csvFilterValue,
    isResource,
    productSalesPriceRangeError,
    PRODUCT_COVERAGE_FILTER_OPTIONS,
    PRODUCT_LISTING_FILTER_OPTIONS,
    qualificationHealthLabel,
    ResourceNav,
    selectedSupplierOptionLabels,
    selectedSupplierOptionValues,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/master-data-list-presentation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { CategoryTreePage } from "@/features/master-data/category-tree-page"
import {
    MasterDataCreateDialog,
    MasterDataDisableDialog,
    MasterDataReviseDialog,
} from "@/features/master-data/master-data-action-dialog"
import { ProductSupplyDialog } from "@/features/master-data/product-supply-dialog"
import { VoucherCategoryFormDialog } from "@/features/master-data/voucher-category-form-dialog"
import {
    useMasterDataCenterQuery,
    useMasterDataExportMutation,
    useMasterDataListQuery,
    useProductFilterOptionsQuery,
    useProductListSkusQuery,
    useProductListingMutation,
} from "@/features/master-data/queries"
import {
    PRODUCT_KIND_LABELS,
    PRODUCT_KIND_VALUES,
    type MasterDataListItem,
    type MasterDataResource,
    type ProductKind,
    type ProductListingFilter,
    type ProductListSkuSummary,
    type ProductSkuCoverageFilter,
    type SupplierQualificationHealth,
} from "@/features/master-data/types"
import { RegisterSupplyForSkuDialog } from "@/features/supplier-offerings/offering-dialogs"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/queries"
import type { FixedSku } from "@/features/supplier-offerings/types"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"

export function MasterDataPage({ resource }: { resource: string }) {
    const navRef = React.useRef<HTMLElement | null>(null)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const resultsHeadingRef = React.useRef<HTMLHeadingElement | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)

    const valid = isResource(resource)

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key === "/" &&
                !(event.target instanceof HTMLInputElement) &&
                !(event.target instanceof HTMLTextAreaElement)
            ) {
                // 弹窗 / 抽屉打开时不让 / 聚焦背景搜索框
                if (
                    document.querySelector(
                        '[role="dialog"], [data-slot="sheet"]',
                    )
                ) {
                    return
                }
                event.preventDefault()
                searchInputRef.current?.focus()
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    React.useEffect(() => {
        if (!valid) return
        // Focus results title after resource switch for a11y announcement
        const t = window.setTimeout(() => {
            resultsHeadingRef.current?.focus()
        }, 0)
        return () => window.clearTimeout(t)
    }, [resource, valid])

    if (!valid) {
        return (
            <PageScaffold>
                <PageHeader
                    title={masterDataCopy.unknownResourceTitle}
                    description={masterDataCopy.unknownResourceDesc()}
                />
                <ResourceNav resource="" navRef={navRef} />
            </PageScaffold>
        )
    }

    /** 商品分类：树形维护，不走扁平列表。 */
    if (resource === "categories") {
        return <CategoryTreePage />
    }

    return (
        <MasterDataListWorkspace
            resource={resource}
            navRef={navRef}
            searchInputRef={searchInputRef}
            resultsHeadingRef={resultsHeadingRef}
            lastFocusedRowId={lastFocusedRowId}
        />
    )
}

function MasterDataListWorkspace({
    resource,
    navRef,
    searchInputRef,
    resultsHeadingRef,
    lastFocusedRowId,
}: {
    resource: MasterDataResource
    navRef: React.RefObject<HTMLElement | null>
    searchInputRef: React.RefObject<HTMLInputElement | null>
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    lastFocusedRowId: React.MutableRefObject<string | null>
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const accountQuery = useAccountProfileQuery()
    /** 商品（SPU）走详情页，不用侧边 sheet。 */
    const isProductResource = resource === "products"
    /** 供应商走详情页（查看与编辑同一页面），不用侧边 sheet / 编辑弹窗。 */
    const isSupplierResource = resource === "suppliers"
    /** 品牌走详情页（通用资料中心），不用右侧 sheet 预览。 */
    const isBrandResource = resource === "brands"
    /** 卡券类目：列表原地 Dialog 新建/编辑，无查看预览、无停用。 */
    const isVoucherCategoryResource = resource === "voucher-categories"
    /** 计量单位：列表 Dialog 更新/停用，无侧边预览、无独立详情入口。 */
    const isUnitOfMeasureResource = resource === "unit-of-measures"
    /** 公司商品池为资格查询视图，只允许查看与导出。 */
    const isSellableResource = resource === "sellable-items"
    const createPermission = CREATE_PERMISSION_BY_RESOURCE[resource]
    const canCreate = createPermission
        ? hasPermission(accountQuery.data?.permissions, createPermission)
        : false
    const createBlockedReason = accountQuery.isPending
        ? "正在核对创建权限，请稍候。"
        : accountQuery.isError
          ? getErrorMessage(
                accountQuery.error,
                "暂时无法核对创建权限，请刷新后重试。",
            )
          : "当前账号没有新建此类资料的权限。"
    const skipPreviewSheet =
        isProductResource ||
        isSupplierResource ||
        isBrandResource ||
        isVoucherCategoryResource ||
        isUnitOfMeasureResource
    /** 即时字典（品牌 / 计量单位等）不展示生效期间列。 */
    const showEffectiveColumn =
        resource !== "brands" &&
        resource !== "unit-of-measures" &&
        !isSupplierResource

    // ── 筛选与分页唯一事实源 = URL（刷新/后退/分享一致） ──
    const q = searchParams.get("q") ?? ""
    const lifecycleStatusParam = searchParams.get("lifecycleStatus")
    const lifecycleStatus: "enabled" | "disabled" | "all" =
        lifecycleStatusParam === "enabled" ||
        lifecycleStatusParam === "disabled"
            ? lifecycleStatusParam
            : "all"
    const revisionTimingParam = searchParams.get("revisionTiming")
    const revisionTiming: "current" | "future" | "all" =
        revisionTimingParam === "current" || revisionTimingParam === "future"
            ? revisionTimingParam
            : "all"
    const productKind = isProductResource
        ? PRODUCT_KIND_VALUES.find(
              (value) => value === searchParams.get("productKind"),
          )
        : undefined
    const productCategoryId = isProductResource
        ? searchParams.get("productCategoryId")?.trim() || undefined
        : undefined
    const productBrandId = isProductResource
        ? searchParams.get("productBrandId")?.trim() || undefined
        : undefined
    const productSupplierId = isProductResource
        ? searchParams.get("productSupplierId")?.trim() || undefined
        : undefined
    const productListingStatus = isProductResource
        ? PRODUCT_LISTING_FILTER_OPTIONS.find(
              (option) =>
                  option.value === searchParams.get("productListingStatus"),
          )?.value
        : undefined
    const productSupplyCoverage = isProductResource
        ? PRODUCT_COVERAGE_FILTER_OPTIONS.find(
              (option) =>
                  option.value === searchParams.get("productSupplyCoverage"),
          )?.value
        : undefined
    const productSalesPriceMin = isProductResource
        ? searchParams.get("productSalesPriceMin")?.trim() || undefined
        : undefined
    const productSalesPriceMax = isProductResource
        ? searchParams.get("productSalesPriceMax")?.trim() || undefined
        : undefined
    const supplierCapabilityCodesParam = searchParams.get(
        "supplierCapabilityCodes",
    )
    const supplierQualificationTypesParam = searchParams.get(
        "supplierQualificationTypes",
    )
    const supplierCapabilityCodes = React.useMemo(
        () =>
            isSupplierResource
                ? selectedSupplierOptionValues(
                      supplierCapabilityCodesParam,
                      SUPPLIER_CAPABILITY_OPTIONS,
                  )
                : [],
        [isSupplierResource, supplierCapabilityCodesParam],
    )
    const supplierQualificationTypes = React.useMemo(
        () =>
            isSupplierResource
                ? selectedSupplierOptionValues(
                      supplierQualificationTypesParam,
                      SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
                  )
                : [],
        [isSupplierResource, supplierQualificationTypesParam],
    )
    const supplierQualificationHealthParam = searchParams.get(
        "supplierQualificationHealth",
    )
    const supplierQualificationHealth = isSupplierResource
        ? (SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.find(
              (option) =>
                  option.value !== "all" &&
                  option.value === supplierQualificationHealthParam,
          )?.value as SupplierQualificationHealth | undefined)
        : undefined
    /** 指标态保留在 URL：与 lifecycleStatus 同源写入，只做展示不做筛选。 */
    const metricKey = searchParams.get("metricKey") ?? "all"
    /** 已生效（非草稿）的结构化商品筛选是否有任意一项非默认值，决定"高级筛选"默认展开与徽标。 */
    const hasStructuredProductFilters = Boolean(
        productKind ||
        lifecycleStatus !== "all" ||
        revisionTiming !== "all" ||
        productCategoryId ||
        productBrandId ||
        productSupplierId ||
        productListingStatus ||
        productSupplyCoverage ||
        productSalesPriceMin ||
        productSalesPriceMax,
    )
    const hasStructuredSupplierFilters = Boolean(
        lifecycleStatus !== "all" ||
        supplierQualificationHealth ||
        supplierCapabilityCodes.length ||
        supplierQualificationTypes.length,
    )
    const pageParamRaw = Number(searchParams.get("page"))
    const pageParamIndex =
        Number.isFinite(pageParamRaw) && pageParamRaw > 0
            ? Math.max(0, Math.floor(pageParamRaw) - 1)
            : 0

    const [searchDraft, setSearchDraft] = React.useState(q)
    /** 高级筛选面板展开态：本地 UI 状态，不入 URL；有已生效筛选时默认展开。 */
    const [productFilterPanelOpen, setProductFilterPanelOpen] = React.useState(
        hasStructuredProductFilters,
    )
    /** 供应商显式提交筛选面板展开态；深链带入条件时自动展开。 */
    const [supplierFilterPanelOpen, setSupplierFilterPanelOpen] =
        React.useState(hasStructuredSupplierFilters)
    const [supplierCapabilityCodesDraft, setSupplierCapabilityCodesDraft] =
        React.useState<string[]>(supplierCapabilityCodes)
    const [
        supplierQualificationTypesDraft,
        setSupplierQualificationTypesDraft,
    ] = React.useState<string[]>(supplierQualificationTypes)
    const [
        supplierQualificationHealthDraft,
        setSupplierQualificationHealthDraft,
    ] = React.useState<SupplierQualificationHealth | "all">(
        supplierQualificationHealth ?? "all",
    )
    // 商品与供应商筛选面板均为"编辑草稿 + 点击搜索才提交"模式：以下均为本地草稿，
    // 与 URL 事实源的同步只发生在挂载、外部 URL 变化（后退/清除）与提交时。
    const [productKindDraft, setProductKindDraft] = React.useState<
        ProductKind | "all"
    >(productKind ?? "all")
    const [lifecycleStatusDraft, setLifecycleStatusDraft] = React.useState<
        "enabled" | "disabled" | "all"
    >(lifecycleStatus)
    const [revisionTimingDraft, setRevisionTimingDraft] = React.useState<
        "current" | "future" | "all"
    >(revisionTiming)
    const [productListingStatusDraft, setProductListingStatusDraft] =
        React.useState<ProductListingFilter | "all">(
            productListingStatus ?? "all",
        )
    const [productSupplyCoverageDraft, setProductSupplyCoverageDraft] =
        React.useState<ProductSkuCoverageFilter | "all">(
            productSupplyCoverage ?? "all",
        )
    const [productCategoryIdDraft, setProductCategoryIdDraft] = React.useState<
        string | null
    >(productCategoryId ?? null)
    const [productBrandIdDraft, setProductBrandIdDraft] = React.useState<
        string | null
    >(productBrandId ?? null)
    const [productSupplierIdDraft, setProductSupplierIdDraft] = React.useState<
        string | null
    >(productSupplierId ?? null)
    const [productSalesPriceMinDraft, setProductSalesPriceMinDraft] =
        React.useState(productSalesPriceMin ?? "")
    const [productSalesPriceMaxDraft, setProductSalesPriceMaxDraft] =
        React.useState(productSalesPriceMax ?? "")
    const [productSalesPriceError, setProductSalesPriceError] = React.useState<
        string | null
    >(null)
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageParamIndex,
        pageSize: 20,
    })
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [reviseTarget, setReviseTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyProduct, setSupplyProduct] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyDialogSku, setSupplyDialogSku] =
        React.useState<FixedSku | null>(null)
    const [exportMeta, setExportMeta] = React.useState<{
        jobId: string
        rowCount: number
        filterSnapshotLabel: string
    } | null>(null)

    const patchUrl = React.useCallback(
        (patch: Record<string, string | null>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") next.delete(key)
                else next.set(key, value)
            }
            const qs = next.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const resetPagination = React.useCallback(() => {
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }, [])

    const changeLifecycle = React.useCallback(
        (next: "enabled" | "disabled" | "all") => {
            if (next === lifecycleStatus) return
            patchUrl({
                lifecycleStatus: next === "all" ? null : next,
                metricKey: next === "all" ? null : next,
                page: null,
            })
            resetPagination()
        },
        [lifecycleStatus, patchUrl, resetPagination],
    )

    const changeRevisionTiming = React.useCallback(
        (next: "current" | "future" | "all") => {
            if (next === revisionTiming) return
            patchUrl({
                revisionTiming: next === "all" ? null : next,
                page: null,
            })
            resetPagination()
        },
        [patchUrl, resetPagination, revisionTiming],
    )

    /** 提交供应商筛选面板；全部草稿字段一次性写入 URL。 */
    const applySupplierFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            lifecycleStatus:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            metricKey:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            supplierCapabilityCodes: csvFilterValue(
                supplierCapabilityCodesDraft,
            ),
            supplierQualificationTypes: csvFilterValue(
                supplierQualificationTypesDraft,
            ),
            supplierQualificationHealth:
                supplierQualificationHealthDraft === "all"
                    ? null
                    : supplierQualificationHealthDraft,
            page: null,
        })
        resetPagination()
    }, [
        patchUrl,
        resetPagination,
        searchDraft,
        lifecycleStatusDraft,
        supplierCapabilityCodesDraft,
        supplierQualificationHealthDraft,
        supplierQualificationTypesDraft,
    ])

    /** 商品筛选面板整体提交：点击"搜索"才把全部草稿字段一次性写入 URL。 */
    const applyProductFilters = React.useCallback(() => {
        const minimum = productSalesPriceMinDraft.trim()
        const maximum = productSalesPriceMaxDraft.trim()
        const error = productSalesPriceRangeError(minimum, maximum)
        setProductSalesPriceError(error)
        if (error) return
        patchUrl({
            q: searchDraft.trim() || null,
            productKind: productKindDraft === "all" ? null : productKindDraft,
            lifecycleStatus:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            metricKey:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            revisionTiming:
                revisionTimingDraft === "all" ? null : revisionTimingDraft,
            productListingStatus:
                productListingStatusDraft === "all"
                    ? null
                    : productListingStatusDraft,
            productSupplyCoverage:
                productSupplyCoverageDraft === "all"
                    ? null
                    : productSupplyCoverageDraft,
            productCategoryId: productCategoryIdDraft,
            productBrandId: productBrandIdDraft,
            productSupplierId: productSupplierIdDraft,
            productSalesPriceMin: minimum || null,
            productSalesPriceMax: maximum || null,
            page: null,
        })
        resetPagination()
    }, [
        patchUrl,
        resetPagination,
        searchDraft,
        productKindDraft,
        lifecycleStatusDraft,
        revisionTimingDraft,
        productListingStatusDraft,
        productSupplyCoverageDraft,
        productCategoryIdDraft,
        productBrandIdDraft,
        productSupplierIdDraft,
        productSalesPriceMinDraft,
        productSalesPriceMaxDraft,
    ])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setProductKindDraft("all")
        setLifecycleStatusDraft("all")
        setRevisionTimingDraft("all")
        setProductListingStatusDraft("all")
        setProductSupplyCoverageDraft("all")
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setProductSalesPriceError(null)
        setProductFilterPanelOpen(false)
        setSupplierCapabilityCodesDraft([])
        setSupplierQualificationTypesDraft([])
        setSupplierQualificationHealthDraft("all")
        setSupplierFilterPanelOpen(false)
        patchUrl({
            q: null,
            lifecycleStatus: null,
            metricKey: null,
            revisionTiming: null,
            productKind: null,
            productCategoryId: null,
            productBrandId: null,
            productSupplierId: null,
            productListingStatus: null,
            productSupplyCoverage: null,
            productSalesPriceMin: null,
            productSalesPriceMax: null,
            supplierCapabilityCodes: null,
            supplierQualificationTypes: null,
            supplierQualificationHealth: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination])

    // URL 回填草稿（后退/前进）；输入内容只在显式提交搜索后写入 URL。
    React.useEffect(() => {
        setSearchDraft(q)
    }, [q])

    React.useEffect(() => {
        setProductKindDraft(productKind ?? "all")
        setLifecycleStatusDraft(lifecycleStatus)
        setRevisionTimingDraft(revisionTiming)
        setProductListingStatusDraft(productListingStatus ?? "all")
        setProductSupplyCoverageDraft(productSupplyCoverage ?? "all")
        setProductCategoryIdDraft(productCategoryId ?? null)
        setProductBrandIdDraft(productBrandId ?? null)
        setProductSupplierIdDraft(productSupplierId ?? null)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
        setProductFilterPanelOpen(hasStructuredProductFilters)
    }, [
        productKind,
        lifecycleStatus,
        revisionTiming,
        productListingStatus,
        productSupplyCoverage,
        productCategoryId,
        productBrandId,
        productSupplierId,
        productSalesPriceMax,
        productSalesPriceMin,
        hasStructuredProductFilters,
    ])

    React.useEffect(() => {
        setSupplierCapabilityCodesDraft(supplierCapabilityCodes)
        setSupplierQualificationTypesDraft(supplierQualificationTypes)
        setSupplierQualificationHealthDraft(
            supplierQualificationHealth ?? "all",
        )
        setSupplierFilterPanelOpen(hasStructuredSupplierFilters)
    }, [
        hasStructuredSupplierFilters,
        supplierCapabilityCodes,
        supplierQualificationHealth,
        supplierQualificationTypes,
    ])

    // URL page 回读（后退/前进/分享恢复）
    React.useEffect(() => {
        setPagination((p) => ({ ...p, pageIndex: pageParamIndex }))
    }, [pageParamIndex])

    // 切换资源时重置本地 UI 状态（筛选来自新 URL，天然为空）
    React.useEffect(() => {
        setPreviewId(null)
        setExportMeta(null)
        setSupplyProduct(null)
        setSupplyDialogSku(null)
    }, [resource])

    const listQuery = useMasterDataListQuery({
        resource,
        q: q.trim() || undefined,
        lifecycleStatus,
        revisionTiming,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        productListingStatus,
        productSupplyCoverage,
        productSalesPriceMin,
        productSalesPriceMax,
        supplierCapabilityCodes,
        supplierQualificationTypes,
        supplierQualificationHealth,
        // metricKey 只做展示不做筛选：指标与 ToggleGroup 共用 lifecycleStatus 状态源
        metricKey: undefined,
    })
    const productFilterOptionsQuery =
        useProductFilterOptionsQuery(isProductResource)
    const exportMutation = useMasterDataExportMutation()
    const productListingMutation = useProductListingMutation()
    const [listingError, setListingError] = React.useState<string | null>(null)
    const canUpdateProductListing =
        isProductResource &&
        hasPermission(accountQuery.data?.permissions, "product:update")

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )

    const previewDetailQuery = useMasterDataCenterQuery(
        resource,
        isSellableResource ? "" : (previewId ?? ""),
    )

    const previewRow = React.useMemo(
        () => rows.find((r) => r.stableId === previewId) ?? null,
        [previewId, rows],
    )

    const pageRows = React.useMemo(() => {
        const start = pagination.pageIndex * pagination.pageSize
        return rows.slice(start, start + pagination.pageSize)
    }, [pagination.pageIndex, pagination.pageSize, rows])

    const productPageIds = React.useMemo(
        () => (isProductResource ? pageRows.map((row) => row.stableId) : []),
        [isProductResource, pageRows],
    )
    const productSkusQuery = useProductListSkusQuery(productPageIds)
    const productSkusByProduct = React.useMemo(() => {
        const grouped = new Map<string, ProductListSkuSummary[]>()
        for (const sku of productSkusQuery.data ?? []) {
            const skus = grouped.get(sku.productId) ?? []
            skus.push(sku)
            grouped.set(sku.productId, skus)
        }
        return grouped
    }, [productSkusQuery.data])
    const productPageSkuIds = React.useMemo(
        () => (productSkusQuery.data ?? []).map((sku) => sku.skuId),
        [productSkusQuery.data],
    )
    const supplierOfferingsQuery =
        useSupplierOfferingsForSkusQuery(productPageSkuIds)
    const currentSupplySkuIds = React.useMemo(
        () =>
            new Set(
                (supplierOfferingsQuery.data ?? [])
                    .filter(
                        (offering) =>
                            offering.status === "ACTIVE" &&
                            Boolean(offering.current_revision_id),
                    )
                    .map((offering) => offering.sku_id),
            ),
        [supplierOfferingsQuery.data],
    )
    const selectedCategoryLabel = React.useMemo(
        () =>
            productFilterOptionsQuery.data?.categories.find(
                (option) => option.value === productCategoryId,
            )?.label ?? productCategoryId,
        [productCategoryId, productFilterOptionsQuery.data?.categories],
    )
    const selectedBrandLabel = React.useMemo(
        () =>
            productFilterOptionsQuery.data?.brands.find(
                (option) => option.value === productBrandId,
            )?.label ?? productBrandId,
        [productBrandId, productFilterOptionsQuery.data?.brands],
    )
    const selectedSupplierLabel = React.useMemo(
        () =>
            (productFilterOptionsQuery.data?.suppliers ?? []).find(
                (option) => option.value === productSupplierId,
            )?.label ?? productSupplierId,
        [productFilterOptionsQuery.data?.suppliers, productSupplierId],
    )
    /** 指标与当前搜索/启停/版本筛选同步，避免「全部 3」与表格行数矛盾。 */
    const syncedMetrics = React.useMemo(() => {
        const base = listQuery.data?.metrics ?? []
        if (rows.length === 0 || listQuery.data == null) return base
        const metricCount = (key: string): number => {
            switch (key) {
                case "enabled":
                    return rows.filter((r) => r.lifecycleStatus === "ENABLED")
                        .length
                case "disabled":
                    return rows.filter((r) => r.lifecycleStatus === "DISABLED")
                        .length
                case "pending":
                    return rows.filter((r) => r.revisionTiming === "FUTURE")
                        .length
                case "expiring":
                    return rows.filter((r) => r.metricTags.includes("expiring"))
                        .length
                default:
                    return rows.length
            }
        }
        return base.map((metric) => ({
            ...metric,
            value: metricCount(metric.key),
        }))
    }, [listQuery.data, rows])

    const filterSnapshotLabel = React.useMemo(() => {
        const parts = [
            `分类=${resourceLabel(resource)}`,
            `启用状态=${lifecycleFilterLabel(lifecycleStatus)}`,
            ...(isSupplierResource
                ? [
                      `资质状态=${qualificationHealthLabel(supplierQualificationHealth)}`,
                  ]
                : [`版本状态=${revisionTimingFilterLabel(revisionTiming)}`]),
            ...(supplierCapabilityCodes.length > 0
                ? [
                      `供应能力=${selectedSupplierOptionLabels(
                          supplierCapabilityCodes,
                          SUPPLIER_CAPABILITY_OPTIONS,
                      ).join("、")}`,
                  ]
                : []),
            ...(supplierQualificationTypes.length > 0
                ? [
                      `资质类型=${selectedSupplierOptionLabels(
                          supplierQualificationTypes,
                          SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
                      ).join("、")}`,
                  ]
                : []),
            ...(productKind
                ? [`商品类型=${PRODUCT_KIND_LABELS[productKind]}`]
                : []),
            ...(selectedCategoryLabel
                ? [`商品分类=${selectedCategoryLabel}`]
                : []),
            ...(selectedBrandLabel ? [`品牌=${selectedBrandLabel}`] : []),
            ...(selectedSupplierLabel
                ? [`供应商=${selectedSupplierLabel}`]
                : []),
            ...(productListingStatus
                ? [
                      `上架状态=${PRODUCT_LISTING_FILTER_OPTIONS.find((option) => option.value === productListingStatus)?.label}`,
                  ]
                : []),
            ...(productSupplyCoverage
                ? [
                      `供给覆盖=${PRODUCT_COVERAGE_FILTER_OPTIONS.find((option) => option.value === productSupplyCoverage)?.label}`,
                  ]
                : []),
            ...(productSalesPriceMin || productSalesPriceMax
                ? [
                      `销售价=${productSalesPriceMin ? `¥${productSalesPriceMin}` : "不限"}–${productSalesPriceMax ? `¥${productSalesPriceMax}` : "不限"}`,
                  ]
                : []),
            q.trim() ? `搜索=${q.trim()}` : "搜索=空",
        ]
        return parts.join(" · ")
    }, [
        lifecycleStatus,
        productKind,
        productListingStatus,
        productSalesPriceMax,
        productSalesPriceMin,
        productSupplyCoverage,
        q,
        resource,
        revisionTiming,
        isSupplierResource,
        supplierCapabilityCodes,
        supplierQualificationHealth,
        supplierQualificationTypes,
        selectedBrandLabel,
        selectedCategoryLabel,
        selectedSupplierLabel,
    ])

    const handleExport = React.useCallback(async () => {
        if (!listQuery.data || rows.length === 0) return
        const refreshed = await exportMutation.mutateAsync({
            resource,
            q: q.trim() || undefined,
            lifecycleStatus,
            revisionTiming,
            productKind,
            productCategoryId,
            productBrandId,
            productSupplierId,
            productListingStatus,
            productSupplyCoverage,
            productSalesPriceMin,
            productSalesPriceMax,
            supplierCapabilityCodes,
            supplierQualificationTypes,
            supplierQualificationHealth,
        })
        const exportRows = refreshed.rows
        if (exportRows.length === 0) return
        const csv = buildMasterDataExportCsv(exportRows, filterSnapshotLabel)
        downloadCsv(csv, `基础资料-${resourceLabel(resource)}`)
        const datePart = new Date().toISOString().slice(0, 10).replace(/-/g, "")
        setExportMeta({
            jobId: `导出-${datePart}-${String(Date.now() % 100000).padStart(5, "0")}`,
            rowCount: exportRows.length,
            filterSnapshotLabel,
        })
    }, [
        exportMutation,
        filterSnapshotLabel,
        lifecycleStatus,
        listQuery.data,
        productBrandId,
        productCategoryId,
        productKind,
        productListingStatus,
        productSupplierId,
        productSalesPriceMax,
        productSalesPriceMin,
        productSupplyCoverage,
        q,
        resource,
        revisionTiming,
        rows.length,
        supplierCapabilityCodes,
        supplierQualificationHealth,
        supplierQualificationTypes,
    ])

    const updateProductListing = React.useCallback(
        async (item: MasterDataListItem, listed: boolean) => {
            if (
                !listed &&
                !window.confirm(
                    `下架后，商品「${item.name}」下的全部 SKU 都会退出公司商品池。确定继续？`,
                )
            ) {
                return
            }
            setListingError(null)
            try {
                await productListingMutation.mutateAsync({
                    productId: item.stableId,
                    listingStatus: listed ? "LISTED" : "UNLISTED",
                })
            } catch (error) {
                setListingError(
                    getErrorMessage(error, "上架状态更新失败，请刷新后重试。"),
                )
            }
        },
        [productListingMutation],
    )

    const columns = useMasterDataColumns({
        isProductResource,
        isSupplierResource,
        isBrandResource,
        isUnitOfMeasureResource,
        isVoucherCategoryResource,
        isSellableResource,
        canUpdateProductListing,
        currentSupplySkuIds,
        lastFocusedRowId,
        productSkusByProduct,
        productSkusPending: productSkusQuery.isPending,
        productSkusError: productSkusQuery.isError,
        productListingPending: productListingMutation.isPending,
        productListingProductId: productListingMutation.variables?.productId,
        resource,
        rows,
        showEffectiveColumn,
        supplierOfferingsPending: supplierOfferingsQuery.isPending,
        supplierOfferingsError: supplierOfferingsQuery.isError,
        onUpdateProductListing: updateProductListing,
        onSupplyProduct: setSupplyProduct,
        onReviseTarget: setReviseTarget,
        onDisableTarget: setDisableTarget,
        onPreview: setPreviewId,
        onNavigate: router.push,
    })
    const isWarehouse = resource === "warehouses"

    if (listQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader
                    title={masterDataCopy.pageTitle(resourceLabel(resource))}
                />
                <ResourceNav resource={resource} navRef={navRef} />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    const listLoadFailed = listQuery.isError || !listQuery.data
    const hasActiveFilters =
        q.trim() !== "" ||
        lifecycleStatus !== "all" ||
        (!isSupplierResource && revisionTiming !== "all") ||
        Boolean(
            supplierQualificationHealth ||
            supplierCapabilityCodes.length ||
            supplierQualificationTypes.length,
        ) ||
        Boolean(
            productKind ||
            productCategoryId ||
            productBrandId ||
            productSupplierId ||
            productListingStatus ||
            productSupplyCoverage ||
            productSalesPriceMin ||
            productSalesPriceMax,
        )
    const metrics = isSupplierResource
        ? syncedMetrics.filter((metric) => metric.key !== "pending")
        : syncedMetrics
    const noDataWithCreate = !listLoadFailed && rows.length === 0

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={masterDataCopy.pageTitle(resourceLabel(resource))}
                breadcrumbs={[
                    {
                        id: "md",
                        label: "基础资料",
                        href: "/master-data",
                    },
                    {
                        id: "resource",
                        label: resourceLabel(resource),
                        current: true,
                    },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt="刚刚"
                        dateTime={listQuery.data?.queriedAt ?? ""}
                        state="fresh"
                        label="基础资料列表"
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "export",
                                label: masterDataCopy.actionExport,
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide" as const,
                                disabled: rows.length === 0,
                                onClick: handleExport,
                            },
                            ...(!isSellableResource
                                ? [
                                      {
                                          actionKey: "create",
                                          label: isWarehouse
                                              ? masterDataCopy.actionCreateClosed
                                              : masterDataCopy.actionCreate,
                                          mobileVisibility: "hide" as const,
                                          icon: PlusIcon,
                                          // 仓库写门禁未开放：按钮真正禁用，不再进入注定失败的表单。
                                          disabled: isWarehouse || !canCreate,
                                          title: isWarehouse
                                              ? masterDataCopy.warehouseWriteBody
                                              : !canCreate
                                                ? createBlockedReason
                                                : undefined,
                                          onClick: () => {
                                              if (
                                                  isProductResource ||
                                                  isSupplierResource
                                              ) {
                                                  router.push(
                                                      `/master-data/${resource}/new`,
                                                  )
                                              } else {
                                                  setCreateOpen(true)
                                              }
                                          },
                                      },
                                  ]
                                : []),
                        ]}
                    />
                }
            />

            {isWarehouse ? (
                <FormalActionResult
                    status="blocked"
                    title={masterDataCopy.warehouseWriteTitle}
                    description={masterDataCopy.warehouseWriteBody}
                    actions={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link href="/master-data/sellable-items" />
                                }
                            >
                                去公司商品池
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={<Link href="/inventory?view=balance" />}
                            >
                                打开库存台账
                            </Button>
                        </div>
                    }
                />
            ) : null}

            {resource === "brands" ? (
                <p className="text-sm text-muted-foreground">
                    {masterDataCopy.brandListHint}
                </p>
            ) : null}

            {resource === "unit-of-measures" ? (
                <p className="text-sm text-muted-foreground">
                    {masterDataCopy.unitListHint}
                </p>
            ) : null}

            {resource === "sellable-items" ? (
                <p className="text-sm text-muted-foreground">
                    {masterDataCopy.sellableItemsHint}
                </p>
            ) : null}

            {isProductResource && listingError ? (
                <p className="text-sm text-destructive" role="alert">
                    {listingError}
                </p>
            ) : null}

            {exportMeta ? (
                <BackgroundJobProgress
                    mode="all-or-nothing"
                    status="succeeded"
                    total={exportMeta.rowCount}
                    completed={exportMeta.rowCount}
                    succeeded={exportMeta.rowCount}
                    label={masterDataCopy.exportDone}
                    description={
                        <>
                            按当前筛选导出 {exportMeta.rowCount} 条。任务号{" "}
                            <span className="num">{exportMeta.jobId}</span>
                            。不含无权限查看的敏感信息。
                        </>
                    }
                />
            ) : null}

            {!isVoucherCategoryResource && metrics.length > 0 ? (
                <MetricStrip
                    columns={4}
                    aria-label={`${resourceLabel(resource)}${
                        isSupplierResource ? "指标" : "指标筛选"
                    }`}
                >
                    {metrics.map((metric) => {
                        const isLifecycleMetric =
                            metric.key === "all" ||
                            metric.key === "enabled" ||
                            metric.key === "disabled"
                        if (isSupplierResource || !isLifecycleMetric) {
                            // 供应商页使用显式提交筛选面板；指标只读展示，避免第二套即时筛选。
                            // 待生效更新属于版本状态维度（有独立筛选控件），同样只读展示。
                            return (
                                <MetricItem
                                    key={metric.key}
                                    label={metric.label}
                                    value={metric.value}
                                    detail={metric.detail}
                                />
                            )
                        }
                        return (
                            <MetricFilterItem
                                key={metric.key}
                                label={metric.label}
                                value={metric.value}
                                detail={metric.detail}
                                // metricKey 与 lifecycleStatus 同源写入；指标高亮只做展示，筛选由 lifecycleStatus 承担
                                active={metricKey === metric.key}
                                onClick={() =>
                                    changeLifecycle(
                                        metric.key as
                                            | "enabled"
                                            | "disabled"
                                            | "all",
                                    )
                                }
                            />
                        )
                    })}
                </MetricStrip>
            ) : null}

            <h2
                ref={resultsHeadingRef}
                tabIndex={-1}
                className="sr-only outline-none"
            >
                {resourceLabel(resource)} · {rows.length} 条结果
            </h2>

            <BusinessTableFrame
                title={`${resourceLabel(resource)}列表`}
                description={
                    isProductResource
                        ? masterDataCopy.productListDescription(rows.length)
                        : isSupplierResource
                          ? masterDataCopy.supplierListDescription(rows.length)
                          : masterDataCopy.listDescription(rows.length)
                }
                toolbar={
                    <MasterDataListToolbar
                        isProductResource={isProductResource}
                        isSupplierResource={isSupplierResource}
                        resource={resource}
                        searchInputRef={searchInputRef}
                        searchDraft={searchDraft}
                        setSearchDraft={setSearchDraft}
                        rowCount={rows.length}
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={clearAllFilters}
                        patchUrl={patchUrl}
                        resetPagination={resetPagination}
                        q={q}
                        lifecycleStatus={lifecycleStatus}
                        revisionTiming={revisionTiming}
                        changeLifecycle={changeLifecycle}
                        changeRevisionTiming={changeRevisionTiming}
                        productFilterPanelOpen={productFilterPanelOpen}
                        setProductFilterPanelOpen={setProductFilterPanelOpen}
                        hasStructuredProductFilters={
                            hasStructuredProductFilters
                        }
                        applyProductFilters={applyProductFilters}
                        productKindDraft={productKindDraft}
                        setProductKindDraft={setProductKindDraft}
                        lifecycleStatusDraft={lifecycleStatusDraft}
                        setLifecycleStatusDraft={setLifecycleStatusDraft}
                        revisionTimingDraft={revisionTimingDraft}
                        setRevisionTimingDraft={setRevisionTimingDraft}
                        productListingStatusDraft={productListingStatusDraft}
                        setProductListingStatusDraft={
                            setProductListingStatusDraft
                        }
                        productSupplyCoverageDraft={productSupplyCoverageDraft}
                        setProductSupplyCoverageDraft={
                            setProductSupplyCoverageDraft
                        }
                        productCategoryIdDraft={productCategoryIdDraft}
                        setProductCategoryIdDraft={setProductCategoryIdDraft}
                        productBrandIdDraft={productBrandIdDraft}
                        setProductBrandIdDraft={setProductBrandIdDraft}
                        productSupplierIdDraft={productSupplierIdDraft}
                        setProductSupplierIdDraft={setProductSupplierIdDraft}
                        productSalesPriceMinDraft={productSalesPriceMinDraft}
                        setProductSalesPriceMinDraft={
                            setProductSalesPriceMinDraft
                        }
                        productSalesPriceMaxDraft={productSalesPriceMaxDraft}
                        setProductSalesPriceMaxDraft={
                            setProductSalesPriceMaxDraft
                        }
                        productSalesPriceError={productSalesPriceError}
                        setProductSalesPriceError={setProductSalesPriceError}
                        productFilterOptionsQuery={productFilterOptionsQuery}
                        supplierFilterPanelOpen={supplierFilterPanelOpen}
                        setSupplierFilterPanelOpen={setSupplierFilterPanelOpen}
                        hasStructuredSupplierFilters={
                            hasStructuredSupplierFilters
                        }
                        applySupplierFilters={applySupplierFilters}
                        supplierQualificationHealthDraft={
                            supplierQualificationHealthDraft
                        }
                        setSupplierQualificationHealthDraft={
                            setSupplierQualificationHealthDraft
                        }
                        supplierCapabilityCodesDraft={
                            supplierCapabilityCodesDraft
                        }
                        setSupplierCapabilityCodesDraft={
                            setSupplierCapabilityCodesDraft
                        }
                        supplierQualificationTypesDraft={
                            supplierQualificationTypesDraft
                        }
                        setSupplierQualificationTypesDraft={
                            setSupplierQualificationTypesDraft
                        }
                    />
                }
                table={
                    <DataTable
                        data={pageRows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        rowCount={rows.length}
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            setPagination(next)
                            const page = next.pageIndex + 1
                            patchUrl({ page: page > 1 ? String(page) : null })
                        }}
                        layout="flush"
                        density="compact"
                        defaultColumnPinning={{
                            left: [isSellableResource ? "name" : "stableNo"],
                            right: isSellableResource ? [] : ["actions"],
                        }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={listQuery.error}
                                    onRetry={() => void listQuery.refetch()}
                                />
                            ) : undefined
                        }
                        emptyState={
                            noDataWithCreate ? (
                                <BusinessEmptyState
                                    kind={
                                        hasActiveFilters ? "filter" : "no-data"
                                    }
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title={
                                        hasActiveFilters
                                            ? "当前筛选无结果"
                                            : `还没有${resourceLabel(resource)}资料`
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "点击「新建」创建第一份资料；历史记录会随资料保留。"
                                    }
                                    action={
                                        !hasActiveFilters &&
                                        !isWarehouse &&
                                        !isSellableResource &&
                                        canCreate ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={() => {
                                                    if (
                                                        isProductResource ||
                                                        isSupplierResource
                                                    ) {
                                                        router.push(
                                                            `/master-data/${resource}/new`,
                                                        )
                                                    } else {
                                                        setCreateOpen(true)
                                                    }
                                                }}
                                            >
                                                {masterDataCopy.actionCreate}
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.stableId
                            if (isProductResource || isSupplierResource) {
                                router.push(
                                    `/master-data/${resource}/${row.stableId}?section=overview`,
                                )
                            } else if (
                                isBrandResource ||
                                isVoucherCategoryResource ||
                                isUnitOfMeasureResource
                            ) {
                                setReviseTarget(row)
                            } else {
                                setPreviewId(row.stableId)
                            }
                        }}
                        onRowOpen={(row) => {
                            lastFocusedRowId.current = row.stableId
                            if (isProductResource || isSupplierResource) {
                                router.push(
                                    `/master-data/${resource}/${row.stableId}?section=overview`,
                                )
                                return
                            }
                            if (
                                isBrandResource ||
                                isVoucherCategoryResource ||
                                isUnitOfMeasureResource
                            ) {
                                setReviseTarget(row)
                                return
                            }
                            setPreviewId(row.stableId)
                        }}
                    />
                }
            />
            <MasterDataPreviewSheet
                skipPreviewSheet={skipPreviewSheet}
                previewRow={previewRow}
                lastFocusedRowId={lastFocusedRowId}
                isSellableResource={isSellableResource}
                resource={resource}
                previewDetail={previewDetailQuery.data}
                previewDetailLoading={previewDetailQuery.isPending}
                onClose={() => setPreviewId(null)}
                onRevise={setReviseTarget}
                onDisable={setDisableTarget}
            />
            <ProductSupplyDialog
                product={supplyProduct}
                skus={
                    supplyProduct
                        ? (productSkusByProduct.get(supplyProduct.stableId) ??
                          [])
                        : []
                }
                skuLoading={productSkusQuery.isPending}
                skuError={productSkusQuery.error}
                offerings={supplierOfferingsQuery.data ?? []}
                offeringLoading={
                    productPageSkuIds.length > 0 &&
                    supplierOfferingsQuery.isPending
                }
                offeringError={supplierOfferingsQuery.error}
                onRetrySkus={() => void productSkusQuery.refetch()}
                onRetryOfferings={() => void supplierOfferingsQuery.refetch()}
                onAddSupply={(sku) => {
                    if (!supplyProduct) return
                    setSupplyDialogSku({
                        skuId: sku.skuId,
                        skuCode: sku.skuNo,
                        skuName: supplyProduct.name,
                        specification: sku.specification,
                        baseUnit: sku.baseUnit,
                        productKind: supplyProduct.productKind,
                    })
                }}
                onOpenChange={(open) => {
                    if (!open) setSupplyProduct(null)
                }}
            />

            {supplyDialogSku ? (
                <RegisterSupplyForSkuDialog
                    key={supplyDialogSku.skuId}
                    open
                    fixedSku={supplyDialogSku}
                    onOpenChange={(open) => {
                        if (!open) setSupplyDialogSku(null)
                    }}
                />
            ) : null}

            {!isProductResource &&
            !isSupplierResource &&
            !isVoucherCategoryResource &&
            !isSellableResource ? (
                <MasterDataCreateDialog
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    resource={resource}
                />
            ) : null}
            {isVoucherCategoryResource ? (
                <>
                    <VoucherCategoryFormDialog
                        open={createOpen}
                        onOpenChange={setCreateOpen}
                    />
                    <VoucherCategoryFormDialog
                        open={reviseTarget != null}
                        onOpenChange={(open) => {
                            if (!open) setReviseTarget(null)
                        }}
                        target={reviseTarget}
                    />
                </>
            ) : null}
            {!isProductResource &&
            !isSupplierResource &&
            !isVoucherCategoryResource &&
            !isSellableResource ? (
                <MasterDataReviseDialog
                    open={reviseTarget != null}
                    onOpenChange={(open) => {
                        if (!open) setReviseTarget(null)
                    }}
                    resource={resource}
                    target={reviseTarget}
                />
            ) : null}
            {!isVoucherCategoryResource && !isSellableResource ? (
                <MasterDataDisableDialog
                    open={disableTarget != null}
                    onOpenChange={(open) => {
                        if (!open) setDisableTarget(null)
                    }}
                    resource={resource}
                    target={disableTarget}
                />
            ) : null}
        </PageScaffold>
    )
}
