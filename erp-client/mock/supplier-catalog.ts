/**
 * 供应商商品库、公司 SKU 映射与供给 · 静态种子。
 * 会话覆盖（领取、暂挂、终结、草稿）在 api / session-state 中投影。
 */

import type {
  SupplierCatalogItemView,
  SupplierProductRevisionView,
  PublicationImpactView,
  SupplierOfferingRevisionView,
} from "@/features/supplier-catalog/types"
import {
  REGISTRATION_BLOCKER_MESSAGE,
  RECOVERY_BLOCKER_MESSAGE,
} from "@/features/supplier-catalog/types"

type SupplierProductRevisionSeed = Omit<
  SupplierProductRevisionView,
  | "dropshipFloorPriceGross"
  | "bulkFloorPriceGross"
  | "bulkMinimumOrderQuantity"
> & {
  /** 种子简写：同时写入代发/集采底价 */
  supplyPriceGross?: string | null
  dropshipFloorPriceGross?: string | null
  bulkFloorPriceGross?: string | null
  bulkMinimumOrderQuantity?: string | null
}

function rev(partial: SupplierProductRevisionSeed): SupplierProductRevisionView {
  const {
    supplyPriceGross,
    dropshipFloorPriceGross,
    bulkFloorPriceGross,
    bulkMinimumOrderQuantity,
    ...rest
  } = partial
  const floor =
    dropshipFloorPriceGross ??
    bulkFloorPriceGross ??
    supplyPriceGross ??
    null
  return {
    ...rest,
    dropshipFloorPriceGross: dropshipFloorPriceGross ?? floor,
    bulkFloorPriceGross: bulkFloorPriceGross ?? floor,
    bulkMinimumOrderQuantity: bulkMinimumOrderQuantity ?? "1",
  }
}

type OfferingSeed = Omit<
  SupplierOfferingRevisionView,
  "offeringRevisionId" | "floorPriceGross" | "supplyMode" | "dropshipExpress"
> &
  Partial<
    Pick<
      SupplierOfferingRevisionView,
      "floorPriceGross" | "supplyMode" | "dropshipExpress"
    >
  >

function offering(partial: OfferingSeed): SupplierOfferingRevisionView {
  return {
    offeringRevisionId: `${partial.offeringId}_r${partial.revisionNo}`,
    floorPriceGross: partial.supplyPriceGross,
    supplyMode: ["BULK"],
    ...partial,
    immutable: true as const,
  }
}

const pauseBase = (
  reasons: string[],
  pubs: PublicationImpactView["pauseSubResults"]
): PublicationImpactView => ({
  activePublicationCount: 0,
  pausedPublicationCount: pubs.length,
  historicalPaidOrderCount: 12,
  safetyPauseTriggered: true,
  safetyPauseReasons: reasons,
  pauseSubResults: pubs,
  mallSalePriceAutoUpdate: false,
  moqCopiedToMallMinPurchase: false,
  recoveryBlocker: {
    code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
    message: RECOVERY_BLOCKER_MESSAGE,
  },
  note: "安全暂停已由系统已完成（重复请求不会再次执行）；历史已支付订单保留下单记录。供货价变化不自动改商城销售价。",
})

const noPause: PublicationImpactView = {
  activePublicationCount: 0,
  pausedPublicationCount: 0,
  historicalPaidOrderCount: 0,
  safetyPauseTriggered: false,
  safetyPauseReasons: [],
  pauseSubResults: [],
  mallSalePriceAutoUpdate: false,
  moqCopiedToMallMinPurchase: false,
  note: "尚无在售发布绑定。",
}

function poolEntryForSku(
  skuId: string,
  salesVisiblePrice: string,
  status: "ACTIVE" | "PAUSED" = "ACTIVE"
) {
  return {
    poolEntryId: `pool_${skuId}`,
    poolEntryRevisionId: `pool_${skuId}_r1`,
    status,
    salesVisiblePrice,
    validFrom: "2026-01-01",
  } as const
}

const skuCandidatesCommon = [
  {
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    skuName: "新春坚果礼盒 · 典藏款",
    specification: "规格：典藏款",
    baseUnit: "套",
    revisionNo: 6,
    similarityLabel: "名称+规格高度相似",
  },
  {
    skuId: "sku_ny_box_02",
    skuCode: "SKU-NY-BOX-02",
    skuName: "新春坚果礼盒 · 典藏款",
    specification: "规格：轻享款",
    baseUnit: "套",
    revisionNo: 6,
    similarityLabel: "品类相近",
  },
]

