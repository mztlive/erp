/**
 * 用户可见文案常量（对齐 docs/ui-glossary.md）。
 *
 * 适用范围：跨页复用的按钮、状态条、结果反馈、跳转名、版本/新鲜度标签。
 * 不适用：页面专属业务说明、错误对象 key、代码注释。
 *
 * 新写跨页文案时优先从此处引用；命中术语表禁用词不得手写绕过。
 */

import { WORKSPACE_ROUTES, type WorkspaceId } from "@/lib/workspace-registry"

// ─── 处理权限（内部称租约，禁止对用户说「租约」） ─────────────────────────────

export const leaseText = {
    /** 对齐术语表 §3.2「任务待领取 → 任务待认领」 */
    unclaimed: "任务待认领",
    active: "正在处理中",
    activeDoNotReopen: "正在处理中 · 请勿重复打开",
    renewing: "处理权限已延期",
    /** 短状态（徽章） */
    lost: "操作已失效",
    /** 带下一步指引 */
    lostRefresh: "操作已失效，请刷新后重新处理",
    released: "本次处理已结束",
    reclaimHint: "领取任务后即可开始处理",
    reclaimAfterLost: "可领取后处理",
    permissionRevoked: "权限已收回，不能提交",
    permissionRevokedCleared: "权限已收回 · 临时信息已清除",
    claimedByOther: "此任务已被其他人领取，请稍后再试",
    reclaimed: "操作已失效，请重新领取",
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
    pendingClaim: "任务待认领",
    teamUnclaimed: "团队待认领",
    minePending: "待我处理",
    decisionSubmitting: "决定正在提交",
    /** 首次领取（从未领取过）与失效后重新领取要区分，避免「重新领取」误导。 */
    claim: "领取任务",
    claiming: "正在领取",
    reclaim: "重新领取",
    reclaiming: "正在重新领取",
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
    W17: "商城同步与映射",
    W18: "导入与期初",
    W19: "权限与审计",
    W20: "API 供应商连接",
    W21: "商品供给",
    W22: "商品发布",
    W23: "执行信息",
    W25: "商城消费订单",
    W26: "供应商订单",
    W27: "API 结算",
    W28: "卡券经营分析",
    W29: "接口错误与对账中心",
    W30: "历史消费回填",
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

// ─── 任务类型 → 主操作按钮 ──────────────────────────────────────────────────

const actionLabels = {
    confirmProcurement: "去确认采购计划",
    reviewCardFunds: "去复核卡券票款",
    handleMappingException: "去处理映射异常",
    reconcileReceipt: "去核对回款",
    handleFulfillment: "去处理",
} as const

const WORK_ITEM_ACTION_BY_TYPE_LABEL: Record<string, string> = {
    采购二次确认: actionLabels.confirmProcurement,
    卡券票款复核: actionLabels.reviewCardFunds,
    映射异常处理: actionLabels.handleMappingException,
    回款事实复核: actionLabels.reconcileReceipt,
    回款核对: actionLabels.reconcileReceipt,
    履约作业: actionLabels.handleFulfillment,
    收货与发货: actionLabels.handleFulfillment,
    交付与代发: actionLabels.handleFulfillment,
    电子履约: actionLabels.handleFulfillment,
    实物履约: actionLabels.handleFulfillment,
    服务履约: actionLabels.handleFulfillment,
}

/** 按任务类型中文名生成主按钮文案；未登记类型兜底「前往处理」。 */
export function actionLabelForWorkItemType(
    workItemTypeLabel: string | null | undefined,
): string {
    if (!workItemTypeLabel) return sequentialText.goProcess
    return (
        WORK_ITEM_ACTION_BY_TYPE_LABEL[workItemTypeLabel] ??
        sequentialText.goProcess
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
