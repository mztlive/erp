/** W14 基础资料 · 对象中心（详情）视图类型。 */

import type { StatusTone } from "@/components/ui/status-badge"
import type {
    ActionBlocker,
    LifecycleStatus,
    MasterDataResource,
    MasterDataSectionId,
    ProductKind,
    SelectorEligibility,
    SensitiveFieldView,
} from "@/features/master-data/api/types-core"
import type { ProductDetailView } from "@/features/master-data/api/types-product"

type RevisionTiming = "CURRENT" | "FUTURE" | "HISTORICAL"

export type RevisionTimelineEntry = Readonly<{
    id: string
    revisionNo: number
    revisionTiming: RevisionTiming
    timingLabel: string
    nameSnapshot: string
    actor: string
    effectiveFrom: string
    effectiveTo?: string
    changeReason: string
    isCurrent: boolean
    lifecycleAtRevision: LifecycleStatus
    /** 商品修订的完整 SPU/SKU/价格快照；历史查看不得回填当前主档。 */
    productSnapshot?: ProductDetailView
}>

export type MasterDataCenterView = Readonly<{
    resource: MasterDataResource
    stableId: string
    stableNo: string
    name: string
    lifecycleStatus: LifecycleStatus
    lifecycleStatusLabel: string
    lifecycleTone: StatusTone
    scheduledLifecycleStatus?: LifecycleStatus
    scheduledLifecycleLabel?: string
    revisionTiming: "CURRENT" | "FUTURE"
    revisionTimingLabel: string
    lockVersion: number
    /** 供应商关联 Party 的独立乐观锁版本。 */
    partyLockVersion?: number
    /** `资质类型::证书编号` → 当前适用能力代码，供原样修订。 */
    supplierQualificationCapabilityCodes?: Readonly<
        Record<string, readonly string[]>
    >
    currentRevision: {
        revisionId: string
        revisionNo: number
        name: string
        effectiveFrom: string
        effectiveTo?: string
        changeReason: string
        actor: string
        fields: ReadonlyArray<{ label: string; value: string }>
    }
    revisionTimeline: readonly RevisionTimelineEntry[]
    selectorEligibility: readonly SelectorEligibility[]
    usageSummary: {
        historicalReferenceCount: number
        note: string
    }
    sensitiveFields: readonly SensitiveFieldView[]
    /** Resource-specific overview facts. */
    resourceFacts: ReadonlyArray<{ label: string; value: string }>
    /** Warehouse only: policy is alert-only, stock summary links W10. */
    warehouseStockSummary?: {
        onHandQty: string
        reservedQty: string
        hasBlockingStock: boolean
        w10Href: string
        policyNote: string
    }
    /**
     * 商品 SPU 约束摘要（不含规格标识：签名由属性组合系统派生，UI 不展示）。
     */
    productConstraints?: {
        baseUnit: string
        hasFormalReferences: boolean
        skuCount: number
    }
    /**
     * 商品 SPU 详情：规格维度 + 由规格组合生成的 SKU 行。
     * 主图在 SKU；轮播图 / 详情图在 SPU。
     */
    productDetail?: ProductDetailView
    /** 公司商品类型（`product.product_kind`）；SPU 稳定身份，创建后不可变。 */
    productKind?: ProductKind
    /**
     * 媒体字段的已登记资产回显：字段 key（`logo`/`qualification`/`contractFile`…）
     * → 文件清单（文件名 + asset id + 可访问 URL）。用于编辑回填与展示链接。
     */
    mediaAssets?: Readonly<
        Record<
            string,
            ReadonlyArray<{
                fileName: string
                assetId: string
                url: string
            }>
        >
    >
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
    auditEvents: readonly {
        id: string
        at: string
        actor: string
        action: string
        detail: string
    }[]
    sections: readonly MasterDataSectionId[]
}>