/** ERROR · 已注册 BUSINESS_EXCEPTION */
export const SEED_ERROR: SupplierCatalogItemView = {
  changeType: "ERROR",
  workItem: {
    workItemId: "wi_ext_err_01",
    workItemType: "BUSINESS_EXCEPTION",
    businessObjectType: "SUPPLIER_CATALOG_SKU",
    subjectVersion: "sv_ext_err_01_v1",
    subjectHash: "sha256:ext_err_01_rev19",
    workItemStatus: "PENDING",
    dueAt: "2026-08-01T17:00:00+08:00",
    allowedActions: [
      "CLAIM",
      "HOLD",
      "RETURN_FOR_DATA_FIX",
      "QUERY_ORIGINAL_RESULT",
      "SAVE_EVIDENCE",
      "CONFIRM_ERROR_RESOLVED",
    ],
    actionBlockers: [
      {
        action: "APPROVE_MAPPING",
        code: "EXCEPTION_NOT_MAPPING",
        message: "异常项不能强行映射，须先退回数据修复或确认异常已解决",
      },
    ],
    reason: "来源规格字段缺失且价格口径异常，无法形成结构化白名单字段",
    impact: "不进入商品与发布；可退回技术/数据修复",
    priority: 95,
    handlerKey: "SupplierCatalogErrorHandler",
  },
  supplierProduct: {
    id: "ep_err_01",
    supplier: { id: "sup_jd", name: "京东企业购" },
    source: {
      type: "API",
      label: "API 同步",
      connection: { id: "conn_jd_01", code: "JD-CATALOG" },
    },
    supplierSpuCode: "EXT-ERR-4410",
    supplierSkuCode: "E-SKU-4410",
    status: "ERROR",
    currentRevision: rev({
      revisionNo: 18,
      sourceRevisionToken: "src_tok_18",
      sourceUpdatedAt: "2026-07-28T10:00:00+08:00",
      syncedAt: "2026-07-28T10:05:00+08:00",
      name: "办公椅 · 残缺记录",
      specification: "—",
      category: "办公家具",
      supplyPriceGross: "899.00",
      availableQuantity: "0",
      availabilityStatus: "UNAVAILABLE",
      contentFingerprintShort: "hmac:a1b2…c9",
    }),
    incomingRevision: rev({
      revisionNo: 19,
      sourceRevisionToken: "src_tok_19",
      sourceUpdatedAt: "2026-08-01T08:10:00+08:00",
      syncedAt: "2026-08-01T08:12:00+08:00",
      name: "办公椅 · 残缺记录",
      specification: "",
      category: "办公家具",
      supplyPriceGross: null,
      availableQuantity: "—",
      availabilityStatus: "UNAVAILABLE",
      contentFingerprintShort: "hmac:d4e5…f0",
    }),
  },
  mapping: {
    mappingStatus: "PENDING",
    history: [],
  },
  skuCandidates: [],
  offering: {
    stableId: "off_err_01",
    revisionHistory: [],
  },
  publicationImpact: noPause,
  sourceContext: {
    intakeId: "sync_job_8821",
    sourceReference: "batch:jd:20260801-0812",
    receivedAt: "2026-08-01T08:12:00+08:00",
  },
  sourceDiff: [
    {
      id: "d1",
      field: "规格",
      before: "—",
      after: "（空）",
      note: "关键字段缺失",
    },
    {
      id: "d2",
      field: "含税供货价",
      before: "899.00",
      after: "（无法解析）",
      note: "价格口径异常",
      costSensitive: true,
    },
  ],
  allowedActions: [
    "HOLD",
    "RETURN_FOR_DATA_FIX",
    "CONFIRM_ERROR_RESOLVED",
    "OPEN_W29",
  ],
  actionBlockers: [
    {
      action: "APPROVE_MAPPING",
      code: "EXCEPTION_NOT_MAPPING",
      message: "异常项不能强行映射",
    },
  ],
  costFieldVisibility: "visible",
}

