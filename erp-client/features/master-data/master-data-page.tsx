"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ArrowUpRightIcon,
  BanIcon,
  ChevronDownIcon,
  DownloadIcon,
  FilterIcon,
  HistoryIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  BackgroundJobProgress,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FixedOptionCheckboxFilter,
  FixedOptionRadioFilter,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
  PageScaffold,
  QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Switch } from "@/components/ui/switch"
import {
  buildMasterDataExportCsv,
  downloadCsv,
} from "@/features/master-data/queries"
import {
  masterDataCopy,
  masterDataSearchPlaceholder,
  lifecycleFilterLabel,
  revisionTimingFilterLabel,
} from "@/features/master-data/copy"
import { resourceLabel } from "@/features/master-data/data"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { formatEffectiveRange } from "@/features/master-data/filter"
import { CategoryTreePage } from "@/features/master-data/category-tree-page"
import {
  MasterDataCreateDialog,
  MasterDataDisableDialog,
  MasterDataReviseDialog,
} from "@/features/master-data/master-data-action-dialog"
import {
  MasterDataPreviewPanel,
  SellableItemPreviewPanel,
} from "@/features/master-data/master-data-preview"
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
  MASTER_DATA_RESOURCES,
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

const VALID = new Set(MASTER_DATA_RESOURCES.map((item) => item.key))

const CNY_FORMATTER = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
})

const PRODUCT_KIND_FILTER_OPTIONS = PRODUCT_KIND_VALUES.map((value) => ({
  value,
  label: PRODUCT_KIND_LABELS[value],
}))

const PRODUCT_KIND_RADIO_FILTER_OPTIONS = [
  { value: "all", label: "全部" },
  ...PRODUCT_KIND_FILTER_OPTIONS,
] as const

const PRODUCT_LISTING_FILTER_OPTIONS = [
  { value: "listed", label: "全部已上架" },
  { value: "partially_listed", label: "部分已上架" },
  { value: "unlisted", label: "全部未上架" },
] as const

const PRODUCT_COVERAGE_FILTER_OPTIONS = [
  { value: "complete", label: "全部 SKU 有供给" },
  { value: "partial", label: "部分 SKU 有供给" },
  { value: "none", label: "所有 SKU 均无供给" },
] as const

const PRODUCT_LISTING_RADIO_FILTER_OPTIONS = [
  { value: "all", label: "全部" },
  ...PRODUCT_LISTING_FILTER_OPTIONS,
] as const

const PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS = [
  { value: "all", label: "全部" },
  ...PRODUCT_COVERAGE_FILTER_OPTIONS,
] as const

const LIFECYCLE_RADIO_FILTER_OPTIONS = [
  { value: "all", label: "全部" },
  { value: "enabled", label: masterDataCopy.lifecycleEnabled },
  { value: "disabled", label: masterDataCopy.lifecycleDisabled },
] as const

const REVISION_TIMING_RADIO_FILTER_OPTIONS = [
  { value: "all", label: "全部" },
  { value: "current", label: "当前生效" },
  { value: "future", label: "待生效" },
] as const

const SUPPLIER_CAPABILITY_OPTIONS = [
  { value: "physical", label: "实物商品" },
  { value: "virtual", label: "虚拟商品" },
  { value: "offline_service", label: "线下服务" },
  { value: "api", label: "API" },
  { value: "printing", label: "印刷" },
] as const

const SUPPLIER_QUALIFICATION_TYPE_OPTIONS = [
  { value: "certificate", label: "资质证照" },
  { value: "contract", label: "合同" },
  { value: "authorization", label: "授权书" },
  { value: "food_license", label: "食品经营许可证" },
  { value: "legal_person_id", label: "法人身份证" },
] as const

const SUPPLIER_QUALIFICATION_HEALTH_OPTIONS = [
  { value: "all", label: "资质状态：全部" },
  { value: "valid", label: "资质状态：有效" },
  { value: "expiring_30", label: "资质状态：30 天内到期" },
  { value: "expired", label: "资质状态：已过期" },
  { value: "not_registered", label: "资质状态：未登记" },
] as const

/** 读取 URL 中逗号分隔的多选条件，去空、去重并固定排序。 */
function selectedCsvValues(value: string | null): string[] {
  if (!value) return []
  return [...new Set(value.split(",").map((item) => item.trim()).filter(Boolean))].sort()
}

/** 将多选条件压缩为 URL 与接口共用的逗号分隔值。 */
function csvFilterValue(values: readonly string[]): string | null {
  const normalized = [...new Set(values.map((item) => item.trim()).filter(Boolean))].sort()
  return normalized.length > 0 ? normalized.join(",") : null
}

/** 返回资质状态的业务文案。 */
function qualificationHealthLabel(value: SupplierQualificationHealth | undefined): string {
  return SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.find(
    (option) => option.value === value
  )?.label.replace("资质状态：", "") ?? "全部"
}

/** 仅保留当前页面已声明的多选枚举值，避免 URL 中的无效值成为隐形状态。 */
function selectedSupplierOptionValues(
  value: string | null,
  options: readonly { value: string; label: string }[]
): string[] {
  return selectedCsvValues(value).filter((item) =>
    options.some((option) => option.value === item)
  )
}

/** 把已选固定枚举代码转换为业务文案，用于导出筛选摘要。 */
function selectedSupplierOptionLabels(
  values: readonly string[],
  options: readonly { value: string; label: string }[]
): string[] {
  return values.flatMap((value) => {
    const label = options.find((option) => option.value === value)?.label
    return label ? [label] : []
  })
}

/** 校验销售价输入，并使用分值整数比较上下界，避免浮点误差。 */
function productSalesPriceRangeError(
  minimum: string,
  maximum: string
): string | null {
  const pricePattern = /^\d+(?:\.\d{1,2})?$/
  if (minimum && !pricePattern.test(minimum)) {
    return "最低价应为最多两位小数的非负金额"
  }
  if (maximum && !pricePattern.test(maximum)) {
    return "最高价应为最多两位小数的非负金额"
  }
  const normalizedParts = (value: string): readonly [string, string] => {
    const [yuan, fraction = ""] = value.split(".")
    return [yuan.replace(/^0+(?=\d)/, ""), fraction.padEnd(2, "0")]
  }
  if (minimum && maximum) {
    const [minimumYuan, minimumFraction] = normalizedParts(minimum)
    const [maximumYuan, maximumFraction] = normalizedParts(maximum)
    const minimumIsHigher =
      minimumYuan.length > maximumYuan.length ||
      (minimumYuan.length === maximumYuan.length &&
        (minimumYuan > maximumYuan ||
          (minimumYuan === maximumYuan &&
            minimumFraction > maximumFraction)))
    if (minimumIsHigher) return "最低价不能高于最高价"
  }
  return null
}

