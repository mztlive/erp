/**
 * 用户可见文案常量。
 *
 * 适用范围：跨页复用的按钮、状态条、结果反馈、跳转名、版本/新鲜度标签。
 * 不适用：页面专属业务说明、错误对象 key、代码注释。
 *
 * 新写跨页文案时优先从此处引用；命中禁用词不得手写绕过（禁用词见 `erp-client/AGENTS.md` §5）。
 */

import { WORKSPACE_ROUTES, type WorkspaceId } from "@/lib/workspace-registry"

export const tableActionText = {
    more: "更多",
} as const

export const comboboxText = {
    refresh: "刷新选项",
    refreshing: "正在刷新…",
    refreshed: "选项已刷新",
    refreshFailed: "选项刷新失败，请重试；已填写内容仍保留",
} as const

export const fileUploadText = {
    upload: "上传文件",
    select: "选择文件",
    replace: "更换文件",
    add: "继续添加",
    remove: "移除",
    hint: "点击选择文件，或将文件拖到此处",
    singleOnly: "请每次选择一个文件",
    unsupported: "文件类型不支持，请按说明重新选择",
    selected: "已选择",
    uploaded: "已上传",
    saved: "已保存",
    pendingSave: "已选择 · 保存时上传",
    pendingSubmit: "已选择 · 提交时上传",
    uploadedUnsaved: "已上传 · 尚未保存到单据",
    preview: "预览",
    batchRejected: "本次文件均未添加，原有文件保持不变。",
    empty: "暂无文件",
} as const

export const listFilterText = {
    selected: "已选条件：",
    clear: "清空",
    more: "更多筛选",
    close: "关闭更多筛选",
    reset: "重置",
    cancel: "取消",
    apply: "应用筛选",
} as const

// ─── 当前责任 ───────────────────────────────────────────────────────────────

export const responsibilityText = {
    assignedToMe: "由你处理",
    assignedToOther: "由其他同事处理",
    blocked: "当前不可处理",
    completed: "已完成",
    closed: "已关闭",
    changed: "处理权已变化，请刷新",
    reassign: "转交",
    permissionRevoked: "权限已收回，不能提交",
    permissionRevokedCleared: "权限已收回 · 临时信息已清除",
    editing: "正在编辑中",
    enteringEdit: "正在进入编辑",
    cannotEdit: "无法进入编辑",
    currentProcessState: "当前处理状态",
} as const

// ─── 结果反馈（禁止「正式结果 / 幂等键」） ───────────────────────────────────

export const resultText = {
    unknown: "处理结果待确认",
    unknownDoNotResubmit: "处理结果待确认，请勿重复提交",
    recorded: "处理结果已记录",
    querySucceeded: "已查到处理结果",
    querySucceededOriginal: "已查到上次提交的处理结果",
    queryNotFound: "未找到该次操作记录",
    queryRetryWithTaskNo: "暂未查到处理结果，可点击安全重试（不会重复提交）",
    retryWithOriginalTaskNo: "请点击安全重试，系统会自动避免重复提交",
    useOriginalTaskNoRetry: "安全重试（不重复提交）",
    originalTaskNo: "本次操作编号",
    originalTaskId: "本次操作编号",
    taskNoTail: "操作尾号",
    skipAlreadyProcessed: "已处理跳过",
    preventDuplicate: "防重复提交保护",
    operationSucceeded: "操作已完成",
    operationRejected: "操作未通过",
    operationBlocked: "暂无法继续操作",
    operationProcessing: "操作正在处理",
} as const

export const systemFeedbackText = {
    source: "来源工作页",
    returnToSource: "返回来源工作页",
    copy: "复制问题信息",
    copied: "问题信息已复制",
    technicalDetails: "技术详情",
    requiredPermission: "缺少的权限码",
    errorCode: "错误码",
    requestId: "请求编号",
    permissionHelp: "请联系管理员核查访问权限。",
    verifyOperation: "请返回来源工作页核对最新状态。结果确认前请勿重复提交。",
    committedResultUnavailable: "操作已提交，暂时无法读取结果",
    checkCommittedResult: "请返回来源工作页查询处理状态。",
} as const

export const approvalResultText = {
    approved: "通过",
    rejected: "驳回",
    cancelled: "已取消",
    rejectionReason: "驳回原因",
} as const

// ─── 数据版本 / 新鲜度（禁止「指纹 / 水位 / 对象版本」） ─────────────────────

export const versionText = {
    dataVersion: "单据版次",
    version: "版次",
    versionStatus: "版次状态",
    versionChanged: "单据内容已被更新",
    versionChangedRefresh: "单据已变更，请刷新后重试",
    versionMismatchBlocked: "当前内容已被他人或新操作更新，请刷新页面后重试",
    checksumShort: "文件核对码",
    packageChecksum: "文件完整性核对码",
    rejectAtVersion: "驳回时版次",
} as const