/** STOPPED · 已注册异常 + 安全暂停 + 恢复责任阻断 */
export const SEED_STOPPED: SupplierCatalogItemView = {
  changeType: "STOPPED",
  workItem: {
    workItemId: "wi_ext_stop_01",
    workItemType: "BUSINESS_EXCEPTION",
    businessObjectType: "SUPPLIER_CATALOG_SKU",
    subjectVersion: "sv_ext_stop_01_v2",
    subjectHash: "sha256:ext_stop_01_rev22",
    workItemStatus: "PENDING",
    dueAt: "2026-08-01T12:00:00+08:00",
    allowedActions: [
      "CLAIM",
      "HOLD",
      "SAVE_EVIDENCE",
      "CONFIRM_STOP_SUPPLY",
    ],
    actionBlockers: [
      {
        action: "SELECT_SUBSTITUTE",
        code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
        message: RECOVERY_BLOCKER_MESSAGE,
      },
      {
        action: "OPEN_W22_RECOVERY",
        code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
        message: RECOVERY_BLOCKER_MESSAGE,
      },
    ],
    reason: "供应商停止供应；系统已安全暂停全部受影响在售发布",
    impact: "不可下单；历史订单记录保留；不得选定替代或恢复发布",
    priority: 100,
    handlerKey: "SupplierCatalogStopSupplyHandler",
  },
  supplierProduct: {
    id: "ep_stop_01",
    supplier: { id: "sup_jd", name: "京东企业购" },
    source: {
      type: "API",
      label: "API 同步",
      connection: { id: "conn_jd_01", code: "JD-CATALOG" },
    },
    supplierSpuCode: "EXT-SKU-8801",
    supplierSkuCode: "E-SKU-8801",
    status: "STOPPED",
    currentRevision: rev({
      revisionNo: 21,
      sourceRevisionToken: "src_tok_21",
      sourceUpdatedAt: "2026-07-20T14:00:00+08:00",
      syncedAt: "2026-07-20T14:02:00+08:00",
      name: "礼盒红茶 250g 铁罐装",
      specification: "净含量：250g / 包装：铁罐",
      category: "茶饮",
      supplyPriceGross: "268.00",
      availableQuantity: "120",
      availabilityStatus: "AVAILABLE",
      contentFingerprintShort: "hmac:11aa…22",
    }),
    incomingRevision: rev({
      revisionNo: 22,
      sourceRevisionToken: "src_tok_22",
      sourceUpdatedAt: "2026-08-01T07:00:00+08:00",
      syncedAt: "2026-08-01T07:01:00+08:00",
      name: "礼盒红茶 250g 铁罐装",
      specification: "净含量：250g / 包装：铁罐",
      category: "茶饮",
      supplyPriceGross: "268.00",
      availableQuantity: "0",
      availabilityStatus: "STOPPED",
      contentFingerprintShort: "hmac:33bb…44",
    }),
  },
  mapping: {
    mappingStatus: "ACTIVE",
    skuId: "sku_tea_04",
    skuCode: "SKU-TEA-250-TIN",
    skuName: "礼盒红茶",
    skuRevisionId: "prd_2_r3:sku_tea_04",
    specification: "净含量：250g / 包装：铁罐",
    baseUnit: "盒",
    approvedBy: "采购 · 周然",
    approvedAt: "2026-06-01T11:00:00+08:00",
    reason: "一物一码映射",
    mappingVersion: "map_v3",
    history: [
      {
        id: "mh1",
        skuCode: "SKU-TEA-250-TIN",
        status: "已生效",
        at: "2026-06-01",
        note: "当前唯一有效映射",
      },
      {
        id: "mh0",
        skuCode: "SKU-TEA-08",
        status: "已失效",
        at: "2026-03-12",
        note: "历史映射，不原位覆盖",
      },
    ],
  },
  skuCandidates: [
    {
      skuId: "sku_tea_04",
      skuCode: "SKU-TEA-250-TIN",
      skuName: "礼盒红茶",
      specification: "净含量：250g / 包装：铁罐",
      baseUnit: "盒",
      revisionNo: 7,
      similarityLabel: "当前映射",
    },
    {
      skuId: "sku_tea_alt",
      skuCode: "SKU-TEA-11",
      skuName: "绿茶礼盒 常销版",
      specification: "雨前龙井 200g×2",
      baseUnit: "盒",
      revisionNo: 3,
      similarityLabel: "替代供应商候选",
    },
  ],
  offering: {
    stableId: "off_tea_01",
    currentRevision: offering({
      offeringId: "off_tea_01",
      revisionNo: 5,
      status: "STOPPED",
      supplyPriceGross: "268.00",
      supplyPriceNet: "237.17",
      inputTaxRate: "0.13",
      freightAmount: "12.00",
      serviceFeeAmount: "0.00",
      minimumOrderQuantity: "2",
      supplyRegion: ["华东", "华北"],
      availabilityStatus: "STOPPED",
      availableQuantity: "0",
      productCapabilities: ["logistics"],
      validFrom: "2026-06-01",
      validTo: "2026-08-01",
      createdAt: "2026-06-01T11:30:00+08:00",
      immutable: true,
    }),
    revisionHistory: [
      offering({
        offeringId: "off_tea_01",
        revisionNo: 4,
        status: "ACTIVE",
        supplyPriceGross: "258.00",
        supplyPriceNet: "228.32",
        inputTaxRate: "0.13",
        freightAmount: "12.00",
        serviceFeeAmount: "0.00",
        minimumOrderQuantity: "2",
        supplyRegion: ["华东", "华北"],
        availabilityStatus: "AVAILABLE",
        availableQuantity: "200",
        productCapabilities: ["cancel", "refund", "logistics"],
        validFrom: "2026-03-01",
        validTo: "2026-05-31",
        createdAt: "2026-03-01T09:00:00+08:00",
        immutable: true,
      }),
      offering({
        offeringId: "off_tea_01",
        revisionNo: 5,
        status: "STOPPED",
        supplyPriceGross: "268.00",
        supplyPriceNet: "237.17",
        inputTaxRate: "0.13",
        freightAmount: "12.00",
        serviceFeeAmount: "0.00",
        minimumOrderQuantity: "2",
        supplyRegion: ["华东", "华北"],
        availabilityStatus: "STOPPED",
        availableQuantity: "0",
        productCapabilities: ["logistics"],
        validFrom: "2026-06-01",
        validTo: "2026-08-01",
        createdAt: "2026-06-01T11:30:00+08:00",
        immutable: true,
      }),
    ],
  },
  poolEntry: poolEntryForSku("sku_tea_04", "98.00", "PAUSED"),
  publicationImpact: pauseBase(
    ["STOPPED"],
    [
      {
        id: "ps1",
        publicationId: "PUB-20260730-018",
        reason: "STOPPED",
        outboxId: "obx_pause_901",
        status: "PAUSED",
      },
      {
        id: "ps2",
        publicationId: "PUB-20260712-003",
        reason: "STOPPED",
        outboxId: "obx_pause_902",
        status: "PAUSED",
      },
    ]
  ),
  sourceContext: {
    intakeId: "sync_job_8800",
    sourceReference: "batch:jd:20260801-0701",
    receivedAt: "2026-08-01T07:01:00+08:00",
  },
  sourceDiff: [
    {
      id: "s1",
      field: "可供状态",
      before: "AVAILABLE",
      after: "STOPPED",
      note: "停止供应",
    },
    {
      id: "s2",
      field: "可供数量",
      before: "120",
      after: "0",
    },
  ],
  allowedActions: [
    "HOLD",
    "CONFIRM_STOP_SUPPLY",
    "PREPARE_SUBSTITUTE_CANDIDATE",
    "OPEN_CENTER",
  ],
  actionBlockers: [
    {
      action: "SELECT_SUBSTITUTE",
      code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
      message: RECOVERY_BLOCKER_MESSAGE,
    },
    {
      action: "OPEN_W22_RECOVERY",
      code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
      message: RECOVERY_BLOCKER_MESSAGE,
    },
  ],
  costFieldVisibility: "visible",
}

