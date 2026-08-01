import type { StatusTone } from "@/components/ui/status-badge"

export type WorkspaceTaskFilter = "all" | "today" | "overdue" | "sync"

export type WorkspaceTask = Readonly<{
  id: string
  filterTags: readonly Exclude<WorkspaceTaskFilter, "all">[]
  taskType: string
  businessObject: string
  counterparty: string
  enteredAt: string
  enteredDateTime: string
  dueAt: string
  dueDateTime: string
  responsibleParty: string
  reason: string
  impact: string
  status: { label: string; tone: StatusTone }
  href: string
  actionLabel: string
}>

/** 工作台样板数据，只表达 UI 场景，不定义正式业务口径。 */
export const WORKSPACE_TASKS: readonly WorkspaceTask[] = [
  {
    id: "task_confirm_01",
    filterTags: ["today"],
    taskType: "采购二次确认",
    businessObject: "XS20260328001",
    counterparty: "星河制造股份有限公司",
    enteredAt: "今天 08:42",
    enteredDateTime: "2026-08-01T08:42:00+08:00",
    dueAt: "今天 11:30",
    dueDateTime: "2026-08-01T11:30:00+08:00",
    responsibleParty: "采购部 · 王敏",
    reason: "销售单已提交，供应商与成本信息待确认",
    impact: "确认后生成采购执行任务并锁定成本口径",
    status: { label: "待处理", tone: "warning" },
    href: "/procurement/confirm",
    actionLabel: "开始处理",
  },
  {
    id: "task_confirm_02",
    filterTags: ["overdue"],
    taskType: "采购二次确认",
    businessObject: "XS20260327012",
    counterparty: "北辰能源集团",
    enteredAt: "昨天 16:18",
    enteredDateTime: "2026-07-31T16:18:00+08:00",
    dueAt: "已超期 42 分钟",
    dueDateTime: "2026-08-01T09:00:00+08:00",
    responsibleParty: "采购部 · 王敏",
    reason: "客户履约日期提前，采购计划尚未确认",
    impact: "可能影响 8 月 3 日首批交付",
    status: { label: "已超期", tone: "destructive" },
    href: "/procurement/confirm",
    actionLabel: "优先处理",
  },
  {
    id: "task_contract_01",
    filterTags: ["today"],
    taskType: "合同信息补全",
    businessObject: "HT-2026-0312",
    counterparty: "星河制造股份有限公司",
    enteredAt: "今天 09:05",
    enteredDateTime: "2026-08-01T09:05:00+08:00",
    dueAt: "今天 17:00",
    dueDateTime: "2026-08-01T17:00:00+08:00",
    responsibleParty: "销售部 · 王敏",
    reason: "付款条件缺少客户确认附件",
    impact: "不影响看单，提交变更前必须补齐",
    status: { label: "待补充", tone: "info" },
    href: "/sales/orders?search=XS20260328001",
    actionLabel: "查看销售单",
  },
  {
    id: "task_sync_01",
    filterTags: ["sync"],
    taskType: "商城同步异常",
    businessObject: "SYNC-20260801-017",
    counterparty: "商城销售单投影",
    enteredAt: "今天 09:21",
    enteredDateTime: "2026-08-01T09:21:00+08:00",
    dueAt: "今天 10:21",
    dueDateTime: "2026-08-01T10:21:00+08:00",
    responsibleParty: "系统管理员",
    reason: "2 张卡券销售单的客户映射未命中",
    impact: "ERP 只读投影延迟，商城原单不受影响",
    status: { label: "同步异常", tone: "destructive" },
    href: "/sales/orders?search=卡券",
    actionLabel: "查看影响",
  },
  {
    id: "task_receipt_01",
    filterTags: ["today"],
    taskType: "回款事实复核",
    businessObject: "XS20260326009",
    counterparty: "海纳教育科技有限公司",
    enteredAt: "今天 09:34",
    enteredDateTime: "2026-08-01T09:34:00+08:00",
    dueAt: "今天 16:00",
    dueDateTime: "2026-08-01T16:00:00+08:00",
    responsibleParty: "财务部 · 陈琳",
    reason: "银行流水与客户结算主体名称存在简称差异",
    impact: "复核前不计入已回款金额",
    status: { label: "待复核", tone: "warning" },
    href: "/sales/orders?search=海纳教育",
    actionLabel: "查看对象",
  },
  {
    id: "task_fulfillment_01",
    filterTags: ["overdue"],
    taskType: "履约异常",
    businessObject: "XS20260312008",
    counterparty: "云帆物流集团",
    enteredAt: "昨天 14:06",
    enteredDateTime: "2026-07-31T14:06:00+08:00",
    dueAt: "已超期 1 小时",
    dueDateTime: "2026-08-01T08:30:00+08:00",
    responsibleParty: "履约组 · 刘青",
    reason: "电子交付批次尚未回传接收结果",
    impact: "200 份夜班餐补包状态待确认",
    status: { label: "已超期", tone: "destructive" },
    href: "/sales/orders?search=XS20260312008",
    actionLabel: "查看销售单",
  },
]

export const WORKSPACE_ALERTS = [
  {
    id: "alert_sync",
    title: "商城投影数据延迟",
    description: "最近成功同步于 09:18，2 张销售单正在等待映射。",
    tone: "warning" as const,
  },
  {
    id: "alert_due",
    title: "2 项任务已超期",
    description: "最早超期 1 小时，建议优先处理采购确认与履约异常。",
    tone: "destructive" as const,
  },
] as const

export const RECENT_WORK = [
  { id: "recent_1", label: "XS20260328001 · 星河制造", href: "/sales/orders?search=XS20260328001" },
  { id: "recent_2", label: "采购二次确认 · 待处理队列", href: "/procurement/confirm" },
  { id: "recent_3", label: "XS20260312008 · 云帆物流", href: "/sales/orders?search=XS20260312008" },
] as const
