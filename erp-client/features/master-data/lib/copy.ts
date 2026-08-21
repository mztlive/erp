/**
 * W14 用户可见文案 — 业务语言，不暴露实现术语。
 * 代码字段名（revision / lockVersion）仅限内部使用。
 */

import type { MasterDataResource } from "@/features/master-data/types"

export function masterDataActionLabel(action: string): string {
    switch (action) {
        case "CREATE":
            return "新建"
        case "CREATE_REVISION":
            return "更新资料"
        case "DISABLE":
            return "停用"
        case "VIEW":
            return "查看"
        case "EXPORT_ROW":
            return "导出本行"
        case "MAINTAIN_POLICY":
            return "维护预警策略"
        default:
            return "该操作"
    }
}

export function masterDataSearchPlaceholder(
    resource: MasterDataResource,
): string {
    switch (resource) {
        case "categories":
            return "分类代码、名称"
        case "brands":
            return "品牌代码、名称"
        case "unit-of-measures":
            return "单位代码、名称、符号"
        case "suppliers":
            return "编号、名称、供应商代码"
        case "warehouses":
            return "仓库代码、名称"
        case "products":
            return "商品编号/名称，SKU 编号/名称/规格/条码"
        case "voucher-categories":
            return "编号、名称、SKU"
        case "sellable-items":
            return "SKU 名称/编号、商品编号/名称、规格、条码"
    }
}

/** 导出元信息：枚举原值不上文件。 */
export function lifecycleFilterLabel(value: string): string {
    switch (value) {
        case "enabled":
            return "当前启用"
        case "disabled":
            return "当前停用"
        default:
            return "全部"
    }
}

export function revisionTimingFilterLabel(value: string): string {
    switch (value) {
        case "current":
            return "当前生效"
        case "future":
            return "待生效"
        default:
            return "全部"
    }
}