/** NEW · 类型未登记 fail-closed */
export const SEED_NEW: SupplierCatalogItemView = {
  changeType: "NEW",
  registrationBlocker: {
    code: "WORK_ITEM_TYPE_UNREGISTERED",
    message: REGISTRATION_BLOCKER_MESSAGE,
    businessProcess: "MAPPING",
  },
  supplierProduct: {
    id: "ep_new_01",
    supplier: { id: "sup_jd", name: "京东企业购" },
    source: {
      type: "API",
      label: "API 同步",
      connection: { id: "conn_jd_01", code: "JD-CATALOG" },
    },
    supplierSpuCode: "EXT-SKU-9912",
    supplierSkuCode: "E-SKU-9912",
    status: "OBSERVED",
    currentRevision: rev({
      revisionNo: 1,
      sourceRevisionToken: "src_tok_1",
      sourceUpdatedAt: "2026-08-01T09:00:00+08:00",
      syncedAt: "2026-08-01T09:05:00+08:00",
      name: "坚果礼盒 A 款",
      description: "六种混合坚果礼盒，独立小袋包装，适合企业节庆赠礼。",
      specification: "混合坚果 1.2kg / 礼盒装",
      category: "休闲食品",
      brand: "企业优选",
      baseUnit: "套",
      barcode: "6901234569912",
      attributes: [
        { name: "净含量", value: "1.2kg" },
        { name: "包装", value: "礼盒装" },
      ],
      media: [
        {
          id: "ep_new_01_media_main",
          usage: "SKU_MAIN",
          fileName: "supplier-nut-box-main.webp",
          sortOrder: 0,
          fileAssetId: "asset_supplier_nut_box_main",
          archiveStatus: "ARCHIVED",
        },
        {
          id: "ep_new_01_media_carousel_1",
          usage: "SPU_CAROUSEL",
          fileName: "supplier-nut-box-front.webp",
          sortOrder: 0,
          fileAssetId: "asset_supplier_nut_box_front",
          archiveStatus: "ARCHIVED",
        },
        {
          id: "ep_new_01_media_detail_1",
          usage: "SPU_DETAIL",
          fileName: "supplier-nut-box-detail.webp",
          sortOrder: 0,
          fileAssetId: "asset_supplier_nut_box_detail",
          archiveStatus: "ARCHIVED",
        },
      ],
      supplyPriceGross: "420.00",
      availableQuantity: "500",
      availabilityStatus: "AVAILABLE",
      contentFingerprintShort: "hmac:55cc…66",
    }),
  },
  mapping: {
    mappingStatus: "PENDING",
    history: [],
  },
  skuCandidates: skuCandidatesCommon,
  offering: {
    stableId: "off_new_01",
    revisionHistory: [],
    proposedDefaults: {
      supplyPriceGross: "420.00",
      floorPriceGross: "398.00",
      supplyMode: ["BULK"],
      inputTaxRate: "0.13",
      freightAmount: "18.00",
      serviceFeeAmount: "5.00",
      minimumOrderQuantity: "10",
      supplyRegion: ["华东", "华南"],
      productCapabilities: ["logistics"],
      validFrom: "2026-08-01",
      sessionDraftOnly: true,
    },
  },
  publicationImpact: noPause,
  sourceContext: {
    intakeId: "sync_job_8910",
    sourceReference: "batch:jd:20260801-0905",
    receivedAt: "2026-08-01T09:05:00+08:00",
  },
  sourceDiff: [
    {
      id: "n1",
      field: "名称",
      before: "（无）",
      after: "坚果礼盒 A 款",
      note: "供应商新商品资料已保存，尚未关联 ERP 商品",
    },
    {
      id: "n2",
      field: "含税供货价",
      before: "（无）",
      after: "420.00",
      costSensitive: true,
    },
    {
      id: "n3",
      field: "最小起订量（供给）",
      before: "（无）",
      after: "10",
      note: "不等于商城最小购买量",
    },
  ],
  allowedActions: ["PREPARE_DRAFT", "OPEN_W14", "OPEN_CENTER", "BROWSE"],
  actionBlockers: [
    {
      action: "APPROVE_MAPPING",
      code: "WORK_ITEM_TYPE_UNREGISTERED",
      message: REGISTRATION_BLOCKER_MESSAGE,
    },
    {
      action: "CONFIRM_OFFERING_REVISION",
      code: "WORK_ITEM_TYPE_UNREGISTERED",
      message: REGISTRATION_BLOCKER_MESSAGE,
    },
    {
      action: "CLAIM",
      code: "WORK_ITEM_TYPE_UNREGISTERED",
      message: "正常映射任务类型未登记，不能领取",
    },
  ],
  costFieldVisibility: "visible",
}

