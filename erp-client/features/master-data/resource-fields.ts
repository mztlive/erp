/**
 * W14 资源专属字段 — 按资源强类型化（对齐 docs/ui-workspaces/w14-basic-data.md §4.3/§5.2/§8.2）。
 * 新建 / 更新资料表单按 `RESOURCE_FIELDS` 渲染专属字段；
 * 提交命令携带强类型 `fields`，禁止退回通用 Record。
 */

import { z } from "zod"

import { masterDataCopy } from "@/features/master-data/copy"
import type {
  MasterDataCenterView,
  MasterDataListItem,
  MasterDataResource,
  MasterDataResourceFields,
} from "@/features/master-data/types"

export const REGION_OPTIONS = [
  "华东",
  "华南",
  "华北",
  "西南",
  "华中",
  "西北",
  "全国",
] as const

/** 供应商结算方式（对齐 supplier_commercial_profile_revision.settlement_mode）。 */
export const SETTLEMENT_MODE_OPTIONS = ["预付款", "先用后付", "现结"] as const

/** 供应商发票类型。 */
export const INVOICE_TYPE_OPTIONS = [
  "增值税专用发票",
  "增值税普通发票",
  "电子发票",
] as const

/** 供应商评级。 */
export const SUPPLIER_RATING_OPTIONS = [
  "A 级",
  "B 级",
  "C 级",
  "D 级",
] as const

/** 供应商能力（多选，对齐 erp-phase-1 §4.5）。 */
export const SUPPLIER_CAPABILITY_OPTIONS = [
  "实物商品",
  "虚拟商品",
  "线下服务",
  "API",
  "印刷",
] as const

/** SKU 基础计量单位字典（演示静态；正式由 unit_of_measure 维护）。 */
export const BASE_UNIT_OPTIONS = [
  "件",
  "个",
  "盒",
  "套",
  "箱",
  "瓶",
  "袋",
  "份",
  "张",
  "篮",
  "kg",
  "g",
] as const

export const PRODUCT_KIND_OPTIONS = [
  "实物",
  "虚拟",
  "服务",
  "卡券",
] as const

/**
 * 商品分类下拉选项（演示）。
 * 正式环境从 `categories` 资源当前启用修订查询；此处与种子数据名称对齐。
 */
export const CATEGORY_OPTIONS = [
  "礼盒",
  "茶叶",
  "零食",
  "酒水",
  "卡券",
  "办公",
] as const

/**
 * 品牌下拉选项（演示）。
 * 正式环境从 `brands` 资源当前启用修订查询；此处与种子数据名称对齐。
 */
export const BRAND_OPTIONS = [
  "新芽",
  "明前",
  "礼遇",
  "企业优选",
  "无品牌",
] as const

/**
 * SKU 供应商下拉选项（演示，仅启用供应商）。
 * 正式环境从 `suppliers` 资源当前启用修订查询；此处与种子数据名称对齐。
 */
export const SUPPLIER_OPTIONS = [
  "鲜果直供供应链",
  "礼遇包装工坊",
] as const

export type ResourceFieldKind =
  | "text"
  | "textarea"
  | "select"
  | "checkbox-group"
  | "category-parent"
  | "media"
  | "media-list"

export type ResourceFieldDef = Readonly<{
  key: string
  label: string
  kind: ResourceFieldKind
  options?: readonly string[]
  required?: boolean
  /** 是否进入列表关键信息（keyFacts）。 */
  listFact?: boolean
  /** 列表/历史版本中的其他同义标签，用于编辑回填。 */
  aliases?: readonly string[]
  /** 宽表单分区（仅商品 SKU 等复杂资源使用）。 */
  section?: "identity" | "catalog" | "media" | "default"
  /** media / media-list 字段说明。 */
  hint?: string
}>

/** 按资源声明可维护专属字段；仓库在写门禁未确认前无维护字段。 */
export const RESOURCE_FIELDS: Readonly<
  Record<MasterDataResource, readonly ResourceFieldDef[]>