function productSkuPriceRange(
  skus: readonly ProductListSkuSummary[],
): string {
  const prices = skus
    .flatMap((sku) => {
      const raw = sku.salesVisiblePriceGross?.trim()
      if (!raw) return []
      const price = Number(raw)
      return Number.isFinite(price) ? [price] : []
    })
    .sort((left, right) => left - right)
  if (prices.length === 0) return "未填写"
  const minimum = CNY_FORMATTER.format(prices[0])
  const maximum = CNY_FORMATTER.format(prices[prices.length - 1])
  return prices[0] === prices[prices.length - 1]
    ? minimum
    : `${minimum}–${maximum}`
}

const CREATE_PERMISSION_BY_RESOURCE: Partial<
  Record<MasterDataResource, string>
> = {
  products: "product:create",
  categories: "product_category:create",
  brands: "product_brand:create",
  "unit-of-measures": "unit_of_measure:create",
  "voucher-categories": "voucher_category_profile:create",
  suppliers: "supplier:create",
  warehouses: "warehouse:create",
}

function isResource(value: string): value is MasterDataResource {
  return VALID.has(value as MasterDataResource)
}

function ResourceNav({
  resource,
  navRef,
}: {
  resource: string
  navRef: React.RefObject<HTMLElement | null>
}) {
  return (
    <nav
      ref={navRef}
      aria-label={masterDataCopy.resourceNavAria}
      className="flex flex-wrap gap-2 border-b border-border/30 pb-3"
    >
      {MASTER_DATA_RESOURCES.map((item) => {
        const selected = item.key === resource
        return (
          <Button
            key={item.key}
            size="sm"
            aria-current={selected ? "page" : undefined}
            variant={selected ? "secondary" : "ghost"}
            render={<Link href={`/master-data/${item.key}`} />}
          >
            {item.label}
          </Button>
        )
      })}
    </nav>
  )
}