/** CHANGED · 供货价变化 · 安全暂停 · 类型未登记 */
export const SEED_CHANGED_PRICE: SupplierCatalogItemView = {
  changeType: "CHANGED",
  registrationBlocker: {
    code: "WORK_ITEM_TYPE_UNREGISTERED",
    message: REGISTRATION_BLOCKER_MESSAGE,
    businessProcess: "OFFERING_REVIEW",
  },
  supplierProduct: {
    id: "ep_chg_01",
    supplier: { id: "sup_sn", name: "苏宁企业购" },
    source: {
      type: "API",
      label: "API 同步",
      connection: { id: "conn_sn_02", code: "SN-CATALOG" },
    },
    supplierSpuCode: "EXT-SKU-5502",
    supplierSkuCode: "E-SKU-5502",
    status: "OBSERVED",
    currentRevision: rev({
      revisionNo: 7,
      sourceRevisionToken: "src_sn_7",
      sourceUpdatedAt: "2026-07-15T11:00:00+08:00",
      syncedAt: "2026-07-15T11:03:00+08:00",
      name: "商务保温杯 500ml",
      specification: "316 不锈钢 / 哑光黑",
      category: "日用百货",
      supplyPriceGross: "88.00",
      availableQuantity: "2000",
      availabilityStatus: "AVAILABLE",
      contentFingerprintShort: "hmac:77dd…88",
    }),
    incomingRevision: rev({
      revisionNo: 8,
      sourceRevisionToken: "src_sn_8",
      sourceUpdatedAt: "2026-08-01T10:20:00+08:00",
      syncedAt: "2026-08-01T10:22:00+08:00",
      name: "商务保温杯 500ml",
      specification: "316 不锈钢 / 哑光黑",
      category: "日用百货",
      supplyPriceGross: "96.00",
      availableQuantity: "1800",
      availabilityStatus: "AVAILABLE",
      contentFingerprintShort: "hmac:99ee…00",
    }),
  },
  mapping: {
    mappingStatus: "ACTIVE",
    skuId: "sku_cup_01",
    skuCode: "SKU-CUP-01",
    skuName: "商务保温杯 500ml",
    skuRevisionId: "sku_rev_cup_01_r3",
    specification: "316 不锈钢 / 哑光黑",
    baseUnit: "个",
    approvedBy: "采购 · 周然",
    approvedAt: "2026-05-08T15:00:00+08:00",
    mappingVersion: "map_cup_v1",
    history: [
      {
        id: "ch1",
        skuCode: "SKU-CUP-01",
        status: "已生效",
        at: "2026-05-08",
        note: "唯一有效映射",
      },
    ],
  },
  skuCandidates: [
    {
      skuId: "sku_cup_01",
      skuCode: "SKU-CUP-01",
      skuName: "商务保温杯 500ml",
      specification: "316 不锈钢 / 哑光黑",
      baseUnit: "个",
      revisionNo: 3,
      similarityLabel: "当前映射",
    },
  ],
  offering: {
    stableId: "off_cup_01",
    currentRevision: offering({
      offeringId: "off_cup_01",
      revisionNo: 3,
      status: "PAUSED",
      supplyPriceGross: "88.00",
      supplyPriceNet: "77.88",
      inputTaxRate: "0.13",
      freightAmount: "6.00",
      serviceFeeAmount: "0.00",
      minimumOrderQuantity: "20",
      supplyRegion: ["全国"],
      availabilityStatus: "AVAILABLE",
      availableQuantity: "2000",
      productCapabilities: ["cancel", "refund", "logistics"],
      validFrom: "2026-05-08",
      createdAt: "2026-05-08T15:10:00+08:00",
      immutable: true,
    }),
    revisionHistory: [
      offering({
        offeringId: "off_cup_01",
        revisionNo: 2,
        status: "ACTIVE",
        supplyPriceGross: "85.00",
        supplyPriceNet: "75.22",
        inputTaxRate: "0.13",
        freightAmount: "6.00",
        serviceFeeAmount: "0.00",
        minimumOrderQuantity: "20",
        supplyRegion: ["全国"],
        availabilityStatus: "AVAILABLE",
        availableQuantity: "3000",
        productCapabilities: ["cancel", "refund", "logistics"],
        validFrom: "2026-02-01",
        validTo: "2026-05-07",
        createdAt: "2026-02-01T10:00:00+08:00",
        immutable: true,
      }),
      offering({
        offeringId: "off_cup_01",
        revisionNo: 3,
        status: "PAUSED",
        supplyPriceGross: "88.00",
        supplyPriceNet: "77.88",
        inputTaxRate: "0.13",
        freightAmount: "6.00",
        serviceFeeAmount: "0.00",
        minimumOrderQuantity: "20",
        supplyRegion: ["全国"],
        availabilityStatus: "AVAILABLE",
        availableQuantity: "2000",
        productCapabilities: ["cancel", "refund", "logistics"],
        validFrom: "2026-05-08",
        createdAt: "2026-05-08T15:10:00+08:00",
        immutable: true,
      }),
    ],
    proposedDefaults: {
      supplyPriceGross: "96.00",
      floorPriceGross: "92.00",
      supplyMode: ["DROPSHIP"],
      dropshipExpress: "顺丰速运",
      inputTaxRate: "0.13",
      freightAmount: "6.00",
      serviceFeeAmount: "2.00",
      minimumOrderQuantity: "20",
      supplyRegion: ["全国"],
      productCapabilities: ["cancel", "refund", "logistics"],
      validFrom: "2026-08-01",
      sessionDraftOnly: true,
    },
  },
  poolEntry: poolEntryForSku("sku_cup_01", "128.00", "PAUSED"),
  publicationImpact: {
    ...pauseBase(
      ["COST_CHANGE_UNCONFIRMED"],
      [
        {
          id: "ps_c1",
          publicationId: "PUB-20260728-011",
          reason: "COST_CHANGE_UNCONFIRMED",
          outboxId: "obx_pause_711",
          status: "PAUSED",
        },
      ]
    ),
    activePublicationCount: 0,
    pausedPublicationCount: 1,
    historicalPaidOrderCount: 48,
    note: "供货价或费用变化尚未确认，相关商城商品已暂停销售。确认新的供货条件后也不会自动恢复销售；商城销售价不会随供货价自动变更，最小起订量也不会复制为商城最小购买量。",
  },
  sourceContext: {
    intakeId: "sync_job_9012",
    sourceReference: "batch:sn:20260801-1022",
    receivedAt: "2026-08-01T10:22:00+08:00",
  },
  sourceDiff: [
    {
      id: "c1",
      field: "含税供货价",
      before: "88.00",
      after: "96.00",
      note: "新的供货条件将作为草稿保存，原记录保持不变",
      costSensitive: true,
    },
    {
      id: "c2",
      field: "其它费用",
      before: "0.00",
      after: "2.00",
      costSensitive: true,
    },
    {
      id: "c3",
      field: "可供数量",
      before: "2000",
      after: "1800",
    },
  ],
  allowedActions: ["PREPARE_DRAFT", "OPEN_CENTER", "BROWSE", "OPEN_W20"],
  actionBlockers: [
    {
      action: "CONFIRM_OFFERING_REVISION",
      code: "WORK_ITEM_TYPE_UNREGISTERED",
      message: REGISTRATION_BLOCKER_MESSAGE,
    },
    {
      action: "APPROVE_MAPPING",
      code: "WORK_ITEM_TYPE_UNREGISTERED",
      message: REGISTRATION_BLOCKER_MESSAGE,
    },
    {
      action: "CLAIM",
      code: "WORK_ITEM_TYPE_UNREGISTERED",
      message: "正常供给复核类型未登记，不能领取",
    },
    {
      action: "OPEN_W22_RECOVERY",
      code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
      message: RECOVERY_BLOCKER_MESSAGE,
    },
  ],
  costFieldVisibility: "visible",
}