> = {
  "sellable-items": [
    {
      key: "sku",
      label: masterDataCopy.fSku,
      kind: "text",
      required: true,
      listFact: true,
    },
    {
      key: "refSupplier",
      label: masterDataCopy.fRefSupplier,
      kind: "text",
      listFact: true,
    },
    {
      key: "refCost",
      label: masterDataCopy.fRefCost,
      kind: "text",
      listFact: true,
    },
    {
      key: "region",
      label: masterDataCopy.fRegion,
      kind: "select",
      options: REGION_OPTIONS,
      listFact: true,
    },
    { key: "leadTime", label: masterDataCopy.fLeadTime, kind: "text" },
    {
      key: "fulfillmentModes",
      label: masterDataCopy.fFulfillmentModes,
      kind: "text",
    },
    { key: "taxRate", label: masterDataCopy.fTaxRate, kind: "text" },
  ],
  /**
   * 商品（SPU）列表/概览事实标签。
   * 完整维护在商品详情页（规格组合 → SKU，主图在 SKU，轮播/详情图在 SPU），
   * 不再使用宽对话框 + 规格标识手填模型。
   */
  products: [
    {
      key: "baseUnit",
      label: masterDataCopy.fBaseUnit,
      kind: "select",
      options: BASE_UNIT_OPTIONS,
      required: true,
      listFact: true,
      section: "identity",
    },
    {
      key: "category",
      label: masterDataCopy.fCategory,
      kind: "select",
      options: CATEGORY_OPTIONS,
      required: true,
      listFact: true,
      section: "catalog",
    },
    {
      key: "brand",
      label: masterDataCopy.fBrand,
      kind: "select",
      options: BRAND_OPTIONS,
      required: true,
      listFact: true,
      section: "catalog",
    },
    {
      key: "supplier",
      label: masterDataCopy.fSupplier,
      kind: "select",
      options: SUPPLIER_OPTIONS,
      listFact: true,
      section: "catalog",
    },
  ],
  categories: [
    {
      key: "code",
      label: masterDataCopy.fCategoryCode,
      kind: "text",
      required: true,
      listFact: true,
    },
    {
      key: "parentId",
      label: masterDataCopy.fParentCategory,
      kind: "category-parent",
      listFact: true,
      aliases: ["上级分类"],
    },
    {
      key: "productKind",
      label: masterDataCopy.fProductKind,
      kind: "select",
      options: PRODUCT_KIND_OPTIONS,
      listFact: true,
    },
  ],
  brands: [
    {
      key: "code",
      label: masterDataCopy.fBrandCode,
      kind: "text",
      required: true,
      listFact: true,
    },
    {
      key: "logo",
      label: masterDataCopy.fBrandLogo,
      kind: "media",
      listFact: true,
      hint: masterDataCopy.brandLogoHint,
      aliases: ["Logo", "品牌 Logo"],
    },
  ],
  "voucher-categories": [
    {
      key: "sku",
      label: masterDataCopy.fSku,
      kind: "text",
      required: true,
      listFact: true,
      aliases: ["卡券 SKU"],
    },
    {
      key: "description",
      label: masterDataCopy.fDescription,
      kind: "textarea",
      listFact: true,
      aliases: ["说明"],
    },
  ],
  suppliers: [
    {
      key: "company",
      label: masterDataCopy.fCompany,
      kind: "text",
      required: true,
      listFact: true,
    },
    {
      key: "contactName",
      label: masterDataCopy.fContactName,
      kind: "text",
      listFact: true,
    },
    {
      key: "contactPhone",
      label: masterDataCopy.fContactPhone,
      kind: "text",
      listFact: true,
    },
    {
      key: "address",
      label: masterDataCopy.fAddress,
      kind: "text",
    },
    {
      key: "settlement",
      label: masterDataCopy.fSettlement,
      kind: "select",
      options: SETTLEMENT_MODE_OPTIONS,
      listFact: true,
      aliases: ["商务结算", "商务结算版本"],
    },
    {
      key: "capability",
      label: masterDataCopy.fCapability,
      kind: "checkbox-group",
      options: SUPPLIER_CAPABILITY_OPTIONS,
      listFact: true,
      aliases: ["能力版本"],
    },
    {
      key: "businessCategory",
      label: masterDataCopy.fBusinessCategory,
      kind: "text",
    },
    {
      key: "signingEntity",
      label: masterDataCopy.fSigningEntity,
      kind: "text",
    },
    {
      key: "paymentEntity",
      label: masterDataCopy.fPaymentEntity,
      kind: "text",
    },
    {
      key: "qualification",
      label: masterDataCopy.fQualification,
      kind: "media-list",
      hint: masterDataCopy.supplierQualificationHint,
    },
    {
      key: "contractNo",
      label: masterDataCopy.fContractNo,
      kind: "text",
    },
    {
      key: "contractValidFrom",
      label: masterDataCopy.fContractValidFrom,
      kind: "text",
    },
    {
      key: "contractValidTo",
      label: masterDataCopy.fContractValidTo,
      kind: "text",
    },
    {
      key: "contractFile",
      label: masterDataCopy.fContractFile,
      kind: "media-list",
      hint: masterDataCopy.supplierQualificationHint,
    },
    {
      key: "authorizationFile",
      label: masterDataCopy.fAuthorizationFile,
      kind: "media-list",
      hint: masterDataCopy.supplierQualificationHint,
    },
    {
      key: "authorizationValidFrom",
      label: masterDataCopy.fAuthorizationValidFrom,
      kind: "text",
    },
    {
      key: "authorizationValidTo",
      label: masterDataCopy.fAuthorizationValidTo,
      kind: "text",
    },
    {
      key: "foodLicense",
      label: masterDataCopy.fFoodLicense,
      kind: "media-list",
      hint: masterDataCopy.supplierQualificationHint,
    },
    {
      key: "legalPersonIdCard",
      label: masterDataCopy.fLegalPersonIdCard,
      kind: "media-list",
      hint: masterDataCopy.supplierQualificationHint,
    },
    { key: "taxNo", label: masterDataCopy.fTaxNo, kind: "text" },
    { key: "bankName", label: masterDataCopy.fBankName, kind: "text" },
    { key: "bankAccount", label: masterDataCopy.fBankAccount, kind: "text" },
    {
      key: "invoiceType",
      label: masterDataCopy.fInvoiceType,
      kind: "select",
      options: INVOICE_TYPE_OPTIONS,
      listFact: true,
    },
    {
      key: "invoiceTaxRate",
      label: masterDataCopy.fInvoiceTaxRate,
      kind: "text",
    },
    {
      key: "initialScore",
      label: masterDataCopy.fInitialScore,
      kind: "text",
    },
    {
      key: "supplierRating",
      label: masterDataCopy.fSupplierRating,
      kind: "select",
      options: SUPPLIER_RATING_OPTIONS,
      listFact: true,
    },
    {
      key: "currentScore",
      label: masterDataCopy.fCurrentScore,
      kind: "text",
    },
  ],
  warehouses: [],
}