/** 禁用按钮的阻断原因提示：disabled 状态下浏览器不显示 title，用外层 span 承载。 */
function DisabledActionHint({
  message,
  children,
}: {
  message?: string
  children: React.ReactNode
}) {
  return message ? (
    <span title={message} className="inline-flex">
      {children}
    </span>
  ) : (
    <>{children}</>
  )
}

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
        if (document.querySelector('[role="dialog"], [data-slot="sheet"]')) {
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
    lifecycleStatusParam === "enabled" || lifecycleStatusParam === "disabled"
      ? lifecycleStatusParam
      : "all"
  const revisionTimingParam = searchParams.get("revisionTiming")
  const revisionTiming: "current" | "future" | "all" =
    revisionTimingParam === "current" || revisionTimingParam === "future"
      ? revisionTimingParam
      : "all"
  const productKind = isProductResource
    ? PRODUCT_KIND_VALUES.find(
        (value) => value === searchParams.get("productKind")
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
        (option) => option.value === searchParams.get("productListingStatus")
      )?.value
    : undefined
  const productSupplyCoverage = isProductResource
    ? PRODUCT_COVERAGE_FILTER_OPTIONS.find(
        (option) => option.value === searchParams.get("productSupplyCoverage")
      )?.value
    : undefined
  const productSalesPriceMin = isProductResource
    ? searchParams.get("productSalesPriceMin")?.trim() || undefined
    : undefined
  const productSalesPriceMax = isProductResource
    ? searchParams.get("productSalesPriceMax")?.trim() || undefined
    : undefined
  const supplierCapabilityCodesParam = searchParams.get("supplierCapabilityCodes")
  const supplierQualificationTypesParam = searchParams.get(
    "supplierQualificationTypes"
  )
  const supplierCapabilityCodes = React.useMemo(
    () =>
      isSupplierResource
        ? selectedSupplierOptionValues(
            supplierCapabilityCodesParam,
            SUPPLIER_CAPABILITY_OPTIONS
          )
        : [],
    [isSupplierResource, supplierCapabilityCodesParam]
  )
  const supplierQualificationTypes = React.useMemo(
    () =>
      isSupplierResource
        ? selectedSupplierOptionValues(
            supplierQualificationTypesParam,
            SUPPLIER_QUALIFICATION_TYPE_OPTIONS
          )
        : [],
    [isSupplierResource, supplierQualificationTypesParam]
  )
  const supplierQualificationHealthParam = searchParams.get(
    "supplierQualificationHealth"
  )
  const supplierQualificationHealth = isSupplierResource
    ? (SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.find(
        (option) =>
          option.value !== "all" &&
          option.value === supplierQualificationHealthParam
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
      productSalesPriceMax
  )
  const hasStructuredSupplierFilters = Boolean(
    lifecycleStatus !== "all" ||
      supplierQualificationHealth ||
      supplierCapabilityCodes.length ||
      supplierQualificationTypes.length
  )
  const pageParamRaw = Number(searchParams.get("page"))
  const pageParamIndex =
    Number.isFinite(pageParamRaw) && pageParamRaw > 0
      ? Math.max(0, Math.floor(pageParamRaw) - 1)
      : 0

  const [searchDraft, setSearchDraft] = React.useState(q)
  /** 高级筛选面板展开态：本地 UI 状态，不入 URL；有已生效筛选时默认展开。 */
  const [productFilterPanelOpen, setProductFilterPanelOpen] = React.useState(
    hasStructuredProductFilters
  )
  /** 供应商显式提交筛选面板展开态；深链带入条件时自动展开。 */
  const [supplierFilterPanelOpen, setSupplierFilterPanelOpen] =
    React.useState(hasStructuredSupplierFilters)
  const [supplierCapabilityCodesDraft, setSupplierCapabilityCodesDraft] =
    React.useState<string[]>(supplierCapabilityCodes)
  const [supplierQualificationTypesDraft, setSupplierQualificationTypesDraft] =
    React.useState<string[]>(supplierQualificationTypes)
  const [supplierQualificationHealthDraft, setSupplierQualificationHealthDraft] =
    React.useState<SupplierQualificationHealth | "all">(
      supplierQualificationHealth ?? "all"
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
    React.useState<ProductListingFilter | "all">(productListingStatus ?? "all")
  const [productSupplyCoverageDraft, setProductSupplyCoverageDraft] =
    React.useState<ProductSkuCoverageFilter | "all">(
      productSupplyCoverage ?? "all"
    )
  const [productCategoryIdDraft, setProductCategoryIdDraft] = React.useState<
    string | null
  >(productCategoryId ?? null)
  const [productBrandIdDraft, setProductBrandIdDraft] = React.useState<
    string | null
  >(productBrandId ?? null)
  const [productSupplierIdDraft, setProductSupplierIdDraft] =
    React.useState<string | null>(productSupplierId ?? null)
  const [productSalesPriceMinDraft, setProductSalesPriceMinDraft] =
    React.useState(productSalesPriceMin ?? "")
  const [productSalesPriceMaxDraft, setProductSalesPriceMaxDraft] =
    React.useState(productSalesPriceMax ?? "")
  const [productSalesPriceError, setProductSalesPriceError] =
    React.useState<string | null>(null)
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
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    },
    [pathname, router, searchParams]
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
    [lifecycleStatus, patchUrl, resetPagination]
  )

  const changeRevisionTiming = React.useCallback(
    (next: "current" | "future" | "all") => {
      if (next === revisionTiming) return
      patchUrl({ revisionTiming: next === "all" ? null : next, page: null })
      resetPagination()
    },
    [patchUrl, resetPagination, revisionTiming]
  )

  /** 提交供应商筛选面板；全部草稿字段一次性写入 URL。 */
  const applySupplierFilters = React.useCallback(() => {
    patchUrl({
      q: searchDraft.trim() || null,
      lifecycleStatus:
        lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
      metricKey: lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
      supplierCapabilityCodes: csvFilterValue(supplierCapabilityCodesDraft),
      supplierQualificationTypes: csvFilterValue(supplierQualificationTypesDraft),
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
      metricKey: lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
      revisionTiming:
        revisionTimingDraft === "all" ? null : revisionTimingDraft,
      productListingStatus:
        productListingStatusDraft === "all" ? null : productListingStatusDraft,
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
    setSupplierQualificationHealthDraft(supplierQualificationHealth ?? "all")
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
    [listQuery.data?.rows]
  )

  const previewDetailQuery = useMasterDataCenterQuery(
    resource,
    isSellableResource ? "" : previewId ?? ""
  )

  const previewRow = React.useMemo(
    () => rows.find((r) => r.stableId === previewId) ?? null,
    [previewId, rows]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return rows.slice(start, start + pagination.pageSize)
  }, [pagination.pageIndex, pagination.pageSize, rows])

  const productPageIds = React.useMemo(
    () =>
      isProductResource ? pageRows.map((row) => row.stableId) : [],
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
        (option) => option.value === productCategoryId
      )?.label ?? productCategoryId,
    [productCategoryId, productFilterOptionsQuery.data?.categories]
  )
  const selectedBrandLabel = React.useMemo(
    () =>
      productFilterOptionsQuery.data?.brands.find(
        (option) => option.value === productBrandId
      )?.label ?? productBrandId,
    [productBrandId, productFilterOptionsQuery.data?.brands]
  )
  const selectedSupplierLabel = React.useMemo(
    () =>
      (productFilterOptionsQuery.data?.suppliers ?? []).find(
        (option) => option.value === productSupplierId
      )?.label ?? productSupplierId,
    [productFilterOptionsQuery.data?.suppliers, productSupplierId]
  )
  /** 指标与当前搜索/启停/版本筛选同步，避免「全部 3」与表格行数矛盾。 */
  const syncedMetrics = React.useMemo(() => {
    const base = listQuery.data?.metrics ?? []
    if (rows.length === 0 || listQuery.data == null) return base
    const metricCount = (key: string): number => {
      switch (key) {
        case "enabled":
          return rows.filter((r) => r.lifecycleStatus === "ENABLED").length
        case "disabled":
          return rows.filter((r) => r.lifecycleStatus === "DISABLED").length
        case "pending":
          return rows.filter((r) => r.revisionTiming === "FUTURE").length
        case "expiring":
          return rows.filter((r) => r.metricTags.includes("expiring")).length
        default:
          return rows.length
      }
    }
    return base.map((metric) => ({ ...metric, value: metricCount(metric.key) }))
  }, [listQuery.data, rows])

  const filterSnapshotLabel = React.useMemo(() => {
    const parts = [
      `分类=${resourceLabel(resource)}`,
      `启用状态=${lifecycleFilterLabel(lifecycleStatus)}`,
      ...(isSupplierResource
        ? [`资质状态=${qualificationHealthLabel(supplierQualificationHealth)}`]
        : [`版本状态=${revisionTimingFilterLabel(revisionTiming)}`]),
      ...(supplierCapabilityCodes.length > 0
        ? [
            `供应能力=${selectedSupplierOptionLabels(
              supplierCapabilityCodes,
              SUPPLIER_CAPABILITY_OPTIONS
            ).join("、")}`,
          ]
        : []),
      ...(supplierQualificationTypes.length > 0
        ? [
            `资质类型=${selectedSupplierOptionLabels(
              supplierQualificationTypes,
              SUPPLIER_QUALIFICATION_TYPE_OPTIONS
            ).join("、")}`,
          ]
        : []),
      ...(productKind ? [`商品类型=${PRODUCT_KIND_LABELS[productKind]}`] : []),
      ...(selectedCategoryLabel ? [`商品分类=${selectedCategoryLabel}`] : []),
      ...(selectedBrandLabel ? [`品牌=${selectedBrandLabel}`] : []),
      ...(selectedSupplierLabel ? [`供应商=${selectedSupplierLabel}`] : []),
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

  const columns = React.useMemo<ColumnDef<MasterDataListItem>[]>(
    () => [
      ...(!isSellableResource && !isSupplierResource
        ? [
            {
              id: "stableNo",
              accessorKey: "stableNo",
              header: masterDataCopy.colStableNo,
              meta: {
                label: masterDataCopy.colStableNo,
                width: "default" as const,
              },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <span className="num text-sm">{row.original.stableNo}</span>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
          ]
        : []),
      {
        id: "name",
        accessorKey: "name",
        header: isSellableResource ? "商品名称 · 规格" : masterDataCopy.colName,
        meta: {
          label: isSellableResource ? "商品名称 · 规格" : masterDataCopy.colName,
        },
        cell: ({ row }) => {
          const sellable = row.original.sellableItem
          return (
            <div className="min-w-0">
              <div className="truncate text-sm font-medium">
                {row.original.name}
                {sellable ? (
                  <span className="text-muted-foreground">
                    {" "}· {sellable.specificationLabel}
                  </span>
                ) : null}
              </div>
              {sellable ? (
                <div className="truncate text-xs text-muted-foreground">
                  SKU 编号：
                  <span className="num">{row.original.stableNo}</span>
                </div>
              ) : row.original.keyFacts[0] ? (
                <div className="truncate text-xs text-muted-foreground">
                  {row.original.keyFacts[0].label}：
                  {row.original.keyFacts[0].value}
                </div>
              ) : null}
            </div>
          )
        },
      },
      ...(isSellableResource
        ? [
            {
              id: "productNo",
              header: "SPU 编号",
              meta: { label: "SPU 编号", width: "default" as const },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <span className="num text-sm">
                  {row.original.sellableItem?.productNo ?? "—"}
                </span>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
            {
              id: "price",
              header: "价格",
              meta: { label: "价格", width: "amount" as const },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <div className="flex flex-col gap-0.5">
                  <MoneyValue
                    value={row.original.sellableItem?.salesVisiblePriceGross}
                  />
                  <span className="text-tiny text-muted-foreground">
                    销售价 · 含税
                  </span>
                </div>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
            {
              id: "supplyRegions",
              header: "可供区域",
              meta: { label: "可供区域" },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => {
                const regions = row.original.sellableItem?.supplyRegions ?? []
                const label = regions.length > 0 ? regions.join("、") : "未标注"
                return (
                  <span className="line-clamp-2 max-w-64 text-sm" title={label}>
                    {label}
                  </span>
                )
              },
            } satisfies ColumnDef<MasterDataListItem>,
            {
              id: "supplierCount",
              header: "有效供应商",
              meta: { label: "有效供应商", width: "status" as const },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <Badge variant="outline">
                  <span className="num">
                    {row.original.sellableItem?.supplierCount ?? 0}
                  </span>{" "}
                  家
                </Badge>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
          ]
        : [
            {
              id: "revisionNo",
              header: masterDataCopy.colVersion,
              meta: {
                label: masterDataCopy.colVersion,
                width: "amount" as const,
              },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <span className="num text-sm">v{row.original.revisionNo}</span>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
            {
              id: "lifecycle",
              header: masterDataCopy.colLifecycle,
              meta: { label: masterDataCopy.colLifecycle },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <div className="flex flex-col gap-1">
                  <BusinessStatusBadge
                    context="list"
                    label={row.original.lifecycleStatusLabel}
                    tone={row.original.lifecycleTone}
                  />
                  {row.original.scheduledLifecycleLabel ? (
                    <span className="text-tiny text-muted-foreground">
                      {row.original.scheduledLifecycleLabel}
                    </span>
                  ) : null}
                </div>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
          ]),
      ...(isProductResource
        ? [
            {
              id: "skuPriceRange",
              header: "SKU 售价",
              meta: { label: "SKU 售价", width: "amount" as const },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <span className="num text-sm">
                  {productSkusQuery.isPending
                    ? "读取中…"
                    : productSkusQuery.isError
                      ? "暂不可查"
                      : productSkuPriceRange(
                          productSkusByProduct.get(row.original.stableId) ?? [],
                        )}
                </span>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
            {
              id: "skuCount",
              header: "SKU 数量",
              meta: { label: "SKU 数量", width: "amount" as const },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <span className="num text-sm">
                  {row.original.skuCount ?? 0} 个
                </span>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
            {
              id: "supply",
              header: "供给",
              meta: { label: "供给", width: "status" as const },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => {
                const item = row.original
                const productSkus =
                  productSkusByProduct.get(item.stableId) ?? []
                const suppliedSkuCount = productSkus.filter((sku) =>
                  currentSupplySkuIds.has(sku.skuId),
                ).length
                const skuDataPending = productSkusQuery.isPending
                const skuDataFailed = productSkusQuery.isError
                const offeringPending =
                  productSkus.length > 0 && supplierOfferingsQuery.isPending
                const offeringFailed =
                  productSkus.length > 0 && supplierOfferingsQuery.isError
                const statusLabel = skuDataPending
                  ? "读取中…"
                  : skuDataFailed || offeringFailed
                    ? "暂不可查"
                    : suppliedSkuCount > 0
                      ? "有供给"
                      : "无供给"
                return (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    className="h-auto gap-1.5 px-1 py-0.5"
                    aria-label={`${item.name}供给详情：${statusLabel}`}
                    onClick={(event) => {
                      event.stopPropagation()
                      lastFocusedRowId.current = item.stableId
                      setSupplyProduct(item)
                    }}
                  >
                    <Badge
                      variant={
                        suppliedSkuCount > 0 &&
                        !skuDataPending &&
                        !offeringPending &&
                        !skuDataFailed &&
                        !offeringFailed
                          ? "success"
                          : "outline"
                      }
                    >
                      {offeringPending ? "读取中…" : statusLabel}
                    </Badge>
                    {!skuDataPending &&
                    !skuDataFailed &&
                    !offeringPending &&
                    !offeringFailed &&
                    productSkus.length > 0 ? (
                      <span className="num text-xs text-muted-foreground">
                        {suppliedSkuCount}/{productSkus.length} SKU
                      </span>
                    ) : null}
                  </Button>
                )
              },
            } satisfies ColumnDef<MasterDataListItem>,
            {
              id: "listing",
              header: "上架状态",
              meta: { label: "上架状态" },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => {
                const item = row.original
                const inherited = item.listingStatus ?? "UNLISTED"
                const pending =
                  productListingMutation.isPending &&
                  productListingMutation.variables?.productId === item.stableId
                const label =
                  inherited === "LISTED"
                    ? "已上架"
                    : inherited === "PARTIALLY_LISTED"
                      ? "部分上架"
                      : "已下架"
                return (
                  <div className="flex items-center gap-2">
                    <Switch
                      size="sm"
                      checked={inherited === "LISTED"}
                      disabled={
                        pending ||
                        !canUpdateProductListing ||
                        (item.lifecycleStatus !== "ENABLED" &&
                          inherited === "UNLISTED") ||
                        (item.skuCount ?? 0) === 0
                      }
                      onCheckedChange={(checked) =>
                        void updateProductListing(item, checked)
                      }
                      aria-label={`${item.name}整组上架状态`}
                    />
                    <span className="whitespace-nowrap text-xs text-muted-foreground">
                      {pending
                        ? "更新中…"
                        : `${label} ${item.listedSkuCount ?? 0}/${item.skuCount ?? 0}`}
                    </span>
                  </div>
                )
              },
            } satisfies ColumnDef<MasterDataListItem>,
          ]
        : []),
      ...(!isSellableResource
        ? [
            {
              id: "revisionTiming",
              header: masterDataCopy.colVersionState,
              meta: { label: masterDataCopy.colVersionState },
              cell: ({ row }: { row: { original: MasterDataListItem } }) => (
                <Badge
                  variant={
                    row.original.revisionTiming === "FUTURE"
                      ? "warning"
                      : "secondary"
                  }
                >
                  {row.original.revisionTimingLabel}
                </Badge>
              ),
            } satisfies ColumnDef<MasterDataListItem>,
          ]
        : []),
    ...(showEffectiveColumn
      ? [
          {
            id: "period",
            header: masterDataCopy.colEffective,
            meta: {
              label: masterDataCopy.colEffective,
            },
            cell: ({ row }: { row: { original: MasterDataListItem } }) => (
              <span className="num text-xs">
                {formatEffectiveRange(
                  row.original.effectiveFrom,
                  row.original.effectiveTo
                )}
              </span>
            ),
          } satisfies ColumnDef<MasterDataListItem>,
        ]
      : []),
      ...(rows.some((r) => r.primaryBlocker)
        ? [
            {
              id: "blocker",
              header: masterDataCopy.colBlocker,
              meta: { label: masterDataCopy.colBlocker },
              cell: ({ row }: { row: { original: MasterDataListItem } }) =>
                row.original.primaryBlocker ? (
                  <span className="text-xs text-destructive">
                    {row.original.primaryBlocker}
                  </span>
                ) : (
                  <span className="text-xs text-muted-foreground">—</span>
                ),
            } satisfies ColumnDef<MasterDataListItem>,
          ]
        : []),
      ...(!isSellableResource
        ? [
            {
              id: "actions",
              header: masterDataCopy.colActions,
              meta: { label: masterDataCopy.colActions },
              cell: ({ row }) => {
                const item = row.original
                const canRevise = item.allowedActions.includes("CREATE_REVISION")
                const canDisable = item.allowedActions.includes("DISABLE")
                const reviseBlocker = item.actionBlockers.find(
                  (b) => b.action === "CREATE_REVISION"
                )
                const disableBlocker = item.actionBlockers.find(
                  (b) => b.action === "DISABLE"
                )
                // 卡券类目：仅原地编辑。
                if (isVoucherCategoryResource) {
                  return (
                    <div className="flex flex-wrap gap-1">
                      <DisabledActionHint message={reviseBlocker?.message}>
                        <Button
                          type="button"
                          size="xs"
                          variant="ghost"
                          disabled={!canRevise}
                          title={reviseBlocker?.message}
                          onClick={(e) => {
                            e.stopPropagation()
                            lastFocusedRowId.current = item.stableId
                            setReviseTarget(item)
                          }}
                        >
                          <HistoryIcon data-icon="inline-start" aria-hidden />
                          {masterDataCopy.actionUpdate}
                        </Button>
                      </DisabledActionHint>
                    </div>
                  )
                }
                // 商品点击行进入详情；品牌 / 计量单位点击行打开更新 Dialog。操作列均仅保留「停用」。
                if (
                  isProductResource ||
                  isBrandResource ||
                  isUnitOfMeasureResource
                ) {
                  return (
                    <div className="flex flex-wrap gap-1">
                      <DisabledActionHint message={disableBlocker?.message}>
                        <Button
                          type="button"
                          size="xs"
                          variant="ghost"
                          disabled={!canDisable}
                          title={disableBlocker?.message}
                          onClick={(e) => {
                            e.stopPropagation()
                            lastFocusedRowId.current = item.stableId
                            setDisableTarget(item)
                          }}
                        >
                          <BanIcon data-icon="inline-start" aria-hidden />
                          {masterDataCopy.actionDisable}
                        </Button>
                      </DisabledActionHint>
                    </div>
                  )
                }
                return (
                  <div className="flex flex-wrap gap-1">
                    <Button
                      type="button"
                      size="xs"
                      variant="ghost"
                      onClick={(e) => {
                        e.stopPropagation()
                        lastFocusedRowId.current = item.stableId
                        if (isSupplierResource) {
                          router.push(
                            `/master-data/${resource}/${item.stableId}?section=overview`
                          )
                        } else {
                          setPreviewId(item.stableId)
                        }
                      }}
                    >
                      {masterDataCopy.actionView}
                    </Button>
                    <DisabledActionHint message={reviseBlocker?.message}>
                      <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        disabled={!canRevise}
                        title={reviseBlocker?.message}
                        onClick={(e) => {
                          e.stopPropagation()
                          if (isSupplierResource) {
                            // 详情页即编辑，与「查看」同一路由
                            router.push(
                              `/master-data/${resource}/${item.stableId}?section=overview`
                            )
                          } else {
                            setReviseTarget(item)
                          }
                        }}
                      >
                        <HistoryIcon data-icon="inline-start" aria-hidden />
                        {masterDataCopy.actionUpdate}
                      </Button>
                    </DisabledActionHint>
                    <DisabledActionHint message={disableBlocker?.message}>
                      <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        disabled={!canDisable}
                        title={disableBlocker?.message}
                        onClick={(e) => {
                          e.stopPropagation()
                          setDisableTarget(item)
                        }}
                      >
                        <BanIcon data-icon="inline-start" aria-hidden />
                        {masterDataCopy.actionDisable}
                      </Button>
                    </DisabledActionHint>
                  </div>
                )
              },
            } satisfies ColumnDef<MasterDataListItem>,
          ]
        : []),
    ],
    [
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
      productSkusQuery.isError,
      productSkusQuery.isPending,
      productListingMutation.isPending,
      productListingMutation.variables,
      resource,
      router,
      rows,
      showEffectiveColumn,
      supplierOfferingsQuery.isError,
      supplierOfferingsQuery.isPending,
      updateProductListing,
    ]
  )

  const isWarehouse = resource === "warehouses"

  if (listQuery.isPending) {
    return (
      <PageScaffold density="compact">
        <PageHeader title={masterDataCopy.pageTitle(resourceLabel(resource))} />
        <ResourceNav resource={resource} navRef={navRef} />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
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
        supplierQualificationTypes.length
    ) ||
    Boolean(
      productKind ||
      productCategoryId ||
      productBrandId ||
      productSupplierId ||
      productListingStatus ||
      productSupplyCoverage ||
      productSalesPriceMin ||
      productSalesPriceMax
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
              ...(!isSellableResource ? [{
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
                  if (isProductResource || isSupplierResource) {
                    router.push(`/master-data/${resource}/new`)
                  } else {
                    setCreateOpen(true)
                  }
                },
              }] : []),
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
                render={<Link href="/master-data/sellable-items" />}
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
                    metric.key as "enabled" | "disabled" | "all"
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
          isProductResource ? (
            <form
              onSubmit={(event) => {
                event.preventDefault()
                applyProductFilters()
              }}
            >
              <ListToolbar
                search={
                  <InputGroup>
                    <InputGroupAddon>
                      <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                      ref={searchInputRef}
                      value={searchDraft}
                      onChange={(e) => setSearchDraft(e.target.value)}
                      placeholder={masterDataSearchPlaceholder(resource)}
                      aria-label={masterDataCopy.searchAria}
                    />
                  </InputGroup>
                }
                filters={
                  <>
                    {!productFilterPanelOpen ? (
                      <Button type="submit" size="sm">
                        <SearchIcon
                          data-icon="inline-start"
                          aria-hidden="true"
                        />
                        搜索
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      aria-expanded={productFilterPanelOpen}
                      onClick={() =>
                        setProductFilterPanelOpen((open) => !open)
                      }
                    >
                      <FilterIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                      />
                      高级筛选
                      {hasStructuredProductFilters ? (
                        <Badge variant="info">已启用</Badge>
                      ) : null}
                      <ChevronDownIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                        className={
                          productFilterPanelOpen
                            ? "rotate-180 transition-transform"
                            : "transition-transform"
                        }
                      />
                    </Button>
                  </>
                }
                secondary={
                  productFilterPanelOpen ? (
                    <div
                      className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                      aria-label="商品筛选条件"
                    >
                      <FixedOptionRadioFilter
                        label="类型"
                        value={productKindDraft}
                        onValueChange={setProductKindDraft}
                        options={PRODUCT_KIND_RADIO_FILTER_OPTIONS}
                      />
                      <FixedOptionRadioFilter
                        label="启停"
                        value={lifecycleStatusDraft}
                        onValueChange={setLifecycleStatusDraft}
                        options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                        aria-label={masterDataCopy.filterLifecycleAria}
                      />
                      <FixedOptionRadioFilter
                        label="版本"
                        value={revisionTimingDraft}
                        onValueChange={setRevisionTimingDraft}
                        options={REVISION_TIMING_RADIO_FILTER_OPTIONS}
                        aria-label={masterDataCopy.filterVersionAria}
                      />
                      <FixedOptionRadioFilter
                        label="上架"
                        value={productListingStatusDraft}
                        onValueChange={setProductListingStatusDraft}
                        options={PRODUCT_LISTING_RADIO_FILTER_OPTIONS}
                      />
                      <FixedOptionRadioFilter
                        label="供给覆盖"
                        value={productSupplyCoverageDraft}
                        onValueChange={setProductSupplyCoverageDraft}
                        options={PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS}
                      />

                      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                        <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                          <span className="text-muted-foreground">分类</span>
                          <OptionCombobox
                            className="w-full"
                            value={productCategoryIdDraft}
                            aria-label="商品分类"
                            onValueChange={setProductCategoryIdDraft}
                            options={
                              productFilterOptionsQuery.data?.categories ?? []
                            }
                            loading={productFilterOptionsQuery.isPending}
                            placeholder="全部分类"
                            searchPlaceholder="搜索分类名称或代码"
                          />
                        </label>
                        <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                          <span className="text-muted-foreground">品牌</span>
                          <OptionCombobox
                            className="w-full"
                            value={productBrandIdDraft}
                            aria-label="商品品牌"
                            onValueChange={setProductBrandIdDraft}
                            options={
                              productFilterOptionsQuery.data?.brands ?? []
                            }
                            loading={productFilterOptionsQuery.isPending}
                            placeholder="全部品牌"
                            searchPlaceholder="搜索品牌名称或代码"
                          />
                        </label>
                        <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                          <span className="text-muted-foreground">
                            供应商
                          </span>
                          <OptionCombobox
                            className="w-full"
                            value={productSupplierIdDraft}
                            aria-label="供应商"
                            onValueChange={setProductSupplierIdDraft}
                            options={
                              productFilterOptionsQuery.data?.suppliers ?? []
                            }
                            loading={productFilterOptionsQuery.isPending}
                            placeholder="全部供应商"
                            searchPlaceholder="搜索供应商名称或代码"
                          />
                        </label>
                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                          <span className="text-muted-foreground">
                            销售价
                          </span>
                          <div className="flex items-center gap-1.5">
                            <Input
                              className="w-0 min-w-0 flex-1"
                              value={productSalesPriceMinDraft}
                              onChange={(event) => {
                                setProductSalesPriceMinDraft(
                                  event.target.value
                                )
                                setProductSalesPriceError(null)
                              }}
                              inputMode="decimal"
                              autoComplete="off"
                              placeholder="最低价"
                              aria-label="最低销售价"
                              aria-invalid={Boolean(productSalesPriceError)}
                              aria-describedby="product-sales-price-error"
                            />
                            <span className="text-muted-foreground">至</span>
                            <Input
                              className="w-0 min-w-0 flex-1"
                              value={productSalesPriceMaxDraft}
                              onChange={(event) => {
                                setProductSalesPriceMaxDraft(
                                  event.target.value
                                )
                                setProductSalesPriceError(null)
                              }}
                              inputMode="decimal"
                              autoComplete="off"
                              placeholder="最高价"
                              aria-label="最高销售价"
                              aria-invalid={Boolean(productSalesPriceError)}
                              aria-describedby="product-sales-price-error"
                            />
                          </div>
                          {productSalesPriceError ? (
                            <span
                              id="product-sales-price-error"
                              className="text-xs text-destructive"
                              role="alert"
                            >
                              {productSalesPriceError}
                            </span>
                          ) : null}
                        </div>
                      </div>

                      <div className="flex justify-end">
                        <Button type="submit" size="sm">
                          <SearchIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                          />
                          搜索
                        </Button>
                      </div>
                    </div>
                  ) : undefined
                }
                actions={
                  <>
                    <span
                      className="text-xs text-muted-foreground"
                      aria-live="polite"
                    >
                      {resourceLabel(resource)} · {rows.length} 条
                    </span>
                    {hasActiveFilters ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={clearAllFilters}
                      >
                        清除筛选
                      </Button>
                    ) : null}
                  </>
                }
              />
            </form>
          ) : isSupplierResource ? (
            <form
              onSubmit={(event) => {
                event.preventDefault()
                applySupplierFilters()
              }}
            >
              <ListToolbar
                search={
                  <InputGroup>
                    <InputGroupAddon>
                      <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                      ref={searchInputRef}
                      value={searchDraft}
                      onChange={(event) => setSearchDraft(event.target.value)}
                      placeholder={masterDataSearchPlaceholder(resource)}
                      aria-label={masterDataCopy.searchAria}
                    />
                  </InputGroup>
                }
                filters={
                  <>
                    {!supplierFilterPanelOpen ? (
                      <Button type="submit" size="sm">
                        <SearchIcon
                          data-icon="inline-start"
                          aria-hidden="true"
                        />
                        搜索
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      aria-expanded={supplierFilterPanelOpen}
                      onClick={() =>
                        setSupplierFilterPanelOpen((open) => !open)
                      }
                    >
                      <FilterIcon data-icon="inline-start" aria-hidden="true" />
                      高级筛选
                      {hasStructuredSupplierFilters ? (
                        <Badge variant="info">已启用</Badge>
                      ) : null}
                      <ChevronDownIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                        className={
                          supplierFilterPanelOpen
                            ? "rotate-180 transition-transform"
                            : "transition-transform"
                        }
                      />
                    </Button>
                  </>
                }
                secondary={
                  supplierFilterPanelOpen ? (
                    <div
                      className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                      aria-label="供应商筛选条件"
                    >
                      <FixedOptionRadioFilter
                        label="启停"
                        value={lifecycleStatusDraft}
                        onValueChange={setLifecycleStatusDraft}
                        options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                        aria-label={masterDataCopy.filterLifecycleAria}
                      />
                      <FixedOptionRadioFilter
                        label="资质状态"
                        value={supplierQualificationHealthDraft}
                        onValueChange={setSupplierQualificationHealthDraft}
                        options={SUPPLIER_QUALIFICATION_HEALTH_OPTIONS}
                        aria-label="资质状态"
                      />
                      <FixedOptionCheckboxFilter
                        label="供应能力"
                        value={supplierCapabilityCodesDraft}
                        onValueChange={setSupplierCapabilityCodesDraft}
                        options={SUPPLIER_CAPABILITY_OPTIONS}
                        aria-label="供应能力，可多选"
                      />
                      <FixedOptionCheckboxFilter
                        label="资质类型"
                        value={supplierQualificationTypesDraft}
                        onValueChange={setSupplierQualificationTypesDraft}
                        options={SUPPLIER_QUALIFICATION_TYPE_OPTIONS}
                        aria-label="资质类型，可多选"
                      />

                      <div className="flex justify-end">
                        <Button type="submit" size="sm">
                          <SearchIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                          />
                          搜索
                        </Button>
                      </div>
                    </div>
                  ) : undefined
                }
                actions={
                  <>
                    <span
                      className="text-xs text-muted-foreground"
                      aria-live="polite"
                    >
                      {resourceLabel(resource)} · {rows.length} 条
                    </span>
                    {hasActiveFilters ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={clearAllFilters}
                      >
                        清除筛选
                      </Button>
                    ) : null}
                  </>
                }
              />
            </form>
          ) : (
            <ListToolbar
              search={
                <form
                  onSubmit={(e) => {
                    e.preventDefault()
                    if (searchDraft.trim() === q) return
                    patchUrl({ q: searchDraft.trim() || null, page: null })
                    resetPagination()
                  }}
                >
                  <InputGroup>
                    <InputGroupAddon>
                      <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                      ref={searchInputRef}
                      value={searchDraft}
                      onChange={(e) => setSearchDraft(e.target.value)}
                      placeholder={masterDataSearchPlaceholder(resource)}
                      aria-label={masterDataCopy.searchAria}
                    />
                    <InputGroupAddon align="inline-end">
                      <InputGroupButton type="submit" aria-label="执行搜索">
                        搜索
                      </InputGroupButton>
                    </InputGroupAddon>
                  </InputGroup>
                </form>
              }
              filters={
                <>
                  <ToggleGroup
                    value={[lifecycleStatus]}
                    onValueChange={(values) => {
                      const next =
                        (values[0] as typeof lifecycleStatus | undefined) ??
                        "all"
                      changeLifecycle(next)
                    }}
                    variant="outline"
                    size="sm"
                    spacing={0}
                    aria-label={masterDataCopy.filterLifecycleAria}
                  >
                    <ToggleGroupItem value="all">全部</ToggleGroupItem>
                    <ToggleGroupItem value="enabled">
                      {masterDataCopy.lifecycleEnabled}
                    </ToggleGroupItem>
                    <ToggleGroupItem value="disabled">
                      {masterDataCopy.lifecycleDisabled}
                    </ToggleGroupItem>
                  </ToggleGroup>
                  <OptionCombobox
                    className="w-[10.5rem]"
                    value={revisionTiming}
                    aria-label={masterDataCopy.filterVersionAria}
                    onValueChange={(v) => {
                      changeRevisionTiming(
                        (v ?? "all") as typeof revisionTiming
                      )
                    }}
                    options={[
                      { value: "all", label: masterDataCopy.versionAll },
                      {
                        value: "current",
                        label: masterDataCopy.versionCurrent,
                      },
                      {
                        value: "future",
                        label: masterDataCopy.versionFuture,
                      },
                    ]}
                    size="sm"
                    allowClear={false}
                    placeholder={masterDataCopy.versionAll}
                  />
                </>
              }
              actions={
                <>
                  <span
                    className="text-xs text-muted-foreground"
                    aria-live="polite"
                  >
                    {resourceLabel(resource)} · {rows.length} 条
                  </span>
                  {hasActiveFilters ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={clearAllFilters}
                    >
                      清除筛选
                    </Button>
                  ) : null}
                </>
              }
            />
          )
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
                  kind={hasActiveFilters ? "filter" : "no-data"}
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
                          if (isProductResource || isSupplierResource) {
                            router.push(`/master-data/${resource}/new`)
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
                  `/master-data/${resource}/${row.stableId}?section=overview`
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
                  `/master-data/${resource}/${row.stableId}?section=overview`
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

      {!skipPreviewSheet ? (
        <QuickPreviewSheet
          open={previewRow != null}
          onOpenChange={(open) => {
            if (!open) {
              setPreviewId(null)
              if (lastFocusedRowId.current) {
                const el = document.querySelector(
                  `[data-row-id="${lastFocusedRowId.current}"]`
                )
                if (el instanceof HTMLElement) el.focus()
              }
            }
          }}
          size={isSellableResource ? "preview" : "detail"}
          title={
            previewRow?.sellableItem
              ? `${previewRow.name} · ${previewRow.sellableItem.specificationLabel}`
              : previewRow?.name ?? "基础资料预览"
          }
          description={
            previewRow?.sellableItem
              ? "公司商品池中当前符合销售资格的 SKU"
              : undefined
          }
          identity={
            previewRow ? (
              <span className="num">
                {previewRow.sellableItem ? "SKU 编号：" : null}
                {previewRow.stableNo}
                {!previewRow.sellableItem ? ` · v${previewRow.revisionNo}` : null}
              </span>
            ) : null
          }
          summary={
            previewRow ? (
              <div className="flex flex-wrap items-center gap-2">
                {previewRow.sellableItem ? (
                  <>
                    <Badge variant="success">当前可售</Badge>
                    <Badge variant="outline">
                      {previewRow.sellableItem.productKindLabel}
                    </Badge>
                    <Badge variant="outline">
                      <span className="num">
                        {previewRow.sellableItem.supplierCount}
                      </span>{" "}
                      家有效供应商
                    </Badge>
                  </>
                ) : (
                  <>
                    <BusinessStatusBadge
                      context="preview"
                      label={previewRow.lifecycleStatusLabel}
                      tone={previewRow.lifecycleTone}
                    />
                    <Badge
                      variant={
                        previewRow.revisionTiming === "FUTURE"
                          ? "warning"
                          : "secondary"
                      }
                    >
                      {previewRow.revisionTimingLabel}
                    </Badge>
                  </>
                )}
              </div>
            ) : null
          }
          footer={
            previewRow ? (
              previewRow.sellableItem ? (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => setPreviewId(null)}
                  >
                    关闭
                  </Button>
                  <Button
                    type="button"
                    render={
                      <Link
                        href={`/master-data/products/${previewRow.sellableItem.productId}?section=overview`}
                      />
                    }
                  >
                    打开商品资料
                    <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                  </Button>
                </>
              ) : (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => setPreviewId(null)}
                  >
                    关闭
                  </Button>
                  <DisabledActionHint
                    message={previewRow.actionBlockers.find(
                      (b) => b.action === "CREATE_REVISION"
                    )?.message}
                  >
                    <Button
                      type="button"
                      variant="outline"
                      disabled={
                        !previewRow.allowedActions.includes("CREATE_REVISION")
                      }
                      title={
                        previewRow.actionBlockers.find(
                          (b) => b.action === "CREATE_REVISION"
                        )?.message
                      }
                      onClick={() => setReviseTarget(previewRow)}
                    >
                      {masterDataCopy.actionUpdate}
                    </Button>
                  </DisabledActionHint>
                  <DisabledActionHint
                    message={previewRow.actionBlockers.find(
                      (b) => b.action === "DISABLE"
                    )?.message}
                  >
                    <Button
                      type="button"
                      variant="outline"
                      disabled={!previewRow.allowedActions.includes("DISABLE")}
                      title={
                        previewRow.actionBlockers.find(
                          (b) => b.action === "DISABLE"
                        )?.message
                      }
                      onClick={() => setDisableTarget(previewRow)}
                    >
                      {masterDataCopy.actionDisable}
                    </Button>
                  </DisabledActionHint>
                  <Button
                    type="button"
                    render={
                      <Link
                        href={`/master-data/${resource}/${previewRow.stableId}?section=overview`}
                      />
                    }
                  >
                    {masterDataCopy.actionOpenDetail}
                  </Button>
                </>
              )
            ) : null
          }
        >
          {previewRow ? (
            previewRow.sellableItem ? (
              <SellableItemPreviewPanel row={previewRow} />
            ) : (
              <MasterDataPreviewPanel
                row={previewRow}
                detail={previewDetailQuery.data}
                detailLoading={previewDetailQuery.isPending}
              />
            )
          ) : null}
        </QuickPreviewSheet>
      ) : null}

      <ProductSupplyDialog
        product={supplyProduct}
        skus={
          supplyProduct
            ? (productSkusByProduct.get(supplyProduct.stableId) ?? [])
            : []
        }
        skuLoading={productSkusQuery.isPending}
        skuError={productSkusQuery.error}
        offerings={supplierOfferingsQuery.data ?? []}
        offeringLoading={
          productPageSkuIds.length > 0 && supplierOfferingsQuery.isPending
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