/** CHANGED · 零库存安全暂停（无任务，仅证据） */
export const SEED_CHANGED_STOCK: SupplierCatalogItemView = {
  changeType: "CHANGED",
  registrationBlocker: {
    code: "WORK_ITEM_TYPE_UNREGISTERED",
    message: REGISTRATION_BLOCKER_MESSAGE,
    businessProcess: "OFFERING_REVIEW",
  },
  supplierProduct: {
    id: "ep_stk_01",
    supplier: { id: "sup_sn", name: "苏宁企业购" },
    source: {
      type: "API",
      label: "API 同步",
      connection: { id: "conn_sn_02", code: "SN-CATALOG" },
    },
    supplierSpuCode: "EXT-SKU-3300",
    supplierSkuCode: "E-SKU-3300",
    status: "OBSERVED",
    currentRevision: rev({
      revisionNo: 4,
      sourceRevisionToken: "src_sn_st_4",
      sourceUpdatedAt: "2026-07-30T16:00:00+08:00",
      syncedAt: "2026-07-30T16:01:00+08:00",
      name: "无线键鼠套装",
      specification: "2.4G / 黑",
      category: "数码配件",
      supplyPriceGross: "129.00",
      availableQuantity: "80",
      availabilityStatus: "AVAILABLE",
      contentFingerprintShort: "hmac:ab12…cd",
    }),
    incomingRevision: rev({
      revisionNo: 5,
      sourceRevisionToken: "src_sn_st_5",
      sourceUpdatedAt: "2026-08-01T11:00:00+08:00",
      syncedAt: "2026-08-01T11:01:00+08:00",
      name: "无线键鼠套装",
      specification: "2.4G / 黑",
      category: "数码配件",
      supplyPriceGross: "129.00",
      availableQuantity: "0",
      availabilityStatus: "UNAVAILABLE",
      contentFingerprintShort: "hmac:ef34…gh",
    }),
  },
  mapping: {
    mappingStatus: "ACTIVE",
    skuId: "sku_kb_01",
    skuCode: "SKU-KB-01",
    skuName: "无线键鼠套装",
    skuRevisionId: "sku_rev_kb_01_r2",
    specification: "2.4G / 黑",
    baseUnit: "套",
    approvedBy: "采购 · 周然",
    approvedAt: "2026-04-02T10:00:00+08:00",
    mappingVersion: "map_kb_v1",
    history: [
      {
        id: "kbh1",
        skuCode: "SKU-KB-01",
        status: "已生效",
        at: "2026-04-02",
        note: "唯一有效映射",
      },
    ],
  },
  skuCandidates: [],
  offering: {
    stableId: "off_kb_01",
    currentRevision: offering({
      offeringId: "off_kb_01",
      revisionNo: 2,
      status: "PAUSED",
      supplyPriceGross: "129.00",
      supplyPriceNet: "114.16",
      inputTaxRate: "0.13",
      freightAmount: "8.00",
      serviceFeeAmount: "0.00",
      minimumOrderQuantity: "5",
      supplyRegion: ["华东"],
      availabilityStatus: "UNAVAILABLE",
      availableQuantity: "0",
      productCapabilities: ["cancel", "logistics"],
      validFrom: "2026-04-02",
      createdAt: "2026-04-02T10:15:00+08:00",
      immutable: true,
    }),
    revisionHistory: [
      offering({
        offeringId: "off_kb_01",
        revisionNo: 2,
        status: "PAUSED",
        supplyPriceGross: "129.00",
        supplyPriceNet: "114.16",
        inputTaxRate: "0.13",
        freightAmount: "8.00",
        serviceFeeAmount: "0.00",
        minimumOrderQuantity: "5",
        supplyRegion: ["华东"],
        availabilityStatus: "UNAVAILABLE",
        availableQuantity: "0",
        productCapabilities: ["cancel", "logistics"],
        validFrom: "2026-04-02",
        createdAt: "2026-04-02T10:15:00+08:00",
        immutable: true,
      }),
    ],
  },
  poolEntry: poolEntryForSku("sku_kb_01", "168.00", "PAUSED"),
  publicationImpact: {
    ...pauseBase(
      ["ZERO_INVENTORY"],
      [
        {
          id: "ps_z1",
          publicationId: "PUB-20260720-009",
          reason: "ZERO_INVENTORY",
          outboxId: "obx_pause_509",
          status: "PAUSED",
        },
      ]
    ),
    historicalPaidOrderCount: 6,
    recoveryBlocker: {
      code: "RECOVERY_RESPONSIBILITY_UNCONFIRMED",
      message: RECOVERY_BLOCKER_MESSAGE,
    },
    note: "可供数量为零，相关商城商品已暂停销售。供应商恢复供货后也不会自动重新上架。",
  },
  sourceContext: {
    intakeId: "sync_job_9100",
    sourceReference: "batch:sn:20260801-1101",
    receivedAt: "2026-08-01T11:01:00+08:00",
  },
  sourceDiff: [
    {
      id: "z1",
      field: "可供数量",
      before: "80",
      after: "0",
      note: "零库存 ≠ 永久停止供应",
    },
    {
      id: "z2",
      field: "可供状态",
      before: "AVAILABLE",
      after: "UNAVAILABLE",
    },
  ],
  allowedActions: ["BROWSE", "OPEN_CENTER", "OPEN_W20"],
  actionBlockers: [
    {
      action: "CONFIRM_OFFERING_REVISION",
      code: "WORK_ITEM_TYPE_UNREGISTERED",
      message: REGISTRATION_BLOCKER_MESSAGE,
    },
    {
      action: "CLAIM",
      code: "NO_REGISTERED_TASK",
      message: "零库存仅形成暂停与阻断证据，不创建异常任务",
    },
  ],
  costFieldVisibility: "visible",
}