export type ResourceFormValues = {
  name: string
  effectiveFrom: string
  effectiveTo: string
  changeReason: string
  [field: string]: string
}

/**
 * 品牌 / 商品分类为即时字典，表单不收集生效期间；
 * 提交时由服务端/会话层默认「立即生效」。
 */
export function usesEffectivePeriod(resource: MasterDataResource): boolean {
  return resource !== "brands" && resource !== "categories"
}

/** 通用基础字段 + 当前资源专属字段（必填规则），供新建 / 更新表单共用。 */
export function buildResourceSchema(
  resource: MasterDataResource,
  defs: readonly ResourceFieldDef[]
) {
  const dynamic: Record<string, z.ZodString> = {}
  for (const def of defs) {
    if (def.kind === "media" && def.required) {
      dynamic[def.key] = z.string().trim().min(1, masterDataCopy.mediaMainRequired)
    } else if (def.required) {
      dynamic[def.key] = z.string().trim().min(1, `请填写${def.label}`)
    } else {
      dynamic[def.key] = z.string()
    }
  }
  return z.object({
    name: z.string().trim().min(2, "请填写名称"),
    effectiveFrom: usesEffectivePeriod(resource)
      ? z.string().min(1, "请填写生效开始日期")
      : z.string(),
    effectiveTo: z.string(),
    changeReason: z.string().trim().min(2, "请填写变更原因"),
    ...dynamic,
  })
}

