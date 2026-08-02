import type { WorkspacePageDef } from "@/features/workspace-kit/types"
import type { WorkspaceId } from "@/lib/workspace-registry"
import { actionLabels, freshnessText, sequentialText } from "@/lib/ui-text"

const money = (n: number) =>
  new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
  }).format(n)

function listRows(
  items: Array<{
    id: string
    cells: Record<string, string>
    status?: {
      label: string
      tone: "success" | "warning" | "destructive" | "info" | "neutral"
    }
    href?: string
    metricTags?: readonly string[]
    filterTags?: readonly string[]
  }>
) {
  return items
}

/** Domain page definitions for workspaces implemented via shared shells. */
export const WORKSPACE_PAGE_DEFS: Partial<
  Record<WorkspaceId, WorkspacePageDef>
> = {
  W02: {
    id: "W02",
    title: "统一待办队列",
    description:
      "在可恢复的队列上下文中连续处理任务；先读懂对象、原因与影响，再做正式决策。",
    mode: "M3",
    breadcrumbs: [
      { id: "work", label: "工作", href: "/workspace" },
      { id: "tasks", label: "待办队列" },
    ],
    shell: {
      kind: "queue",
      payload: {
        scopeLabels: ["我的待办", "待领取", "团队"],
        tasks: [
          {
            id: "wi_pc_01",
            taskType: "采购二次确认",
            businessObject: "销售单 · XS20260328001",
            counterparty: "星河制造股份有限公司",
            enteredAt: "今天 08:42",
            enteredDateTime: "2026-08-01T08:42:00+08:00",
            dueAt: "今天 11:30",
            dueDateTime: "2026-08-01T11:30:00+08:00",
            responsibleParty: "采购部 · 王敏",
            reason: "销售单已提交，供应商与成本信息待确认",
            impact: "确认后生成采购执行任务并锁定成本口径",
            status: { label: "待处理", tone: "warning" },
            summaryFields: [
              { label: "任务类型", value: "采购二次确认" },
              { label: "版本", value: "v1" },
              { label: "优先级", value: "高" },
              { label: "截止", value: "今天 11:30", numeric: true },
            ],
            checkItems: [
              "供应商资质在有效期内",
              "成本覆盖全部明细",
              "交付方式与客户要求一致",
            ],
            actionLabel: actionLabels.confirmProcurement,
            handlerHref: "/procurement/confirm?scope=mine&currentWorkItemId=wi_pc_01",
            scopeTags: ["我的待办", "团队"],
          },
          {
            id: "wi_card_02",
            taskType: "卡券票款复核",
            businessObject: "销售单 · XS20260325008",
            counterparty: "蓝湾集团",
            enteredAt: "昨天 15:20",
            enteredDateTime: "2026-07-31T15:20:00+08:00",
            dueAt: "已超期 1 小时",
            dueDateTime: "2026-08-01T09:00:00+08:00",
            responsibleParty: "财务部 · 待领取",
            reason: "商城回款与开票金额待与 ERP 应收对齐",
            impact: "未复核前票款数据不可作为经营结果",
            status: { label: "待领取", tone: "info" },
            scopeTags: ["待领取"],
            actionLabel: actionLabels.reviewCardFunds,
            handlerHref: "/finance/card-funds-review",
            summaryFields: [
              { label: "任务类型", value: "卡券票款复核" },
              { label: "应收余额", value: money(86000) },
              { label: "待复核回款", value: money(42000) },
              { label: "优先级", value: "紧急" },
            ],
          },
          {
            id: "wi_map_03",
            taskType: "映射异常处理",
            businessObject: "同步批次 · SYNC-20260801-017",
            counterparty: "华东商城",
            enteredAt: "今天 09:10",
            enteredDateTime: "2026-08-01T09:10:00+08:00",
            dueAt: "今天 18:00",
            dueDateTime: "2026-08-01T18:00:00+08:00",
            responsibleParty: "运营 · 李倩",
            reason: "外部商品缺少可销售项目映射",
            impact: "阻断 12 条消费订单入账",
            status: { label: "处理中", tone: "info" },
            scopeTags: ["团队"],
            actionLabel: actionLabels.handleMappingException,
            handlerHref: "/governance/mall-sync",
            summaryFields: [
              { label: "任务类型", value: "映射异常" },
              { label: "影响对象", value: "12 条消费订单" },
              { label: "责任角色", value: "运营映射" },
              { label: "截止", value: "今天 18:00", numeric: true },
            ],
          },
        ],
      },
    },
  },

  W03: {
    id: "W03",
    title: "客户中心",
    description:
      "围绕稳定企业客户身份，查看负责人、联系结算、合同销售与票款摘要。",
    mode: "M4",
    breadcrumbs: [
      { id: "sales", label: "销售", href: "/sales/orders" },
      { id: "customers", label: "客户中心" },
    ],
    shell: {
      kind: "object",
      payload: {
        scopeLabels: ["我的客户", "协作客户", "团队客户"],
        searchPlaceholder: "名称、编码、统一社会信用代码",
        primaryActionLabel: "新建客户",
        items: [
          {
            id: "cust_128",
            title: "东方企业服务有限公司",
            subtitle: "负责销售：王敏 · 协作 2 人",
            code: "KH-000128",
            status: { label: "启用", tone: "success" },
            owner: "王敏",
            updatedAt: "2026-07-28",
            scopeTags: ["我的客户", "协作客户", "团队客户"],
            metrics: [
              { label: "有效合同", value: "3" },
              { label: "进行中销售单", value: "8" },
              { label: "应收余额", value: money(186400) },
              { label: "逾期金额", value: money(12000) },
            ],
            sections: [
              {
                id: "identity",
                title: "主体身份与客户角色",
                fields: [
                  { label: "法定名称", value: "东方企业服务有限公司" },
                  { label: "客户简称", value: "东方企业" },
                  { label: "统一社会信用代码", value: "91310000MA1KXXXX1X" },
                  { label: "默认付款条件", value: "货到 30 日内付款" },
                  { label: "主数据版本", value: "v4 · 2026-07-01 生效" },
                ],
              },
              {
                id: "contacts",
                title: "联系与地址",
                fields: [
                  { label: "默认联系人", value: "张工 · 138****6210" },
                  { label: "履约地址", value: "上海市浦东新区 ****" },
                  { label: "开票资料", value: "已确认税务身份" },
                ],
              },
              {
                id: "related",
                title: "合同与销售",
                fields: [
                  { label: "最近合同", value: "HT-2026-0312 · 将到期" },
                  { label: "最近销售单", value: "XS20260328001 · 待二次确认" },
                ],
              },
              {
                id: "finance",
                title: "票款摘要",
                fields: [
                  { label: "应收余额", value: money(186400) },
                  { label: "逾期金额", value: money(12000) },
                  { label: "最早逾期日", value: "2026-07-15" },
                ],
              },
            ],
          },
          {
            id: "cust_204",
            title: "星河制造股份有限公司",
            subtitle: "负责销售：王敏 · 协作 1 人",
            code: "KH-000204",
            status: { label: "启用", tone: "success" },
            owner: "王敏",
            scopeTags: ["我的客户", "团队客户"],
            metrics: [
              { label: "有效合同", value: "2" },
              { label: "进行中销售单", value: "4" },
              { label: "应收余额", value: money(428500.5) },
              { label: "逾期金额", value: money(0) },
            ],
            sections: [
              {
                id: "identity",
                title: "主体身份与客户角色",
                fields: [
                  { label: "法定名称", value: "星河制造股份有限公司" },
                  { label: "客户编号", value: "KH-000204" },
                  { label: "负责销售", value: "王敏" },
                ],
              },
            ],
          },
          {
            id: "cust_311",
            title: "北辰能源集团",
            subtitle: "负责销售：李倩 · 需关注逾期",
            code: "KH-000311",
            status: { label: "启用", tone: "warning" },
            owner: "李倩",
            scopeTags: ["团队客户"],
            metrics: [
              { label: "有效合同", value: "1" },
              { label: "进行中销售单", value: "2" },
              { label: "应收余额", value: money(96000) },
              { label: "逾期金额", value: money(36000) },
            ],
            sections: [
              {
                id: "identity",
                title: "主体身份",
                fields: [
                  { label: "法定名称", value: "北辰能源集团" },
                  { label: "客户编号", value: "KH-000311" },
                ],
              },
            ],
          },
        ],
      },
    },
  },

  W04: {
    id: "W04",
    title: "合同",
    description: "上传和查询合同 PDF、有效期与关联销售单；对象中心只读核对归档版本。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "sales", label: "销售", href: "/sales/orders" },
      { id: "contracts", label: "合同" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "合同号、客户、结算主体",
        primaryActionLabel: "上传合同 PDF",
        filterLabels: ["全部", "有效", "将到期", "已终止"],
        metrics: [
          { key: "all", label: "全部合同", value: 24, detail: "当前业务范围" },
          { key: "active", label: "有效", value: 18, detail: "可关联建单" },
          { key: "expiring", label: "将到期", value: 3, detail: "30 日内" },
          { key: "ended", label: "已终止", value: 3, detail: "只读历史" },
        ],
        columns: [
          { key: "number", header: "合同号" },
          { key: "customer", header: "客户" },
          { key: "entity", header: "结算主体" },
          { key: "period", header: "有效期", numeric: true },
          { key: "sales", header: "关联销售单" },
          { key: "owner", header: "负责人" },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "ct_1",
            cells: {
              number: "HT-2026-0312",
              customer: "星河制造股份有限公司",
              entity: "星河制造股份有限公司",
              period: "2026-01-01 ~ 2026-12-31",
              sales: "2",
              owner: "王敏",
              status: "有效",
            },
            status: { label: "有效", tone: "success" },
          },
          {
            id: "ct_2",
            cells: {
              number: "HT-2026-0288",
              customer: "青禾科技有限公司",
              entity: "青禾科技有限公司",
              period: "2026-03-01 ~ 2026-08-31 · 将到期",
              sales: "1",
              owner: "王敏",
              status: "将到期",
            },
            status: { label: "将到期", tone: "warning" },
          },
          {
            id: "ct_3",
            cells: {
              number: "HT-2025-1190",
              customer: "北辰能源集团",
              entity: "北辰能源集团",
              period: "2025-01-01 ~ 2025-12-31",
              sales: "5",
              owner: "李倩",
              status: "已终止",
            },
            status: { label: "已终止", tone: "neutral" },
          },
        ]),
      },
    },
  },

  W08: {
    id: "W08",
    title: "采购单",
    description:
      "集中核对采购单状态、供应商、金额与审核进度；支持预览与进入对象中心。",
    mode: "M2+M4+M5",
    breadcrumbs: [
      { id: "proc", label: "采购与履约", href: "/procurement/confirm" },
      { id: "orders", label: "采购单" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "采购单号、供应商、来源销售单",
        primaryActionLabel: "新建采购单",
        filterLabels: ["全部", "草稿", "待审核", "已生效"],
        metrics: [
          { key: "all", label: "全部采购单", value: 16, detail: "当前范围" },
          { key: "draft", label: "草稿", value: 2, detail: "可继续编辑" },
          { key: "review", label: "待审核", value: 4, detail: "财务闸门" },
          { key: "active", label: "已生效", value: 10, detail: "可履约" },
        ],
        columns: [
          { key: "number", header: "采购单号" },
          { key: "supplier", header: "供应商" },
          { key: "source", header: "来源销售单" },
          { key: "amount", header: "采购含税金额", numeric: true },
          { key: "owner", header: "采购负责人" },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "po_1",
            cells: {
              number: "CG20260328001",
              supplier: "鲜果直供供应链",
              source: "XS20260328001",
              amount: money(98000),
              owner: "赵强",
              status: "待审核",
            },
            status: { label: "待审核", tone: "warning" },
          },
          {
            id: "po_2",
            cells: {
              number: "CG20260327012",
              supplier: "礼遇包装工坊",
              source: "XS20260327018",
              amount: money(215600),
              owner: "赵强",
              status: "已生效",
            },
            status: { label: "已生效", tone: "success" },
          },
          {
            id: "po_3",
            cells: {
              number: "CG20260326005",
              supplier: "云仓配送服务",
              source: "XS20260325008",
              amount: money(18600),
              owner: "陈璐",
              status: "草稿",
            },
            status: { label: "草稿", tone: "neutral" },
          },
        ]),
      },
    },
  },

  W09: {
    id: "W09",
    title: "履约作业",
    description:
      "按任务连续处理出库、到货、服务交付等履约作业，记录履约记录。",
    mode: "M3+M5",
    breadcrumbs: [
      { id: "proc", label: "采购与履约", href: "/procurement/confirm" },
      { id: "fulfillment", label: "履约作业" },
    ],
    shell: {
      kind: "queue",
      payload: {
        scopeLabels: [sequentialText.minePending, "待领取", "已暂挂"],
        tasks: [
          {
            id: "ff_01",
            taskType: "出库确认",
            businessObject: "销售单 · XS20260327018",
            counterparty: "青禾科技有限公司",
            enteredAt: "今天 08:10",
            enteredDateTime: "2026-08-01T08:10:00+08:00",
            dueAt: "今天 15:00",
            dueDateTime: "2026-08-01T15:00:00+08:00",
            responsibleParty: "仓储 · 周航",
            reason: "采购到货已入库，客户要求今日发货",
            impact: "延迟将影响客户验收窗口",
            status: { label: "待处理", tone: "warning" },
            scopeTags: [sequentialText.minePending, "已暂挂"],
            summaryFields: [
              { label: "仓库", value: "华东一号仓" },
              { label: "SKU 行数", value: "6" },
              { label: "计划出库量", value: "420 套", numeric: true },
              { label: "履约方式", value: "公司仓发" },
            ],
            checkItems: ["拣货完成", "物流单号已生成", "附件齐全"],
          },
          {
            id: "ff_02",
            taskType: "服务交付登记",
            businessObject: "销售单 · XS20260326004",
            counterparty: "蓝湾集团",
            enteredAt: "昨天 17:40",
            enteredDateTime: "2026-07-31T17:40:00+08:00",
            dueAt: "今天 12:00",
            dueDateTime: "2026-08-01T12:00:00+08:00",
            responsibleParty: "履约 · 待领取",
            reason: "现场服务完成，等待交付记录登记",
            impact: "登记后才可进入客户验收",
            status: { label: "待领取", tone: "info" },
            scopeTags: ["待领取"],
            summaryFields: [
              { label: "服务项目", value: "年节礼包现场布置" },
              { label: "计划完成日", value: "2026-08-01", numeric: true },
            ],
          },
        ],
      },
    },
  },

  W10: {
    id: "W10",
    title: "库存台账",
    description:
      "按仓库与 SKU 查看账面现存、可用量与近期流水；分析零可用与调整风险。",
    mode: "M2+M6",
    breadcrumbs: [
      { id: "proc", label: "采购与履约", href: "/inventory" },
      { id: "inv", label: "库存台账" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "SKU、仓库、商品名称",
        filterLabels: ["全部", "零可用", "低于预警", "有在途"],
        metrics: [
          { key: "combos", label: "仓库+SKU 组合", value: 1286, detail: "当前筛选" },
          { key: "zero", label: "零可用", value: 42, detail: "需关注补货" },
          { key: "alert", label: "低于预警", value: 19, detail: "策略触发" },
          { key: "adj", label: "待确认调整", value: 3, detail: "调整单" },
        ],
        columns: [
          { key: "sku", header: "SKU / 商品" },
          { key: "warehouse", header: "仓库" },
          { key: "onHand", header: "账面现存", numeric: true },
          { key: "available", header: "可用量", numeric: true },
          { key: "inTransit", header: "在途", numeric: true },
          { key: "status", header: freshnessText.syncProgress, status: true },
        ],
        rows: listRows([
          {
            id: "inv_1",
            cells: {
              sku: "SKU-NY-BOX-01 · 新春坚果礼盒",
              warehouse: "华东一号仓",
              onHand: "860",
              available: "640",
              inTransit: "200",
              status: "正常",
            },
            status: { label: "正常", tone: "success" },
            filterTags: ["有在途"],
            metricTags: ["combos"],
          },
          {
            id: "inv_2",
            cells: {
              sku: "SKU-CARD-02 · 定制贺卡套装",
              warehouse: "华东一号仓",
              onHand: "12",
              available: "0",
              inTransit: "0",
              status: "零可用",
            },
            status: { label: "零可用", tone: "destructive" },
            metricTags: ["zero", "combos"],
            filterTags: ["零可用"],
          },
          {
            id: "inv_3",
            cells: {
              sku: "SKU-TEA-09 · 礼盒红茶",
              warehouse: "华南中转仓",
              onHand: "48",
              available: "18",
              inTransit: "60",
              status: "低于预警",
            },
            status: { label: "低于预警", tone: "warning" },
            metricTags: ["alert", "combos"],
            filterTags: ["低于预警", "有在途"],
          },
        ]),
      },
    },
  },

  W11: {
    id: "W11",
    title: "客户往来",
    description:
      "查看客户应收余额、回款与开票进度，进入分摊与核销工作区处理票款。",
    mode: "M2+M5",
    breadcrumbs: [
      { id: "fin", label: "财务", href: "/finance/customer-accounts" },
      { id: "ar", label: "客户往来" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "客户、销售单、回款单号",
        primaryActionLabel: "登记回款",
        filterLabels: ["全部", "有余额", "有逾期", "待核销"],
        metrics: [
          { key: "ar", label: "应收余额", value: money(1_286_400), detail: "当前范围" },
          { key: "overdue", label: "逾期金额", value: money(96_200), detail: "需催收" },
          { key: "unapplied", label: "待核销回款", value: money(42_000), detail: "已到账" },
          { key: "open", label: "未结清客户", value: 36, detail: "户数" },
        ],
        columns: [
          { key: "customer", header: "客户" },
          { key: "balance", header: "应收余额", numeric: true },
          { key: "overdue", header: "逾期金额", numeric: true },
          { key: "received", header: "已回款", numeric: true },
          { key: "invoiced", header: "已开票", numeric: true },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "ar_1",
            cells: {
              customer: "星河制造股份有限公司",
              balance: money(186000),
              overdue: money(0),
              received: money(0),
              invoiced: money(0),
              // 卡券差额复核未完成：不以 0/投影冒充已核实
              status: "票款指标不可靠",
            },
            status: { label: "票款指标不可靠", tone: "warning" },
          },
          {
            id: "ar_2",
            cells: {
              customer: "北辰能源集团",
              balance: money(96000),
              overdue: money(36000),
              received: money(40000),
              invoiced: money(50000),
              status: "有逾期 · 卡券未复核",
            },
            status: { label: "票款指标不可靠", tone: "warning" },
          },
          {
            id: "ar_3",
            cells: {
              customer: "蓝湾集团（卡券期初）",
              balance: money(128000),
              overdue: money(0),
              received: money(0),
              invoiced: money(0),
              status: "未复核 · 0≠已核实",
            },
            status: { label: "票款指标不可靠", tone: "warning" },
          },
        ]),
      },
    },
  },

  W12: {
    id: "W12",
    title: "供应商往来",
    description:
      "查看供应商应付余额、付款与进项票进度，进入分摊与核销工作区。",
    mode: "M2+M5",
    breadcrumbs: [
      { id: "fin", label: "财务", href: "/finance/supplier-accounts" },
      { id: "ap", label: "供应商往来" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "供应商、采购单、付款单号",
        primaryActionLabel: "登记付款",
        filterLabels: ["全部", "有余额", "待付款", "待核销"],
        metrics: [
          { key: "ap", label: "应付余额", value: money(642_800), detail: "当前范围" },
          { key: "due", label: "本期应付", value: money(188_000), detail: "7 日内" },
          { key: "unapplied", label: "待核销付款", value: money(25_000), detail: "已付" },
          { key: "open", label: "未结清供应商", value: 22, detail: "户数" },
        ],
        columns: [
          { key: "supplier", header: "供应商" },
          { key: "balance", header: "应付余额", numeric: true },
          { key: "paid", header: "已付款", numeric: true },
          { key: "invoiced", header: "已收票", numeric: true },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "ap_1",
            cells: {
              supplier: "鲜果直供供应链",
              balance: money(98000),
              paid: money(0),
              invoiced: money(0),
              status: "待付款",
            },
            status: { label: "待付款", tone: "warning" },
          },
          {
            id: "ap_2",
            cells: {
              supplier: "礼遇包装工坊",
              balance: money(120600),
              paid: money(95000),
              invoiced: money(110000),
              status: "正常",
            },
            status: { label: "正常", tone: "info" },
          },
        ]),
      },
    },
  },

  W13: {
    id: "W13",
    title: "卡券票款复核",
    description:
      "连续复核卡券销售单的回款、开票与商城票款记录，确认后方可作为经营结果。",
    mode: "M3",
    breadcrumbs: [
      { id: "fin", label: "财务", href: "/finance/card-funds-review" },
      { id: "card", label: "卡券票款复核" },
    ],
    shell: {
      kind: "queue",
      payload: {
        scopeLabels: ["待复核", "有差异", "已通过"],
        tasks: [
          {
            id: "cfr_01",
            taskType: "卡券回款复核",
            businessObject: "销售单 · XS20260325008",
            counterparty: "蓝湾集团",
            enteredAt: "今天 07:50",
            enteredDateTime: "2026-08-01T07:50:00+08:00",
            dueAt: "今天 16:00",
            dueDateTime: "2026-08-01T16:00:00+08:00",
            responsibleParty: "财务 · 王敏",
            reason: "商城支付成功记录与 ERP 应收需人工对齐",
            impact: "未复核前不得计入已确认经营收入",
            status: { label: "待复核", tone: "warning" },
            summaryFields: [
              { label: "成交金额（含税）", value: money(128000) },
              { label: "商城实付", value: money(128000) },
              { label: "已核销回款", value: money(86000) },
              { label: "待复核差额", value: money(42000) },
            ],
            checkItems: [
              "支付成功记录完整",
              "客户归属正确",
              "回款分配覆盖明细",
            ],
          },
        ],
      },
    },
  },

  W14: {
    id: "W14",
    title: "主数据",
    description:
      "维护可销售项目、商品、卡券类目、供应商与仓库；版本变更可追溯。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "md", label: "主数据", href: "/master-data/sellable-items" },
      { id: "resource", label: "可销售项目" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "编号、名称、负责人",
        primaryActionLabel: "新建 / 形成新版本",
        filterLabels: [
          "可销售项目",
          "商品与 SKU",
          "卡券类目",
          "供应商",
          "仓库",
        ],
        metrics: [
          { key: "enabled", label: "当前启用", value: 426, detail: "可被选择" },
          { key: "disabled", label: "当前停用", value: 38, detail: "历史保留" },
          { key: "expiring", label: "有效期将尽", value: 7, detail: "30 日内" },
          { key: "pending", label: "待生效修订", value: 4, detail: "计划生效" },
        ],
        columns: [
          { key: "code", header: "稳定编号" },
          { key: "name", header: "名称" },
          { key: "version", header: "当前版本" },
          { key: "period", header: "生效区间" },
          { key: "owner", header: "负责人" },
          { key: "status", header: "启停状态", status: true },
        ],
        rows: listRows([
          {
            id: "si_1",
            cells: {
              code: "SI-2026-0188",
              name: "新春坚果礼盒 · 典藏款",
              version: "v6",
              period: "2026-01-01 ~ 2026-12-31",
              owner: "赵强",
              status: "当前启用",
            },
            status: { label: "当前启用", tone: "success" },
          },
          {
            id: "si_2",
            cells: {
              code: "SI-2026-0201",
              name: "员工生日蛋糕卡",
              version: "v2",
              period: "2026-03-01 ~ 2026-09-30",
              owner: "李倩",
              status: "当前启用",
            },
            status: { label: "当前启用", tone: "success" },
          },
          {
            id: "si_3",
            cells: {
              code: "SI-2025-0902",
              name: "中秋月饼礼盒（旧版）",
              version: "v9",
              period: "2025-08-01 ~ 2025-10-31",
              owner: "赵强",
              status: "当前停用",
            },
            status: { label: "当前停用", tone: "neutral" },
          },
        ]),
      },
    },
  },

  W15: {
    id: "W15",
    title: "客户经营质量",
    description:
      "只读分析客户规模、利润贡献与回款风险；展示数据更新时间与成本覆盖。",
    mode: "M6",
    breadcrumbs: [
      { id: "an", label: "分析", href: "/analytics/customer-quality" },
      { id: "cq", label: "客户经营质量" },
    ],
    shell: {
      kind: "analytics",
      payload: {
        metrics: [
          { key: "customers", label: "活跃客户", value: 128, detail: "本期有成交" },
          { key: "gmv", label: "成交规模（含税）", value: money(18_640_000) },
          { key: "profit", label: "利润贡献（不含税）", value: money(2_186_400) },
          { key: "risk", label: "高回款风险", value: 9, detail: "需跟进" },
          { key: "coverage", label: "成本覆盖率", value: "92%", detail: "汇总口径" },
        ],
        seriesTitle: "近 6 个月成交规模",
        series: [
          { label: "3月", value: 2.1 },
          { label: "4月", value: 2.4 },
          { label: "5月", value: 2.8 },
          { label: "6月", value: 3.1 },
          { label: "7月", value: 3.6 },
          { label: "8月", value: 4.0 },
        ],
        tableTitle: "客户贡献排行",
        columns: [
          { key: "customer", header: "客户" },
          { key: "orders", header: "销售单数", numeric: true },
          { key: "gmv", header: "成交规模", numeric: true },
          { key: "profit", header: "利润贡献", numeric: true },
          { key: "risk", header: "回款风险", status: true },
        ],
        rows: listRows([
          {
            id: "q1",
            cells: {
              customer: "星河制造股份有限公司",
              orders: "12",
              gmv: money(2_860_000),
              profit: money(386_000),
              risk: "低",
            },
            status: { label: "低", tone: "success" },
          },
          {
            id: "q2",
            cells: {
              customer: "北辰能源集团",
              orders: "6",
              gmv: money(960_000),
              profit: money(42_000),
              risk: "高",
            },
            status: { label: "高", tone: "destructive" },
          },
        ]),
        notes: [
          "经营质量为异步汇总，允许最多约 1 分钟延迟。",
          "利润与成本字段受权限控制；无权时仅显示覆盖等级。",
        ],
      },
    },
  },

  W16: {
    id: "W16",
    title: "实际经营盈亏",
    description:
      "按期间查看实际收入、成本与利润；成本完整性与复核状态作为只读记录展示。",
    mode: "M6",
    breadcrumbs: [
      { id: "an", label: "分析", href: "/analytics/profit-loss" },
      { id: "pl", label: "实际经营盈亏" },
    ],
    shell: {
      kind: "analytics",
      payload: {
        metrics: [
          { key: "revenue", label: "实际收入（不含税）", value: money(12_480_000) },
          { key: "cost", label: "已复核成本", value: money(9_860_000) },
          { key: "profit", label: "经营利润", value: money(2_620_000) },
          { key: "margin", label: "利润率", value: "21.0%" },
          { key: "integrity", label: "成本完整性", value: "94%", detail: "汇总" },
        ],
        seriesTitle: "月度利润趋势（万元）",
        series: [
          { label: "3月", value: 180 },
          { label: "4月", value: 210 },
          { label: "5月", value: 240 },
          { label: "6月", value: 255 },
          { label: "7月", value: 290 },
          { label: "8月", value: 310 },
        ],
        tableTitle: "期间明细",
        columns: [
          { key: "period", header: "期间" },
          { key: "revenue", header: "收入", numeric: true },
          { key: "cost", header: "成本", numeric: true },
          { key: "profit", header: "利润", numeric: true },
          { key: "status", header: "成本状态", status: true },
        ],
        rows: listRows([
          {
            id: "pl1",
            cells: {
              period: "2026-07",
              revenue: money(2_180_000),
              cost: money(1_720_000),
              profit: money(460_000),
              status: "已复核",
            },
            status: { label: "已复核", tone: "success" },
          },
          {
            id: "pl2",
            cells: {
              period: "2026-08",
              revenue: money(1_420_000),
              cost: money(1_050_000),
              profit: money(370_000),
              status: "部分待复核",
            },
            status: { label: "部分待复核", tone: "warning" },
          },
        ]),
      },
    },
  },

  W17: {
    id: "W17",
    title: "商城同步与映射",
    description:
      "治理商城与 ERP 的主责边界、映射任务与同步进度；处理待映射差异。",
    mode: "M7",
    breadcrumbs: [
      { id: "gov", label: "治理", href: "/governance/mall-sync" },
      { id: "sync", label: "商城同步与映射" },
    ],
    shell: {
      kind: "governance",
      payload: {
        stages: [
          { key: "detect", label: "发现差异", status: "complete" },
          { key: "map", label: "映射处理", status: "current" },
          { key: "confirm", label: "确认生效", status: "pending" },
          { key: "seal", label: "封存进度", status: "pending" },
        ],
        metrics: [
          { key: "pending", label: "待映射", value: 17, detail: "阻断入账" },
          { key: "erp", label: "主责 ERP", value: 842, detail: "销售单" },
          { key: "mall", label: "主责商城", value: 1_206, detail: "销售单" },
          { key: "last", label: "最近成功同步", value: "今天 09:12" },
        ],
        batchColumns: [
          { key: "batch", header: "批次" },
          { key: "type", header: "类型" },
          { key: "count", header: "差异数", numeric: true },
          { key: "status", header: "状态", status: true },
        ],
        batches: listRows([
          {
            id: "b1",
            cells: {
              batch: "SYNC-20260801-017",
              type: "商品映射",
              count: "12",
              status: "处理中",
            },
            status: { label: "处理中", tone: "info" },
          },
          {
            id: "b2",
            cells: {
              batch: "SYNC-20260731-009",
              type: "客户映射",
              count: "5",
              status: "待确认",
            },
            status: { label: "待确认", tone: "warning" },
          },
        ]),
        issues: [
          {
            id: "i1",
            severity: "error",
            message: "外部商品缺少可销售项目映射",
            objectLabel: "EXT-SKU-9912",
            field: "sellableItemId",
          },
          {
            id: "i2",
            severity: "warning",
            message: "客户信用代码为空，需人工确认主体",
            objectLabel: "MALL-C-8821",
          },
        ],
        diffEntries: [
          {
            id: "d1",
            field: "主责系统",
            before: "商城",
            after: "ERP（迁移中）",
          },
          {
            id: "d2",
            field: "映射商品",
            before: "未映射",
            after: "SI-2026-0188",
          },
        ],
      },
    },
  },

  W18: {
    id: "W18",
    title: "导入与期初",
    description:
      "管理导入批次、期初数据与校验错误；按阶段推进上传、校验、确认与过账。",
    mode: "M7",
    breadcrumbs: [
      { id: "gov", label: "治理", href: "/governance/imports" },
      { id: "imp", label: "导入与期初" },
    ],
    shell: {
      kind: "governance",
      payload: {
        stages: [
          { key: "upload", label: "上传", status: "complete" },
          { key: "validate", label: "校验", status: "current" },
          { key: "diff", label: "差异确认", status: "pending" },
          { key: "post", label: "过账", status: "pending" },
        ],
        metrics: [
          { key: "batches", label: "进行中批次", value: 3 },
          { key: "errors", label: "校验错误", value: 14 },
          { key: "warnings", label: "警告", value: 6 },
          { key: "ready", label: "可确认行", value: 1_280 },
        ],
        batchColumns: [
          { key: "batch", header: "批次编号" },
          { key: "type", header: "对象集合" },
          { key: "file", header: "文件" },
          { key: "status", header: "状态", status: true },
        ],
        batches: listRows([
          {
            id: "ib1",
            cells: {
              batch: "IMP-20260801-003",
              type: "客户期初应收",
              file: "ar-opening-v3.xlsx",
              status: "校验中",
            },
            status: { label: "校验中", tone: "info" },
          },
          {
            id: "ib2",
            cells: {
              batch: "IMP-20260728-011",
              type: "库存期初",
              file: "inv-open.csv",
              status: "待确认",
            },
            status: { label: "待确认", tone: "warning" },
          },
        ]),
        issues: [
          {
            id: "ie1",
            severity: "error",
            message: "客户编号不存在",
            objectLabel: "行 28",
            field: "customerNo",
          },
          {
            id: "ie2",
            severity: "error",
            message: "金额精度超过分",
            objectLabel: "行 41",
            field: "amount",
          },
        ],
      },
    },
  },

  W19: {
    id: "W19",
    title: "权限与审计",
    description:
      "查询角色、用户授权、数据范围策略与操作审计；配置与审计双视图。",
    mode: "M2",
    breadcrumbs: [
      { id: "sys", label: "系统", href: "/system/access-audit" },
      { id: "access", label: "权限与审计" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "角色、用户、策略、审计对象",
        filterLabels: ["角色", "用户", "数据范围", "审计"],
        metrics: [
          { key: "roles", label: "启用角色", value: 28 },
          { key: "users", label: "启用账号", value: 146 },
          { key: "policies", label: "数据范围策略", value: 12 },
          { key: "events", label: "今日审计事件", value: 392 },
        ],
        columns: [
          { key: "name", header: "名称 / 账号" },
          { key: "code", header: "代码" },
          { key: "scope", header: "数据范围" },
          { key: "source", header: "权限来源" },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "r1",
            cells: {
              name: "销售",
              code: "role.sales",
              scope: "本人负责 + 协作",
              source: "角色授权",
              status: "启用",
            },
            status: { label: "启用", tone: "success" },
          },
          {
            id: "r2",
            cells: {
              name: "王敏",
              code: "user.wangmin",
              scope: "销售 · 华东团队",
              source: "角色 + 临时授权",
              status: "启用",
            },
            status: { label: "启用", tone: "success" },
          },
          {
            id: "r3",
            cells: {
              name: "财务审核",
              code: "role.finance_review",
              scope: "公司级",
              source: "角色授权",
              status: "启用",
            },
            status: { label: "启用", tone: "success" },
          },
        ]),
      },
    },
  },

  W20: {
    id: "W20",
    title: "API 供应商连接",
    description:
      "管理供应商 API 连接、环境、健康检查与责任人；打开对象中心查看配置。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "api", label: "供应商 API", href: "/supplier-api/connections" },
      { id: "conn", label: "API 连接" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "连接编号、供应商、环境",
        primaryActionLabel: "新建连接",
        filterLabels: ["全部", "生产", "预发", "异常"],
        metrics: [
          { key: "all", label: "连接数", value: 9 },
          { key: "healthy", label: "健康", value: 7 },
          { key: "degraded", label: "降级", value: 1 },
          { key: "down", label: "不可用", value: 1 },
        ],
        columns: [
          { key: "code", header: "连接编号" },
          { key: "supplier", header: "供应商" },
          { key: "env", header: "环境" },
          { key: "health", header: "最近健康检查", numeric: true },
          { key: "owners", header: "业务 / 技术责任" },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "c1",
            cells: {
              code: "CONN-JD-PROD",
              supplier: "京东企业购",
              env: "生产",
              health: "今天 09:00 · 正常",
              owners: "赵强 / 运维组",
              status: "健康",
            },
            status: { label: "健康", tone: "success" },
          },
          {
            id: "c2",
            cells: {
              code: "CONN-SF-STG",
              supplier: "顺丰同城",
              env: "预发",
              health: "今天 08:40 · 延迟",
              owners: "陈璐 / 平台组",
              status: "降级",
            },
            status: { label: "降级", tone: "warning" },
          },
        ]),
      },
    },
  },

  W21: {
    id: "W21",
    title: "外部商品映射与供给",
    description:
      "连续处理外部商品观察、映射与固定供给；进入对象中心维护供给条件。",
    mode: "M3+M4",
    breadcrumbs: [
      { id: "api", label: "供应商 API", href: "/supplier-api/catalog" },
      { id: "cat", label: "外部商品供给" },
    ],
    shell: {
      kind: "queue",
      payload: {
        scopeLabels: ["待映射", "供给变更", "成本异常"],
        tasks: [
          {
            id: "ep_01",
            taskType: "外部商品映射",
            businessObject: "EXT-SKU-9912",
            counterparty: "京东企业购",
            enteredAt: "今天 09:05",
            enteredDateTime: "2026-08-01T09:05:00+08:00",
            dueAt: "今天 17:00",
            dueDateTime: "2026-08-01T17:00:00+08:00",
            responsibleParty: "运营 · 李倩",
            reason: "新上架外部商品尚未映射到可销售项目",
            impact: "阻断相关消费订单与发布",
            status: { label: "待映射", tone: "warning" },
            summaryFields: [
              { label: "外部名称", value: "坚果礼盒 A 款" },
              { label: "来源版本", value: "v18" },
              { label: "参考成本", value: money(420) },
              { label: "运费策略", value: "按区" },
            ],
          },
        ],
      },
    },
  },

  W22: {
    id: "W22",
    title: "商品发布",
    description:
      "管理向目标商城的商品发布修订、生效状态与接收结果。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "com", label: "商城与发布", href: "/commerce/publications" },
      { id: "pub", label: "商品发布" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "发布编号、SKU、目标商城",
        primaryActionLabel: "新建发布",
        filterLabels: ["全部", "待确认", "已生效", "接收失败"],
        metrics: [
          { key: "all", label: "发布单", value: 64 },
          { key: "pending", label: "待确认", value: 5 },
          { key: "live", label: "已生效", value: 52 },
          { key: "fail", label: "接收失败", value: 2 },
        ],
        columns: [
          { key: "code", header: "发布编号" },
          { key: "sku", header: "SKU" },
          { key: "mall", header: "目标商城" },
          { key: "version", header: "发布修订" },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "pub1",
            cells: {
              code: "PUB-20260801-004",
              sku: "SKU-NY-BOX-01",
              mall: "华东商城",
              version: "r12",
              status: "待确认",
            },
            status: { label: "待确认", tone: "warning" },
          },
          {
            id: "pub2",
            cells: {
              code: "PUB-20260730-018",
              sku: "SKU-TEA-09",
              mall: "华北商城",
              version: "r7",
              status: "已生效",
            },
            status: { label: "已生效", tone: "success" },
          },
        ]),
      },
    },
  },

  W23: {
    id: "W23",
    title: "执行信息",
    description:
      "查询销售单在商城侧的执行信息版本、接收状态与迁移基线。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "com", label: "商城与发布", href: "/commerce/execution-projections" },
      { id: "ep", label: "执行信息" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "执行编号、销售单、商城",
        filterLabels: ["全部", "接收中", "已接收", "失败"],
        metrics: [
          { key: "all", label: "执行记录", value: 1_024 },
          { key: "recv", label: "已接收", value: 980 },
          { key: "inflight", label: "接收中", value: 28 },
          { key: "fail", label: "失败", value: 16 },
        ],
        columns: [
          { key: "code", header: "执行编号" },
          { key: "sales", header: "销售单" },
          { key: "mall", header: "商城" },
          { key: "version", header: "执行信息版本" },
          { key: "status", header: "商城接收状态", status: true },
        ],
        rows: listRows([
          {
            id: "xp1",
            cells: {
              code: "XP-20260801-110",
              sales: "XS20260327018",
              mall: "华东商城",
              version: "ep-v3",
              status: "已接收",
            },
            status: { label: "已接收", tone: "success" },
          },
          {
            id: "xp2",
            cells: {
              code: "XP-20260801-118",
              sales: "XS20260328001",
              mall: "华东商城",
              version: "ep-v1",
              status: "接收中",
            },
            status: { label: "接收中", tone: "info" },
          },
        ]),
      },
    },
  },

  W24: {
    id: "W24",
    title: "主责迁移批次",
    description:
      "治理销售单主责在商城与 ERP 之间的迁移批次、预检与切换确认。",
    mode: "M7",
    breadcrumbs: [
      { id: "gov", label: "治理", href: "/governance/ownership-migrations" },
      { id: "om", label: "主责迁移" },
    ],
    shell: {
      kind: "governance",
      payload: {
        stages: [
          { key: "prep", label: "预检", status: "complete" },
          { key: "freeze", label: "维护冻结", status: "current" },
          { key: "cutover", label: "切换确认", status: "pending" },
          { key: "seal", label: "封存", status: "pending" },
        ],
        metrics: [
          { key: "batches", label: "进行中批次", value: 2 },
          { key: "orders", label: "覆盖销售单", value: 186 },
          { key: "blockers", label: "预检阻塞", value: 4 },
          { key: "ready", label: "可切换", value: 172 },
        ],
        batchColumns: [
          { key: "batch", header: "批次编号" },
          { key: "customer", header: "客户范围" },
          { key: "fingerprint", header: "版本摘要" },
          { key: "status", header: "状态", status: true },
        ],
        batches: listRows([
          {
            id: "mb1",
            cells: {
              batch: "OM-20260801-002",
              customer: "星河制造及 3 家关联",
              fingerprint: "a91c…e2",
              status: "维护冻结",
            },
            status: { label: "维护冻结", tone: "warning" },
          },
        ]),
        issues: [
          {
            id: "mi1",
            severity: "error",
            message: "存在未关闭履约任务，禁止切换",
            objectLabel: "XS20260327012",
          },
        ],
        diffEntries: [
          {
            id: "md1",
            field: "主责系统",
            before: "商城",
            after: "ERP",
          },
        ],
      },
    },
  },

  W25: {
    id: "W25",
    title: "商城消费订单",
    description:
      "记录追溯：关键记录、支付分摊、来源追溯与供应商履约。专用页 features/mall-consumption-orders。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "com", label: "商城与发布", href: "/commerce/consumption-orders" },
      { id: "co", label: "商城消费订单" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "商城单号、ERP 编号、客户",
        filterLabels: ["全部", "支付成功", "待归集", "差异", "履约异常"],
        metrics: [
          { key: "paid", label: "支付成功", value: 8 },
          { key: "pending_attr", label: "待归集", value: 1 },
          { key: "fact_diff", label: "记录差异", value: 2 },
          { key: "auto_exception", label: "自动履约异常", value: 2 },
          { key: "cost_none", label: "成本未覆盖", value: 3 },
        ],
        columns: [
          { key: "mall", header: "商城订单" },
          { key: "customer", header: "客户" },
          { key: "paidAt", header: "支付时间" },
          { key: "paid", header: "实付", numeric: true },
          { key: "chain", header: "履约链" },
          { key: "attr", header: "归集", status: true },
        ],
        rows: listRows([
          {
            id: "mo-90881",
            cells: {
              mall: "M2026080190881",
              customer: "星河制造股份有限公司",
              paidAt: "08-01 08:12",
              paid: money(680),
              chain: "ERP 自动",
              attr: "已归集",
            },
            status: { label: "已归集", tone: "success" },
          },
          {
            id: "mo-77120",
            cells: {
              mall: "M2026073177120",
              customer: "青禾科技有限公司",
              paidAt: "07-31 18:45",
              paid: money(198),
              chain: "ERP 自动",
              attr: "已归集",
            },
            status: { label: "履约异常", tone: "warning" },
          },
        ]),
      },
    },
  },

  W26: {
    id: "W26",
    title: "供应商订单",
    description:
      "查看供应商接单、履约轨与售后状态；处理失败与人工介入项。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "api", label: "供应商 API", href: "/supplier-api/orders" },
      { id: "so", label: "供应商订单" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "供应商订单号、商城订单、供应商",
        filterLabels: ["全部", "待接单", "履约中", "失败"],
        metrics: [
          { key: "all", label: "供应商订单", value: 2_104 },
          { key: "accept", label: "待接单", value: 18 },
          { key: "ff", label: "履约中", value: 260 },
          { key: "fail", label: "失败 / 待人工", value: 7 },
        ],
        columns: [
          { key: "number", header: "供应商订单" },
          { key: "supplier", header: "供应商" },
          { key: "mall", header: "关联商城订单" },
          { key: "fulfillment", header: "履约轨" },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "sord1",
            cells: {
              number: "SO-JD-90881",
              supplier: "京东企业购",
              mall: "M2026080190881",
              fulfillment: "部分",
              status: "履约中",
            },
            status: { label: "履约中", tone: "info" },
          },
          {
            id: "sord2",
            cells: {
              number: "SO-SF-77120",
              supplier: "顺丰同城",
              mall: "M2026073190012",
              fulfillment: "失败",
              status: "待人工",
            },
            status: { label: "待人工", tone: "destructive" },
          },
        ]),
      },
    },
  },

  W27: {
    id: "W27",
    title: "API 结算",
    description:
      "管理供应商结算单期间、明细汇总、差异与确认状态。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "api", label: "供应商 API", href: "/supplier-api/settlements" },
      { id: "st", label: "API 结算" },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: "结算单号、供应商、期间",
        primaryActionLabel: "新建结算单",
        filterLabels: ["全部", "待对账", "有差异", "已确认"],
        metrics: [
          { key: "all", label: "结算单", value: 48 },
          { key: "diff", label: "有差异", value: 5 },
          { key: "review", label: "待复核", value: 3 },
          { key: "done", label: "已确认", value: 36 },
        ],
        columns: [
          { key: "number", header: "结算单号" },
          { key: "supplier", header: "供应商" },
          { key: "period", header: "结算期间" },
          { key: "amount", header: "结算金额", numeric: true },
          { key: "status", header: "状态", status: true },
        ],
        rows: listRows([
          {
            id: "st1",
            cells: {
              number: "ST-2026-07-JD",
              supplier: "京东企业购",
              period: "2026-07",
              amount: money(486_200),
              status: "有差异",
            },
            status: { label: "有差异", tone: "warning" },
          },
          {
            id: "st2",
            cells: {
              number: "ST-2026-06-SF",
              supplier: "顺丰同城",
              period: "2026-06",
              amount: money(62_800),
              status: "已确认",
            },
            status: { label: "已确认", tone: "success" },
          },
        ]),
      },
    },
  },

  W28: {
    id: "W28",
    title: "卡券消费台账与经营分析",
    description:
      "分析卡券销售额度、消费进度、退款与成本覆盖；只读经营汇总。",
    mode: "M6",
    breadcrumbs: [
      { id: "an", label: "分析", href: "/analytics/card-business" },
      { id: "card", label: "卡券经营分析" },
    ],
    shell: {
      kind: "analytics",
      payload: {
        metrics: [
          { key: "quota", label: "可消费总额度", value: money(8_600_000) },
          { key: "spent", label: "累计卡券消费", value: money(5_240_000) },
          { key: "rate", label: "消费进度", value: "60.9%" },
          { key: "refund", label: "卡券退款", value: money(86_000) },
          { key: "cost", label: "成本覆盖率", value: "88%" },
        ],
        seriesTitle: "周消费金额（万元）",
        series: [
          { label: "W23", value: 42 },
          { label: "W24", value: 48 },
          { label: "W25", value: 51 },
          { label: "W26", value: 55 },
          { label: "W27", value: 60 },
          { label: "W28", value: 58 },
        ],
        tableTitle: "卡券销售明细摘要",
        columns: [
          { key: "sales", header: "销售单" },
          { key: "customer", header: "客户" },
          { key: "quota", header: "额度", numeric: true },
          { key: "spent", header: "已消费", numeric: true },
          { key: "status", header: "进度", status: true },
        ],
        rows: listRows([
          {
            id: "cb1",
            cells: {
              sales: "XS20260325008",
              customer: "蓝湾集团",
              quota: money(500_000),
              spent: money(286_000),
              status: "进行中",
            },
            status: { label: "进行中", tone: "info" },
          },
        ]),
      },
    },
  },

  W29: {
    id: "W29",
    title: "接口错误与对账中心",
    description:
      "处理接口错误任务与对账差异；按责任领取、分类处置与关闭。",
    mode: "M7",
    breadcrumbs: [
      { id: "gov", label: "治理", href: "/governance/integration-errors" },
      { id: "ie", label: "接口错误与对账" },
    ],
    shell: {
      kind: "governance",
      payload: {
        stages: [
          { key: "triage", label: "分诊", status: "complete" },
          { key: "claim", label: "领取处理", status: "current" },
          { key: "reconcile", label: "对账确认", status: "pending" },
          { key: "close", label: "关闭", status: "pending" },
        ],
        metrics: [
          { key: "errors", label: "开放错误", value: 23 },
          { key: "diff", label: "对账差异", value: 8 },
          { key: "mine", label: sequentialText.minePending, value: 6 },
          { key: "critical", label: "阻断级", value: 2 },
        ],
        batchColumns: [
          { key: "task", header: "任务编号" },
          { key: "class", header: "错误分类" },
          { key: "object", header: "影响对象" },
          { key: "status", header: "状态", status: true },
        ],
        batches: listRows([
          {
            id: "err1",
            cells: {
              task: "IE-20260801-044",
              class: "接单超时",
              object: "SO-SF-77120",
              status: "处理中",
            },
            status: { label: "处理中", tone: "info" },
          },
          {
            id: "err2",
            cells: {
              task: "DF-20260801-009",
              class: "金额差异",
              object: "ST-2026-07-JD",
              status: "待对账",
            },
            status: { label: "待对账", tone: "warning" },
          },
        ]),
        issues: [
          {
            id: "ei1",
            severity: "error",
            message: "供应商回调签名校验失败，已转技术责任",
            objectLabel: "CONN-JD-PROD",
          },
          {
            id: "ei2",
            severity: "warning",
            message: "结算明细缺少 3 行供应商账单金额",
            objectLabel: "ST-2026-07-JD",
          },
        ],
      },
    },
  },

  W30: {
    id: "W30",
    title: "历史消费回填",
    description:
      "管理历史消费回填任务范围、来源校验、策略确认与执行状态。",
    mode: "M7",
    breadcrumbs: [
      { id: "gov", label: "治理", href: "/governance/history-backfill" },
      { id: "hb", label: "历史消费回填" },
    ],
    shell: {
      kind: "governance",
      payload: {
        stages: [
          { key: "scope", label: "范围确认", status: "complete" },
          { key: "validate", label: "来源校验", status: "complete" },
          { key: "policy", label: "策略确认", status: "current" },
          { key: "run", label: "执行回填", status: "pending" },
        ],
        metrics: [
          { key: "jobs", label: "任务数", value: 4 },
          { key: "ready", label: "待确认策略", value: 1 },
          { key: "running", label: "执行中", value: 1 },
          { key: "rows", label: "预计回填行", value: 128_400 },
        ],
        batchColumns: [
          { key: "job", header: "任务编号" },
          { key: "range", header: "回填范围" },
          { key: "source", header: "来源校验" },
          { key: "status", header: "状态", status: true },
        ],
        batches: listRows([
          {
            id: "hb1",
            cells: {
              job: "HB-202607-01",
              range: "2025-01 ~ 2025-12 · 华东商城",
              source: "通过",
              status: "待确认",
            },
            status: { label: "待确认", tone: "warning" },
          },
          {
            id: "hb2",
            cells: {
              job: "HB-202606-03",
              range: "2024-Q4 · 华北商城",
              source: "通过",
              status: "执行中",
            },
            status: { label: "执行中", tone: "info" },
          },
        ]),
        issues: [
          {
            id: "hi1",
            severity: "info",
            message: "策略未配置：历史退款冲正规则需产品确认",
            objectLabel: "HB-202607-01",
          },
        ],
      },
    },
  },
}