type ActiveSupplySeedInput = {
  id: string
  supplierId: string
  supplierName: string
  connectionId: string
  connectionCode: string
  supplierSkuCode: string
  externalName: string
  category: string
  skuId: string
  skuCode: string
  productName: string
  productRevisionId: string
  specification: string
  baseUnit: string
  priceGross: string
  priceNet: string
  salesVisiblePrice: string
  minimumOrderQuantity: string
  supplyRegion: string[]
}

/** 已生效供给用于商品中心关系视图；正常数据不进入待处理队列。 */
function activeSupplySeed(input: ActiveSupplySeedInput): SupplierCatalogItemView {
  const offeringId = `off_${input.id}`
  const revision = offering({
    offeringId,
    revisionNo: 1,
    status: "ACTIVE",
    supplyPriceGross: input.priceGross,
    supplyPriceNet: input.priceNet,
    inputTaxRate: "0.13",
    freightAmount: "0.00",
    serviceFeeAmount: "0.00",
    minimumOrderQuantity: input.minimumOrderQuantity,
    supplyRegion: input.supplyRegion,
    availabilityStatus: "AVAILABLE",
    availableQuantity: "500",
    productCapabilities: ["cancel", "refund", "logistics"],
    validFrom: "2026-01-01",
    createdAt: "2026-01-01T09:00:00+08:00",
    immutable: true,
  })

  return {
    changeType: "UNCHANGED",
    supplierProduct: {
      id: input.id,
      supplier: { id: input.supplierId, name: input.supplierName },
      source: {
        type: "API",
        label: "API 同步",
        connection: { id: input.connectionId, code: input.connectionCode },
      },
      supplierSpuCode: `EXT-${input.supplierSkuCode}`,
      supplierSkuCode: input.supplierSkuCode,
      status: "ACTIVE",
      currentRevision: rev({
        revisionNo: 1,
        sourceRevisionToken: `src_${input.id}_1`,
        sourceUpdatedAt: "2026-08-01T08:00:00+08:00",
        syncedAt: "2026-08-01T08:02:00+08:00",
        name: input.externalName,
        specification: input.specification,
        category: input.category,
        supplyPriceGross: input.priceGross,
        availableQuantity: "500",
        availabilityStatus: "AVAILABLE",
      }),
    },
    mapping: {
      mappingStatus: "ACTIVE",
      skuId: input.skuId,
      skuCode: input.skuCode,
      skuName: input.productName,
      skuRevisionId: `${input.productRevisionId}:${input.skuId}`,
      specification: input.specification,
      baseUnit: input.baseUnit,
      approvedBy: "采购 · 周然",
      approvedAt: "2026-01-01T09:00:00+08:00",
      reason: "供应商商品与 ERP SKU 为同一可采购规格",
      mappingVersion: `map_${input.id}_v1`,
      history: [
        {
          id: `mh_${input.id}_1`,
          skuCode: input.skuCode,
          status: "已生效",
          at: "2026-01-01",
          note: "当前有效关联",
        },
      ],
    },
    skuCandidates: [],
    offering: {
      stableId: offeringId,
      currentRevision: revision,
      revisionHistory: [revision],
    },
    poolEntry: poolEntryForSku(input.skuId, input.salesVisiblePrice),
    publicationImpact: {
      ...noPause,
      activePublicationCount: 1,
      note: "当前供给关系有效；商城销售价与最小购买量仍由销售发布独立维护。",
    },
    sourceContext: {
      intakeId: `sync_${input.id}`,
      sourceReference: `batch:${input.connectionCode.toLowerCase()}:20260801`,
      receivedAt: "2026-08-01T08:02:00+08:00",
    },
    sourceDiff: [],
    allowedActions: ["BROWSE", "OPEN_CENTER"],
    actionBlockers: [],
    costFieldVisibility: "visible",
  }
}