/** 字典类资源默认立即生效的业务日。 */
export function defaultImmediateEffectiveFrom(): string {
  return new Date().toISOString().slice(0, 10)
}

export function emptyResourceFieldValues(
  resource: MasterDataResource
): Record<string, string> {
  return Object.fromEntries(
    RESOURCE_FIELDS[resource].map((def) => [def.key, ""])
  )
}

export type ResourceFieldValues = Record<string, string>

/**
 * 从编辑目标（列表行或对象中心）回填当前值。
 * 以展示标签匹配；`aliases` 覆盖列表与中心用词差异。
 */
export function currentResourceFieldValues(
  target: MasterDataListItem | MasterDataCenterView
): ResourceFieldValues {
  const resource: MasterDataResource =
    "currentRevision" in target ? target.resource : target.objectType
  const facts =
    "currentRevision" in target ? target.currentRevision.fields : target.keyFacts
  const byLabel = new Map(facts.map((fact) => [fact.label, fact.value]))
  const out: ResourceFieldValues = {}
  for (const def of RESOURCE_FIELDS[resource]) {
    // 分类上级存稳定 ID；展示事实是名称，优先从列表行回填
    if (def.key === "parentId" && !("currentRevision" in target)) {
      out.parentId = target.parentStableId ?? ""
      continue
    }
    if (
      def.key === "code" &&
      !("currentRevision" in target) &&
      target.dictionaryCode
    ) {
      out.code = target.dictionaryCode
      continue
    }
    if (
      def.key === "productKind" &&
      !("currentRevision" in target) &&
      target.productKind
    ) {
      out.productKind = target.productKind
      continue
    }
    const matched =
      byLabel.get(def.label) ??
      def.aliases
        ?.map((alias) => byLabel.get(alias))
        .find((value) => value !== undefined && value !== "")
    if (matched === undefined) continue
    // 展示事实可能带「（N 张）」摘要后缀，回填表单时去掉
    if (def.kind === "media-list") {
      out[def.key] = matched.replace(/（\d+\s*张）\s*$/, "").trim()
    } else if (def.key === "parentId") {
      out.parentId = matched === "（根分类）" ? "" : matched
    } else {
      out[def.key] = matched
    }
  }
  if (
    resource === "categories" &&
    !("currentRevision" in target) &&
    target.parentStableId !== undefined
  ) {
    out.parentId = target.parentStableId
  }
  return out
}

function pickField(
  values: ResourceFieldValues,
  key: string
): string | undefined {
  const value = values[key]
  return value?.trim() ? value.trim() : undefined
}