export const freshnessText = {
    dataUpdatedAt: "数据更新时间",
    dataUpdatedAtPrefix: "数据更新于",
    syncProgress: "同步进度",
    latestSyncAt: "最新同步时间",
    catalogSyncAt: "目录同步时间",
    queueUpdatedAt: "列表刷新时间",
    mayBeStale: "数据可能不是最新，请刷新",
    lastSuccessKept: "刷新未成功，当前暂显示已加载的内容。",
} as const

// ─── 工作面短名（用户提示禁止写 Wxx） ────────────────────────────────────────

/**
 * 跨页提示用的短名。未列出的回退到 `WORKSPACE_ROUTES.name`。
 * 完整路由名仍以 registry 为准（导航、页眉）。
 */
const WORKSPACE_SHORT_LABEL: Partial<Record<WorkspaceId, string>> = {
    W01: "今日工作台",
    W02: "待办队列",
    W03: "客户中心",
    W04: "合同",
    W05: "销售单",
    W06: "客户验收",
    W07: "采购二次确认",
    W08: "采购单",
    /** 跨页提示用中性短名；侧栏按岗位分「收货与发货 / 交付与代发」 */
    W09: "收发货办理",
    W10: "库存台账",
    W11: "客户往来",
    W12: "供应商往来",
    W14: "基础资料",
    W15: "客户经营质量",
    W16: "实际经营盈亏",
    W18: "导入与期初",
    W19: "权限与审计",
    W20: "供应商平台连接",
    W21: "供应商供货目录",
    W26: "供应商订单",
    W27: "供应商平台结算",
    W29: "同步异常与对账",
}

const workspaceNameById = new Map(
    WORKSPACE_ROUTES.map((entry) => [entry.id, entry.name] as const),
)

/** 用户提示中的工作面名称（禁止出现 W01–W30）。 */
export function workspaceLabel(id: WorkspaceId): string {
    return WORKSPACE_SHORT_LABEL[id] ?? workspaceNameById.get(id) ?? id
}

export function openWorkspaceLabel(id: WorkspaceId): string {
    return `打开${workspaceLabel(id)}`
}

const NEXT_ACTION_HINT_BY_TYPE_LABEL: Record<string, string> = {
    采购二次确认: "进入采购确认页后，逐行确认可供数量；确认通过后销售单才会生效。",
    待采购建单: "打开采购单页，按销售明细剩余数量选择本次采购数量并创建草稿。",
    待供给分配: "进入货源分配页后确认库存优先的自动推荐；库存不足部分再创建采购单。",
    待安排采购: "进入货源分配页后确认库存优先的自动推荐；库存不足部分再创建采购单。",
    低毛利销售审批:
        "进入销售单后，确认是否按原条件承接；通过后仍需采购再次确认供货。",
    采购单财务审核:
        "进入后核对供应商、含税成本、进项税和付款条件，再提交通过或驳回。",
    销售变更履约影响复核: "进入销售单后，核对本次变更对履约的影响并提交结论。",
    销售变更财务复核: "进入销售单后，核对本次变更对金额的影响并提交结论。",
}

/** 按任务类型生成进入处理后的下一步说明。 */
export function nextActionHintForWorkItemType(
    workItemTypeLabel: string | null | undefined,
): string {
    if (!workItemTypeLabel) return "进入对应页面后提交处理结论。"
    return (
        NEXT_ACTION_HINT_BY_TYPE_LABEL[workItemTypeLabel] ??
        "进入对应页面后提交处理结论。"
    )
}

// ─── 数据范围（M07/M08/M09 资金往来范围行共用） ─────────────────────────────

export const scopeText = {
    visibleShare: "本人权限内金额",
    limitedOnlyVisibleShare: "部分金额不在查看范围内，仅统计本人权限内金额",
    wholeRestricted: "无整单金额查看权限",
    unassignedShare: "未分配业务归属",
    scopeChangedRetry: "您的数据查看范围已更新，请从第一页重新查询",
    candidateUnavailable: "已选人员（当前不可见）",
} as const

// ─── 接口错误类指引 ──────────────────────────────────────────────────────────

export const interfaceText = {
    duplicateCallbackIgnored: "重复通知将忽略，不会重复形成业务记录或待办。",
} as const

// ─── 纸质单据 / 通用页脚 ─────────────────────────────────────────────────────

export const documentText = {
    printFooter: "此单据为系统数据的打印件",
    effectiveVersionNote: "请以系统当前有效版本为准",
} as const

// ─── 销售变更对采购的影响（第一期 §6.3） ─────────────────────────────────────

export const purchaseSourceUpdateText = {
    title: "关联销售单已变更，待核对采购单",
    maintain: "登记维持原采购",
    paymentPaused: "暂停付款",
    noPurchasePermission: "当前账号没有采购单查看权限，看不到具体采购单。",
    completed: "采购处理完成",
    awaitingSubmission: "已修改，待提交审批",
    awaitingSubmissionTitle: "已按最新销售单修改，待提交审批",
    awaitingSubmissionDescription:
        "本采购单已按变更后的销售单调整，提交审批后即完成更新。",
} as const