const ACTIVE_PRODUCT_SUPPLY_SEEDS = [
  activeSupplySeed({
    id: "ep_active_ny_01",
    supplierId: "sup_fresh",
    supplierName: "鲜果直供供应链",
    connectionId: "conn_fresh_01",
    connectionCode: "FRESH-CATALOG",
    supplierSkuCode: "FRESH-NY-CLASSIC",
    externalName: "新春坚果礼盒典藏装",
    category: "礼盒",
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    productName: "新春坚果礼盒 · 典藏款",
    productRevisionId: "prd_1_r6",
    specification: "规格：典藏款",
    baseUnit: "套",
    priceGross: "118.00",
    priceNet: "104.42",
    salesVisiblePrice: "168.00",
    minimumOrderQuantity: "10",
    supplyRegion: ["华东", "华北"],
  }),
  activeSupplySeed({
    id: "ep_active_ny_02",
    supplierId: "sup_fresh",
    supplierName: "鲜果直供供应链",
    connectionId: "conn_fresh_01",
    connectionCode: "FRESH-CATALOG",
    supplierSkuCode: "FRESH-NY-LITE",
    externalName: "新春坚果礼盒轻享装",
    category: "礼盒",
    skuId: "sku_ny_box_02",
    skuCode: "SKU-NY-BOX-02",
    productName: "新春坚果礼盒 · 典藏款",
    productRevisionId: "prd_1_r6",
    specification: "规格：轻享款",
    baseUnit: "套",
    priceGross: "82.00",
    priceNet: "72.57",
    salesVisiblePrice: "128.00",
    minimumOrderQuantity: "20",
    supplyRegion: ["华东", "华南"],
  }),
  activeSupplySeed({
    id: "ep_active_tea_01",
    supplierId: "sup_fresh",
    supplierName: "鲜果直供供应链",
    connectionId: "conn_fresh_01",
    connectionCode: "FRESH-CATALOG",
    supplierSkuCode: "FRESH-TEA-100-PAPER",
    externalName: "礼盒红茶 100g 纸盒装",
    category: "茶叶",
    skuId: "sku_tea_01",
    skuCode: "SKU-TEA-100-PAPER",
    productName: "礼盒红茶",
    productRevisionId: "prd_2_r3",
    specification: "净含量：100g / 包装：纸盒",
    baseUnit: "盒",
    priceGross: "42.00",
    priceNet: "37.17",
    salesVisiblePrice: "58.00",
    minimumOrderQuantity: "12",
    supplyRegion: ["华东", "华南"],
  }),
  activeSupplySeed({
    id: "ep_active_tea_02",
    supplierId: "sup_fresh",
    supplierName: "鲜果直供供应链",
    connectionId: "conn_fresh_01",
    connectionCode: "FRESH-CATALOG",
    supplierSkuCode: "FRESH-TEA-100-TIN",
    externalName: "礼盒红茶 100g 铁罐装",
    category: "茶叶",
    skuId: "sku_tea_02",
    skuCode: "SKU-TEA-100-TIN",
    productName: "礼盒红茶",
    productRevisionId: "prd_2_r3",
    specification: "净含量：100g / 包装：铁罐",
    baseUnit: "盒",
    priceGross: "49.00",
    priceNet: "43.36",
    salesVisiblePrice: "68.00",
    minimumOrderQuantity: "12",
    supplyRegion: ["华东", "华南"],
  }),
  activeSupplySeed({
    id: "ep_active_tea_03",
    supplierId: "sup_tea",
    supplierName: "明前茶业供应链",
    connectionId: "conn_tea_01",
    connectionCode: "TEA-CATALOG",
    supplierSkuCode: "TEA-250-PAPER",
    externalName: "精选红茶 250g 礼盒",
    category: "茶叶",
    skuId: "sku_tea_03",
    skuCode: "SKU-TEA-250-PAPER",
    productName: "礼盒红茶",
    productRevisionId: "prd_2_r3",
    specification: "净含量：250g / 包装：纸盒",
    baseUnit: "盒",
    priceGross: "65.00",
    priceNet: "57.52",
    salesVisiblePrice: "88.00",
    minimumOrderQuantity: "8",
    supplyRegion: ["全国"],
  }),
  activeSupplySeed({
    id: "ep_active_tea_04",
    supplierId: "sup_tea",
    supplierName: "明前茶业供应链",
    connectionId: "conn_tea_01",
    connectionCode: "TEA-CATALOG",
    supplierSkuCode: "TEA-250-TIN",
    externalName: "精选红茶 250g 铁罐",
    category: "茶叶",
    skuId: "sku_tea_04",
    skuCode: "SKU-TEA-250-TIN",
    productName: "礼盒红茶",
    productRevisionId: "prd_2_r3",
    specification: "净含量：250g / 包装：铁罐",
    baseUnit: "盒",
    priceGross: "72.00",
    priceNet: "63.72",
    salesVisiblePrice: "98.00",
    minimumOrderQuantity: "8",
    supplyRegion: ["全国"],
  }),
] as const

export const SUPPLIER_CATALOG_SEED: readonly SupplierCatalogItemView[] = [
  ...ACTIVE_PRODUCT_SUPPLY_SEEDS,
  SEED_STOPPED,
  SEED_ERROR,
  SEED_CHANGED_PRICE,
  SEED_NEW,
  SEED_CHANGED_STOCK,
]