/** 表单值 → 当前资源的强类型字段对象。 */
export function buildResourceFields(
  resource: MasterDataResource,
  values: ResourceFieldValues
): MasterDataResourceFields[MasterDataResource] {
  switch (resource) {
    case "sellable-items":
      return {
        sku: pickField(values, "sku") ?? "",
        refSupplier: pickField(values, "refSupplier"),
        refCost: pickField(values, "refCost"),
        region: pickField(values, "region"),
        leadTime: pickField(values, "leadTime"),
        fulfillmentModes: pickField(values, "fulfillmentModes"),
        taxRate: pickField(values, "taxRate"),
      }
    case "products":
      // 商品完整字段由商品表单页直接组装 ProductFields；此处仅兜底空结构。
      return {
        baseUnit: pickField(values, "baseUnit") ?? "",
        category: pickField(values, "category") ?? "",
        brand: pickField(values, "brand") ?? "",
        supplier: pickField(values, "supplier"),
        carouselImages: [],
        detailImages: [],
        specs: [],
        skus: [],
      }
    case "categories":
      return {
        code: pickField(values, "code") ?? "",
        parentId: pickField(values, "parentId") || undefined,
        parent: pickField(values, "parent"),
        productKind: pickField(values, "productKind"),
      }
    case "brands":
      return {
        code: pickField(values, "code") ?? "",
        logo: pickField(values, "logo"),
      }
    case "voucher-categories":
      return {
        sku: pickField(values, "sku") ?? "",
        description: pickField(values, "description"),
      }
    case "suppliers":
      return {
        company: pickField(values, "company") ?? "",
        contactName: pickField(values, "contactName"),
        contactPhone: pickField(values, "contactPhone"),
        address: pickField(values, "address"),
        settlement: pickField(values, "settlement"),
        capability: pickField(values, "capability"),
        businessCategory: pickField(values, "businessCategory"),
        signingEntity: pickField(values, "signingEntity"),
        paymentEntity: pickField(values, "paymentEntity"),
        qualification: pickField(values, "qualification"),
        contractNo: pickField(values, "contractNo"),
        contractValidFrom: pickField(values, "contractValidFrom"),
        contractValidTo: pickField(values, "contractValidTo"),
        contractFile: pickField(values, "contractFile"),
        authorizationFile: pickField(values, "authorizationFile"),
        authorizationValidFrom: pickField(values, "authorizationValidFrom"),
        authorizationValidTo: pickField(values, "authorizationValidTo"),
        foodLicense: pickField(values, "foodLicense"),
        legalPersonIdCard: pickField(values, "legalPersonIdCard"),
        taxNo: pickField(values, "taxNo"),
        bankName: pickField(values, "bankName"),
        bankAccount: pickField(values, "bankAccount"),
        invoiceType: pickField(values, "invoiceType"),
        invoiceTaxRate: pickField(values, "invoiceTaxRate"),
        initialScore: pickField(values, "initialScore"),
        supplierRating: pickField(values, "supplierRating"),
        currentScore: pickField(values, "currentScore"),
      }
    case "warehouses":
      return {}
  }
}

/** 媒体类字段展示值：空则跳过；列表类显示张数摘要。 */
function formatFieldDisplayValue(
  def: ResourceFieldDef,
  value: string | undefined
): string | null {
  if (!value?.trim()) return null
  if (def.kind === "media-list") {
    const count = value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean).length
    return count > 0 ? `${value}（${count} 张）` : null
  }
  return value.trim()
}

/** 强类型字段 → 展示事实（跳过未填写的字段），写入版本 fields。 */
export function resourceFieldsToFacts(
  resource: MasterDataResource,
  fields: MasterDataResourceFields[MasterDataResource] | undefined
): ReadonlyArray<{ label: string; value: string }> {
  if (!fields) return []
  return RESOURCE_FIELDS[resource]
    .map((def) => {
      const raw = (fields as Readonly<Record<string, string | undefined>>)[
        def.key
      ]
      const value = formatFieldDisplayValue(def, raw)
      return value ? { label: def.label, value } : null
    })
    .filter((fact): fact is { label: string; value: string } => fact !== null)
}

/** 强类型字段 → 列表关键信息（只保留 listFact）。 */
export function resourceFieldsToListFacts(
  resource: MasterDataResource,
  fields: MasterDataResourceFields[MasterDataResource] | undefined
): ReadonlyArray<{ label: string; value: string }> {
  if (!fields) return []
  return RESOURCE_FIELDS[resource]
    .filter((def) => def.listFact)
    .map((def) => {
      const raw = (fields as Readonly<Record<string, string | undefined>>)[
        def.key
      ]
      const value = formatFieldDisplayValue(def, raw)
      return value ? { label: def.label, value } : null
    })
    .filter((fact): fact is { label: string; value: string } => fact !== null)
}

/** 解析 media-list 逗号分隔文件名。 */
export function parseMediaList(value: string | undefined): string[] {
  if (!value?.trim()) return []
  return value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
}

/** 序列化 media-list。 */
export function joinMediaList(names: readonly string[]): string {
  return names.filter(Boolean).join(", ")
}

/**
 * 商品已改为详情页维护，不再使用宽对话框。
 * 保留函数以免调用方断裂，恒为 false。
 */
export function usesWideDialog(_resource: MasterDataResource): boolean {
  return false
}