export function getWorkspacePageDef(id: WorkspaceId): WorkspacePageDef {
  const def = WORKSPACE_PAGE_DEFS[id]
  if (!def) {
    throw new Error(`No shared page def for ${id}`)
  }
  return def
}

export const MASTER_DATA_RESOURCES = [
  { key: "sellable-items", label: "可销售项目" },
  { key: "products", label: "商品与 SKU" },
  { key: "voucher-categories", label: "卡券类目" },
  { key: "suppliers", label: "供应商与资质" },
  { key: "warehouses", label: "仓库" },
] as const

export type MasterDataResource =
  (typeof MASTER_DATA_RESOURCES)[number]["key"]

type MasterDataFixture = Readonly<{
  searchPlaceholder: string
  metrics: readonly {
    key: string
    label: string
    value: string | number
    detail?: string
  }[]
  columns: readonly {
    key: string
    header: string
    numeric?: boolean
    status?: boolean
  }[]
  rows: ReturnType<typeof listRows>
}>

/** Per-resource list fixtures for W14 — rows must differ by resource identity. */
export const MASTER_DATA_FIXTURES: Record<MasterDataResource, MasterDataFixture> = {
  "sellable-items": {
    searchPlaceholder: "可销售项目编号、名称、负责人",
    metrics: [
      { key: "enabled", label: "当前启用", value: 426, detail: "可被选择" },
      { key: "disabled", label: "当前停用", value: 38, detail: "历史保留" },
      { key: "expiring", label: "有效期将尽", value: 7, detail: "30 日内" },
      { key: "pending", label: "待生效修订", value: 4, detail: "计划生效" },
    ],
    columns: [
      { key: "code", header: "稳定编号" },
      { key: "name", header: "名称" },
      { key: "version", header: "当前版本" },
      { key: "period", header: "生效区间" },
      { key: "owner", header: "负责人" },
      { key: "status", header: "启停状态", status: true },
    ],
    rows: listRows([
      {
        id: "si_1",
        cells: {
          code: "SI-2026-0188",
          name: "新春坚果礼盒 · 典藏款",
          version: "v6",
          period: "2026-01-01 ~ 2026-12-31",
          owner: "赵强",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled"],
        filterTags: ["可销售项目"],
      },
      {
        id: "si_2",
        cells: {
          code: "SI-2026-0201",
          name: "员工生日蛋糕卡",
          version: "v2",
          period: "2026-03-01 ~ 2026-09-30",
          owner: "李倩",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled", "expiring"],
      },
      {
        id: "si_3",
        cells: {
          code: "SI-2025-0902",
          name: "中秋月饼礼盒（旧版）",
          version: "v9",
          period: "2025-08-01 ~ 2025-10-31",
          owner: "赵强",
          status: "当前停用",
        },
        status: { label: "当前停用", tone: "neutral" },
        metricTags: ["disabled"],
      },
    ]),
  },
  products: {
    searchPlaceholder: "SKU、SPU、规格签名",
    metrics: [
      { key: "enabled", label: "当前启用", value: 912, detail: "可被选择" },
      { key: "disabled", label: "当前停用", value: 64, detail: "历史保留" },
      { key: "expiring", label: "有效期将尽", value: 11, detail: "30 日内" },
      { key: "pending", label: "待生效修订", value: 6, detail: "计划生效" },
    ],
    columns: [
      { key: "code", header: "SKU" },
      { key: "name", header: "商品名称" },
      { key: "spu", header: "SPU" },
      { key: "unit", header: "基础单位" },
      { key: "version", header: "当前版本" },
      { key: "status", header: "启停状态", status: true },
    ],
    rows: listRows([
      {
        id: "prd_1",
        cells: {
          code: "SKU-NY-BOX-01",
          name: "新春坚果礼盒 · 典藏款",
          spu: "SPU-NY-BOX",
          unit: "套",
          version: "v6",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled"],
      },
      {
        id: "prd_2",
        cells: {
          code: "SKU-TEA-09",
          name: "礼盒红茶",
          spu: "SPU-TEA",
          unit: "盒",
          version: "v3",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled"],
      },
      {
        id: "prd_3",
        cells: {
          code: "SKU-OLD-88",
          name: "已下市果篮",
          spu: "SPU-FRUIT",
          unit: "篮",
          version: "v12",
          status: "当前停用",
        },
        status: { label: "当前停用", tone: "neutral" },
        metricTags: ["disabled"],
      },
    ]),
  },
  "voucher-categories": {
    searchPlaceholder: "类目编号、名称",
    metrics: [
      { key: "enabled", label: "当前启用", value: 28, detail: "可被选择" },
      { key: "disabled", label: "当前停用", value: 5, detail: "历史保留" },
      { key: "expiring", label: "有效期将尽", value: 2, detail: "30 日内" },
      { key: "pending", label: "待生效修订", value: 1, detail: "计划生效" },
    ],
    columns: [
      { key: "code", header: "类目编号" },
      { key: "name", header: "类目名称" },
      { key: "sku", header: "卡券 SKU" },
      { key: "version", header: "当前版本" },
      { key: "note", header: "说明" },
      { key: "status", header: "启停状态", status: true },
    ],
    rows: listRows([
      {
        id: "vc_1",
        cells: {
          code: "VC-BIRTHDAY",
          name: "员工生日卡",
          sku: "SKU-CARD-BDAY",
          version: "v4",
          note: "不含商城玩法",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled"],
      },
      {
        id: "vc_2",
        cells: {
          code: "VC-FESTIVAL",
          name: "节日慰问卡",
          sku: "SKU-CARD-FEST",
          version: "v2",
          note: "不含商城玩法",
          status: "当前停用",
        },
        status: { label: "当前停用", tone: "neutral" },
        metricTags: ["disabled"],
      },
    ]),
  },
  suppliers: {
    searchPlaceholder: "供应商编号、企业名称、资质",
    metrics: [
      { key: "enabled", label: "当前启用", value: 86, detail: "可被选择" },
      { key: "disabled", label: "当前停用", value: 9, detail: "历史保留" },
      { key: "expiring", label: "资质将到期", value: 4, detail: "30 日内" },
      { key: "pending", label: "待生效修订", value: 3, detail: "计划生效" },
    ],
    columns: [
      { key: "code", header: "供应商编号" },
      { key: "name", header: "企业名称" },
      { key: "role", header: "供应商角色" },
      { key: "settlement", header: "商务结算版本" },
      { key: "qual", header: "资质预警" },
      { key: "status", header: "启停状态", status: true },
    ],
    rows: listRows([
      {
        id: "sup_1",
        cells: {
          code: "SUP-2026-014",
          name: "鲜果直供供应链",
          role: "实物供应",
          settlement: "v3",
          qual: "正常",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled"],
      },
      {
        id: "sup_2",
        cells: {
          code: "SUP-2025-088",
          name: "礼遇包装工坊",
          role: "包装服务",
          settlement: "v5",
          qual: "资质将到期",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "warning" },
        metricTags: ["enabled", "expiring"],
      },
      {
        id: "sup_3",
        cells: {
          code: "SUP-2024-003",
          name: "旧版物流商（停用）",
          role: "配送",
          settlement: "v1",
          qual: "已失效",
          status: "当前停用",
        },
        status: { label: "当前停用", tone: "neutral" },
        metricTags: ["disabled"],
      },
    ]),
  },
  warehouses: {
    searchPlaceholder: "仓库代码、名称、地址",
    metrics: [
      { key: "enabled", label: "当前启用", value: 12, detail: "可被选择" },
      { key: "disabled", label: "当前停用", value: 2, detail: "历史保留" },
      { key: "expiring", label: "策略将尽", value: 1, detail: "30 日内" },
      { key: "pending", label: "写权限未确认", value: 12, detail: "只读查询" },
    ],
    columns: [
      { key: "code", header: "仓库代码" },
      { key: "name", header: "仓库名称" },
      { key: "region", header: "区域" },
      { key: "contact", header: "联系人" },
      { key: "policy", header: "SKU 预警策略" },
      { key: "status", header: "启停状态", status: true },
    ],
    rows: listRows([
      {
        id: "wh_1",
        cells: {
          code: "WH-EAST-01",
          name: "华东一号仓",
          region: "华东",
          contact: "周航",
          policy: "安全库存 · 默认",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled", "pending"],
      },
      {
        id: "wh_2",
        cells: {
          code: "WH-SOUTH-02",
          name: "华南中转仓",
          region: "华南",
          contact: "陈璐",
          policy: "低周转加强",
          status: "当前启用",
        },
        status: { label: "当前启用", tone: "success" },
        metricTags: ["enabled", "pending"],
      },
      {
        id: "wh_3",
        cells: {
          code: "WH-OLD-99",
          name: "已封存临时仓",
          region: "华北",
          contact: "—",
          policy: "已停用",
          status: "当前停用",
        },
        status: { label: "当前停用", tone: "neutral" },
        metricTags: ["disabled"],
      },
    ]),
  },
}

export function getMasterDataPageDef(
  resource: MasterDataResource
): WorkspacePageDef {
  const label =
    MASTER_DATA_RESOURCES.find((item) => item.key === resource)?.label ??
    resource
  const fixture = MASTER_DATA_FIXTURES[resource]
  return {
    id: "W14",
    title: `主数据 · ${label}`,
    description:
      "维护可销售项目、商品、卡券类目、供应商与仓库；版本变更可追溯。",
    mode: "M2+M4",
    breadcrumbs: [
      { id: "md", label: "主数据", href: "/master-data/sellable-items" },
      { id: "resource", label },
    ],
    shell: {
      kind: "list",
      payload: {
        searchPlaceholder: fixture.searchPlaceholder,
        primaryActionLabel: "新建 / 形成新版本",
        filterLabels: ["全部", "当前启用", "当前停用"],
        metrics: fixture.metrics,
        columns: fixture.columns,
        rows: fixture.rows,
      },
    },
  }
}