/** 权限条、表头等短标签 */
export const masterDataCopy = {
    resourceNavAria: "基础资料分类",
    unknownResourceTitle: "找不到该分类",
    unknownResourceDesc: () =>
        "该分类不存在或链接已失效，请从上方分类重新进入。",
    pageTitle: (resourceLabel: string) => `基础资料 · ${resourceLabel}`,
    listDescription: (count: number) =>
        `共 ${count} 条 · 可按启用状态、版本状态筛选 · 按 / 搜索 · 回车打开详情`,
    productListDescription: (count: number) =>
        `共 ${count} 条 · 筛选按商品归属、状态与 SKU 条件分组 · 回车打开详情`,
    supplierListDescription: (count: number) =>
        `共 ${count} 条 · 可按启用状态、资质状态与能力条件筛选 · 按 / 搜索 · 回车打开详情`,
    sellableListDescription: (_count: number) =>
        "点击任一行查看价格、可供区域和供应保障。",
    searchAria: "搜索基础资料",
    sellableItemsHint:
        "公司商品池只显示已上架、资料有效且当前有供给关系的 SKU；销售价来自公司商品主档，采购成本不会在这里展示。",
    filterLifecycleAria: "启用状态",
    filterVersionAria: "版本状态",
    filterProductKindAria: "商品类型",
    versionAll: "版本：全部",
    versionCurrent: "版本：当前生效",
    versionFuture: "版本：待生效",
    colStableNo: "资料编号",
    colName: "名称",
    colVersion: "版本",
    colLifecycle: "启用状态",
    colVersionState: "版本状态",
    colEffective: "生效期间",
    colBlocker: "不可用原因",
    colActions: "操作",
    actionView: "查看",
    actionUpdate: "更新资料",
    actionDisable: "停用",
    actionCreate: "新建",
    actionCreateClosed: "新建（暂不可用）",
    actionExport: "导出",
    actionOpenDetail: "打开完整资料",
    actionBackList: "返回列表",
    permissionModule: "模块：有权",
    permissionResource: (label: string) => `分类：${label} 有权`,
    permissionRole: (role: string) => `角色：${role}`,
    permissionReveal: (ok: boolean) =>
        ok ? "敏感信息：可短时查看" : "敏感信息：不可查看",
    permissionWriteOpen: "维护：可新建与更新",
    permissionWriteWarehouse: "维护：仓库暂不可改",
    permissionExport: (ok: boolean) => (ok ? "导出：允许" : "导出：无权限"),
    warehouseWriteTitle: "仓库资料暂不可维护",
    warehouseWriteBody:
        "目前只能查看仓库信息和库存摘要，不能新建、更新或停用。维护功能尚未开放。",
    eligible: "可选",
    ineligible: "不可选",
    exportDone: "导出已完成",
    previewIdentity: "基本信息",
    previewKeyFacts: "关键信息",
    previewUsability: "哪些业务能选用",
    previewSensitive: "敏感信息（已打码，可短时查看）",
    previewStock: "库存摘要（只读）",
    previewHistory: "资料变更历史",
    previewActionBlocked: "当前无法进行的操作",
    createTitle: (resourceLabel: string) => `新建${resourceLabel}`,
    createDesc:
        "保存后生成资料编号和第一版内容。以后如需修改，请用「更新资料」，历史记录会保留。",
    createSuccessTitle: "已新建",
    createSuccessDesc: "资料已可用。历史业务单据不会自动引用这份新资料。",
    createBlockedTitle: "无法新建",
    createSubmit: "保存",
    createSubmitRejected: "暂不可保存",
    reviseTitle: "更新资料",
    reviseDesc:
        "会生成新一版内容并保留原因与时间；不会改掉历史业务单据里已经用过的那一版。",
    reviseSuccessTitle: "资料已更新",
    reviseSuccessDesc:
        "若立即生效，列表会显示新内容；若指定了未来日期，到期后自动切换。",
    reviseBlockedTitle: "无法更新",
    reviseConflictTitle: "资料已被他人更新",
    reviseConflictHint: "请重新加载最新内容后，再重新填写。",
    reloadAction: "重新加载",
    reviseNameLabel: "名称",
    reviseSubmit: "保存更新",
    disableTitle: "停用资料",
    disableDesc:
        "停用后，业务页面里一般选不到这份资料；资料编号和历史记录会保留，不是删除。",
    disableSuccessTitle: "已停用",
    disableSuccessDesc: "以后业务中默认选不到；历史单据与记录仍可查看。",
    disableBlockedTitle: "无法停用",
    disableSubmit: "确认停用",
    fieldEffectiveFrom: "生效开始",
    fieldEffectiveTo: "生效结束（可留空表示长期）",
    fieldChangeReason: "变更原因",
    fieldDisableAt: "停用时间",
    fieldDisableReason: "停用原因",
    fieldResourceSection: "资源专属信息",
    fieldIdentitySection: "商品信息",
    fieldCatalogSection: "分类与品牌",
    fieldMediaSection: "商品图片（SPU）",
    fieldSpecSection: "规格",
    fieldSkuSection: "SKU（由规格组合生成）",
    fSku: "SKU 编号",
    fSpu: "商品编号",
    fBaseUnit: "基础单位",
    fCategory: "分类",
    fBrand: "品牌",
    fSupplier: "供应商",
    fBarcode: "条码",
    fMainImage: "主图",
    fCarouselImages: "轮播图",
    fDetailImages: "详情图",
    fCostPrice: "成本价",
    fSalePrice: "销售价",
    fMarketPrice: "市场价",
    fProductCode: "产品编码",
    fSkuName: "SKU 名称",
    fSpecName: "规格名",
    fSpecValues: "规格取值",
    fSpecLabel: "规格",
    fSkuCount: "SKU 数",
    productCreateTitle: "新建商品",
    productEditTitle: "商品详情",
    supplierCreateTitle: "新建供应商",
    supplierCreateDesc:
        "维护供应商名称、企业主体与联系人，补充结算、能力与资质附件。保存后进入同一详情页继续维护。",
    productCreateDesc:
        "以 SPU 维护商品，配置规格后组合生成 SKU。主图在 SKU；轮播图与详情图在商品（SPU）。保存后进入同一详情页继续维护。",
    productEditDesc:
        "详情页可直接修改并保存：保存即生成新版本。规格取值变更会重建 SKU 组合，已有主图和商品池价格尽量按规格匹配保留。",
    productSpecsHint:
        "添加规格维度（如颜色、规格），填写取值后自动组合出 SKU。无规格时保留一个默认 SKU。",
    productSkuHint:
        "SKU 的上架状态与启用状态分别管理；公司商品池只读取已上架且当前有供给关系的 SKU。采购成本、供给方式、税费和起订量按供给关系独立维护。",
    productAddSpec: "添加规格",
    productRemoveSpec: "移除规格",
    productRebuildSkus: "按规格重新生成 SKU",
    productSpecValuesPlaceholder: "多个取值用顿号或逗号分隔，如：红、蓝、绿",
    productNoSkus: "尚未生成 SKU",
    productDefaultSpec: "默认规格",
    productMainImageHint: "SKU 主图，单张",
    productSpuMediaHint: "商品级多图，允许为空",
    centerSkuTable: "SKU 列表",
    centerSpecDims: "规格维度",
    fCategoryCode: "分类代码",
    fBrandCode: "品牌代码",
    fBrandLogo: "品牌 Logo",
    brandLogoHint: "1:1 正方形；支持 jpg / png / webp；可留空",
    fParentCategory: "上级分类",
    fProductKind: "适用商品类型",
    categoryTreeTitle: "商品分类树",
    categoryTreeDesc: (count: number) =>
        `共 ${count} 个分类 · 按树形维护上下级 · 停用后业务页默认不可选`,
    categoryTreeEmpty: "暂无分类，请先新建根分类",
    categoryTreeNoMatch: "没有匹配的分类",
    categoryTreeNoMatchDesc:
        "当前搜索或启用状态筛选下没有分类。可调整关键词或清除筛选后重试。",
    categoryTreeSearch: "搜索分类代码或名称",
    categoryAddRoot: "新建根分类",
    categoryAddChild: "新建子分类",
    categoryExpandAll: "全部展开",
    categoryCollapseAll: "全部收起",
    categoryParentRoot: "（根分类）",
    categoryColCode: "分类代码",
    categoryColParent: "上级",
    categoryColKind: "适用类型",
    brandListHint: "品牌字典供商品与 SKU 下拉选用；停用不删除历史引用。",
    fUnitCode: "单位代码",
    fUnitSymbol: "单位符号",
    fQuantityScale: "数量小数位",
    unitListHint:
        "计量单位供公司商品等表单选择基础单位；停用后业务页默认不可选，历史引用保留。单位代码创建后不可修改。",
    fDescription: "类目描述",
    fSalesVisiblePrice: "销售价",
    fSupplierCount: "可供供应商数",
    fRegion: "服务区域",
    fLeadTime: "交期",
    fFulfillmentModes: "允许履约方式",
    fTaxRate: "税率",
    fCompany: "企业主体",
    fContactName: "联系人",
    fContactPhone: "联系电话",
    fAddress: "供应商地址",
    fSettlement: "结算方式",
    fCapability: "能力",
    fBusinessCategory: "经营类目",
    fSigningEntity: "公司签约主体",
    fPaymentEntity: "公司付款主体",
    fQualification: "资质附件",
    supplierQualificationHint:
        "支持图片 / PDF 等文件，可多选；保存后归档为正式附件",
    fContractNo: "合同编号",
    fContractValidFrom: "合同有效期起",
    fContractValidTo: "合同有效期止",
    fContractFile: "合同文件",
    fAuthorizationFile: "授权书文件",
    fAuthorizationValidFrom: "授权书有效期起",
    fAuthorizationValidTo: "授权书有效期止",
    fFoodLicense: "食品经营许可证",
    fLegalPersonIdCard: "供应商法人身份证",
    fTaxNo: "税号",
    fCreditCode: "统一社会信用代码",
    fBankName: "开户银行",
    fBankAccount: "银行账号",
    fInvoiceType: "发票类型",
    fInvoiceTaxRate: "发票税点",
    fInitialScore: "合作期初评分",
    fSupplierRating: "供应商评级",
    fCurrentScore: "合作中评分",
    mediaMainRequired: "请上传主图",
    mediaAllowEmpty: "可选，允许为空",
    mediaUploadHint: "支持 jpg / png / webp，保存后归档为正式图片",
    mediaRemove: "移除",
    mediaEmpty: "未上传",
    mediaCount: (n: number) => (n > 0 ? `${n} 张` : "未上传"),
    resultNo: "资料编号",
    resultVersion: "版本",
    resultVersionState: "版本状态",
    resultEffective: "生效时间",
    resultActor: "操作人",
    resultAt: "操作时间",
    resultReason: "原因",
    versionStateCurrent: "当前生效",
    versionStateFuture: "待生效",
    centerLoading: "正在加载…",
    centerLoadFail: "加载失败，请重试。",
    centerMissingTitle: "找不到这份资料或无权查看",
    centerMissingDesc:
        "停用后的资料通常仍可打开；若确实无权限，则不会显示内容。",
    centerOverview: "概览",
    centerVersions: "变更历史",
    centerRelations: "引用与选用",
    centerAudit: "操作记录",
    centerOverviewDesc: "编号、启用状态、生效期间与分类信息",
    centerVersionsDesc: "每一版的名称、原因与生效期间；改名称不会改掉历史记录",
    centerRelationsDesc: "被业务引用的情况，以及哪些页面还能选到",
    centerAuditDesc: "新建、更新、停用等操作记录（敏感内容不显示原文）",
    centerNoAudit: "暂无操作记录",
    centerCurrentVersion: "当前版本",
    centerChangeReason: "变更原因",
    centerActor: "操作人",
    centerVersionState: "版本状态",
    centerScheduledLifecycle: "计划中的启用变化",
    centerSensitive: "敏感信息",
    centerUpdateBlocked: (msg: string) => `暂时不能更新资料：${msg}`,
    centerDisableBlocked: (msg: string) => `暂时不能停用：${msg}`,
    centerSpecNote:
        "SKU 由规格组合生成；已被业务使用的商品不可改基础单位。规格由规格项自动组合生成，不在界面单独维护。",
    centerHistoryName: "当时名称",
    centerUsageCount: (n: number) => `约被业务引用 ${n} 次。`,
    lifecycleEnabled: "当前启用",
    lifecycleDisabled: "当前停用",
    timingCurrent: "当前生效",
    timingFuture: "待生效",
    timingHistorical: "已结束",
} as const
