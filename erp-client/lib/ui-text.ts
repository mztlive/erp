/**
 * 用户可见文案常量（对齐 docs/ui-glossary.md）。
 *
 * 适用范围：跨页复用的按钮、状态条、结果反馈、跳转名、版本/新鲜度标签。
 * 不适用：页面专属业务说明、错误对象 key、代码注释。
 *
 * 新写跨页文案时优先从此处引用；命中术语表禁用词不得手写绕过。
 */

import { WORKSPACE_ROUTES, type WorkspaceId } from "@/lib/workspace-registry"

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

// ─── 连续处理条 / 队列动作 ───────────────────────────────────────────────────

export const sequentialText = {
    goProcess: "前往处理",
    goProcessReturnQueue: "前往处理 · 处理完返回队列",
    completeCurrent: "完成当前项",
    completeAndNext: "完成并处理下一条",
    completeAndOpenNext: "完成并打开下一条",
    process: "处理",
    submitting: "正在提交…",
    submittingResult: "正在提交处理结果…",
    minePending: "待我处理",
    decisionSubmitting: "决定正在提交",
} as const

// ─── 结果反馈（禁止「正式结果 / 幂等键」） ───────────────────────────────────

export const resultText = {
    unknown: "处理结果待确认",
    unknownDoNotResubmit: "处理结果待确认，请勿重复提交",
    recorded: "处理结果已记录",
    querySucceeded: "已查到处理结果",
    querySucceededOriginal: "已查到原任务处理结果",
    queryNotFound: "未找到该次操作记录",
    queryRetryWithTaskNo: "未查到处理结果，请使用原任务号重试",
    retryWithOriginalTaskNo: "请使用原任务号重试",
    useOriginalTaskNoRetry: "使用原任务号重试",
    originalTaskNo: "原任务号",
    originalTaskId: "原任务标识",
    taskNoTail: "任务号尾号",
    skipAlreadyProcessed: "已处理跳过",
    preventDuplicate: "防重复与不可覆盖",
    operationSucceeded: "操作已完成",
    operationRejected: "操作未通过",
    operationBlocked: "操作被阻断",
    operationProcessing: "操作正在处理",
} as const

// ─── 数据版本 / 新鲜度（禁止「指纹 / 水位 / 对象版本」） ─────────────────────

export const versionText = {
    dataVersion: "数据版本",
    version: "版本",
    versionStatus: "版本状态",
    versionChanged: "数据版本已变化",
    versionChangedRefresh: "数据已变更，请刷新后重试",
    versionMismatchBlocked: "数据版本不匹配，已阻断。请刷新后重试",
    checksumShort: "校验码（短）",
    packageChecksum: "数据包校验码",
    rejectAtVersion: "驳回时数据版本",
} as const

export const freshnessText = {
    dataUpdatedAt: "数据更新时间",
    dataUpdatedAtPrefix: "数据更新于",
    syncProgress: "同步进度",
    latestSyncAt: "最新同步时间",
    catalogSyncAt: "目录同步时间",
    queueUpdatedAt: "队列更新时间",
    mayBeStale: "数据可能不是最新，请刷新",
    lastSuccessKept: "当前显示的是上次成功数据，业务记录未被修改。",
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
    W09: "履约处理",
    W10: "库存台账",
    W11: "客户往来",
    W12: "供应商往来",
    W13: "卡券票款复核",
    W14: "基础资料",
    W15: "客户经营质量",
    W16: "实际经营盈亏",
    W18: "导入与期初",
    W19: "权限与审计",
    W20: "API 供应商连接",
    W21: "商品供给",
    W26: "供应商订单",
    W27: "API 结算",
    W29: "接口错误与对账中心",
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

export function goToWorkspaceLabel(id: WorkspaceId): string {
    return `前往${workspaceLabel(id)}`
}

const NEXT_ACTION_HINT_BY_TYPE_LABEL: Record<string, string> = {
    采购二次确认: "进入采购确认页后，逐行确认可供数量；确认通过后销售单才会生效。",
    待采购建单: "打开采购单页，按销售明细剩余数量选择本次采购数量并创建草稿。",
    待供给分配: "进入供给分配页后确认库存优先的自动推荐；库存不足部分再创建采购单。",
    低毛利销售审批:
        "进入销售单后，确认是否按原条件承接；通过后仍需采购再次确认供货。",
    采购单财务审核:
        "进入后核对供应商、含税成本、进项税和付款条件，再提交通过或驳回。",
    销售变更履约影响复核: "进入销售单后，核对本次变更对履约的影响并提交结论。",
    销售变更财务复核: "进入销售单后，核对本次变更对金额的影响并提交结论。",
    卡券票款复核: "进入票款复核页后，核对准期初回款与开票事实。",
    卡券票款差异复核: "进入票款复核页后，核对差额并提交复核结论。",
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

// ─── 接口错误类指引 ──────────────────────────────────────────────────────────

export const interfaceText = {
    duplicateCallbackIgnored: "重复通知将忽略，不会重复形成业务记录或待办。",
} as const

// ─── 纸质单据 / 通用页脚 ─────────────────────────────────────────────────────

export const documentText = {
    printFooter: "此单据为系统数据的打印件",
    effectiveVersionNote: "请以系统当前有效版本为准",
} as const
