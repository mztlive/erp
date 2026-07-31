"use client"

import * as React from "react"
import Link from "next/link"
import {
  ArchiveIcon,
  BellIcon,
  FileCheck2Icon,
  LayoutDashboardIcon,
  MoonIcon,
  PackageIcon,
  PlusIcon,
  ReceiptTextIcon,
  SettingsIcon,
  StampIcon,
  SunIcon,
  TriangleAlertIcon,
  UsersIcon,
  WalletIcon,
} from "lucide-react"

import {
  AdvancedFilterSheet,
  AfterSalesTrackPanel,
  AllocationWorkspace,
  ApprovalDecisionPanel,
  AsyncSectionState,
  AuditTimeline,
  BackgroundJobProgress,
  BatchImpactPreview,
  BatchOperationResult,
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessObjectCombobox,
  BusinessObjectRef,
  BusinessStatusBadge,
  BusinessTableFrame,
  CardVoucherLineItem,
  ConflictResolutionDialog,
  CostCoverageNotice,
  DataFreshness,
  DataTable,
  DataTableViewOptions,
  DocumentAttachmentList,
  DocumentHeader,
  DocumentSection,
  DocumentSummary,
  DocumentTotals,
  DraftSaveIndicator,
  EditableLineItemTable,
  EditorPresence,
  ErpAppShell,
  FormalActionConfirmDialog,
  FormalActionResult,
  GlobalTopbar,
  GuardedBusinessAction,
  ImportIssueTable,
  ImportStageIndicator,
  InterfaceErrorResolutionPanel,
  InventoryBalanceSummary,
  ListToolbar,
  MaintenanceBanner,
  MetricItem,
  MetricStrip,
  MoneyValue,
  PageActions,
  PageHeader,
  PaperDocument,
  PrepaymentGate,
  QuantityValue,
  QuickPreviewSheet,
  RateValue,
  RelatedDocumentList,
  ResponsibilityPanel,
  RevisionTimeline,
  SavedViewPicker,
  SelectionScopeBar,
  SensitiveValue,
  SequentialProcessBar,
  StatusMatrix,
  StatusTrackSummary,
  TaskTabs,
  ValidationSummary,
  WorkTaskItem,
  type ColumnDef,
  type EditableLineItemColumn,
  type PaginationState,
  type PaperDocumentColumn,
  type RowSelectionState,
  type SortingState,
  type VisibilityState,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Separator } from "@/components/ui/separator"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"

/* -------------------------------------------------------------------------
   本页是业务组件预览与验收页，不取数、不提交，也不承载真实业务流程。
   交互状态仅用于展示 Tabs、选择、Dialog、Sheet 与 TanStack Table 行为。
   ------------------------------------------------------------------------- */

const noop = () => undefined

const NAVIGATION = [
  { label: "工作台", icon: LayoutDashboardIcon },
  { label: "销售与客户", icon: UsersIcon, badge: "12", active: true },
  { label: "采购与供应商", icon: ReceiptTextIcon },
  { label: "商品与库存", icon: PackageIcon },
  { label: "财务结算", icon: WalletIcon, badge: "3" },
  { label: "异常与对账", icon: TriangleAlertIcon, badge: "2" },
  { label: "系统管理", icon: SettingsIcon },
] as const

type OrderRow = {
  id: string
  number: string
  customer: string
  amount: string
  quantity: string
  rate: string
  status: "待审批" | "已生效" | "已阻断"
}

const ORDERS: OrderRow[] = [
  {
    id: "order-01",
    number: "SO-20260731-0148",
    customer: "北方能源集团",
    amount: "1284650.00",
    quantity: "320",
    rate: "12.50",
    status: "已生效",
  },
  {
    id: "order-02",
    number: "SO-20260731-0147",
    customer: "华东数字科技有限公司",
    amount: "356200.50",
    quantity: "86",
    rate: "8.00",
    status: "待审批",
  },
  {
    id: "order-03",
    number: "SO-20260730-0146",
    customer: "西南建设投资",
    amount: "92800.00",
    quantity: "24",
    rate: "0.00",
    status: "已阻断",
  },
]

const ORDER_COLUMNS: ColumnDef<OrderRow, unknown>[] = [
  {
    accessorKey: "number",
    header: "销售单",
    meta: { label: "销售单", width: "reference" },
    cell: ({ row }) => (
      <BusinessObjectRef
        objectType="销售单"
        stableNumber={row.original.number}
        title={row.original.customer}
      />
    ),
  },
  {
    accessorKey: "status",
    header: "主状态",
    meta: { label: "主状态", width: "status" },
    cell: ({ row }) => (
      <BusinessStatusBadge
        context="list"
        label={row.original.status}
        tone={
          row.original.status === "已生效"
            ? "success"
            : row.original.status === "已阻断"
              ? "destructive"
              : "warning"
        }
      />
    ),
  },
  {
    accessorKey: "amount",
    header: "含税金额",
    meta: {
      label: "含税金额",
      align: "end",
      numeric: true,
      width: "amount",
    },
    cell: ({ row }) => (
      <MoneyValue value={row.original.amount} taxBasis="gross" />
    ),
  },
  {
    accessorKey: "quantity",
    header: "数量",
    meta: {
      label: "数量",
      align: "end",
      numeric: true,
      width: "quantity",
    },
    cell: ({ row }) => (
      <QuantityValue value={row.original.quantity} unit="份" />
    ),
  },
  {
    accessorKey: "rate",
    header: "配赠率",
    meta: {
      label: "配赠率",
      align: "end",
      numeric: true,
      width: "rate",
    },
    cell: ({ row }) => <RateValue value={row.original.rate} precision={2} />,
  },
  {
    id: "tracks",
    header: "并行状态",
    meta: { label: "并行状态", width: "tracks" },
    enableSorting: false,
    cell: ({ row }) => (
      <StatusTrackSummary
        tracks={[
          {
            id: "fulfillment",
            label: "履约",
            status: {
              label: row.original.status === "已生效" ? "部分履约" : "未开始",
              tone: row.original.status === "已生效" ? "warning" : "neutral",
            },
          },
          {
            id: "payment",
            label: "回款",
            status: { label: "待复核", tone: "info" },
          },
        ]}
      />
    ),
  },
]

type LineItem = {
  id: string
  product: string
  quantity: string
  unitPrice: string
}

const LINE_ITEMS: LineItem[] = [
  {
    id: "line-01",
    product: "员工福利组合包 A",
    quantity: "320",
    unitPrice: "3800.00",
  },
  {
    id: "line-02",
    product: "企业关怀服务",
    quantity: "1",
    unitPrice: "68650.00",
  },
]

const LINE_COLUMNS: EditableLineItemColumn<LineItem>[] = [
  {
    id: "product",
    header: "商品或服务",
    renderValue: ({ item }) => item.product,
    renderEditor: ({ item, rowIndex }) => (
      <Input
        aria-label={`第 ${rowIndex + 1} 行商品`}
        defaultValue={item.product}
        disabled
      />
    ),
  },
  {
    id: "quantity",
    header: "数量",
    align: "end",
    numeric: true,
    renderValue: ({ item }) => (
      <QuantityValue value={item.quantity} unit="份" />
    ),
    renderEditor: ({ item, rowIndex }) => (
      <Input
        aria-label={`第 ${rowIndex + 1} 行数量`}
        defaultValue={item.quantity}
        disabled
      />
    ),
  },
  {
    id: "unitPrice",
    header: "含税单价",
    align: "end",
    numeric: true,
    renderValue: ({ item }) => (
      <MoneyValue value={item.unitPrice} taxBasis="gross" />
    ),
    renderEditor: ({ item, rowIndex }) => (
      <Input
        aria-label={`第 ${rowIndex + 1} 行含税单价`}
        defaultValue={item.unitPrice}
        disabled
      />
    ),
  },
]

type PaperSalesLine = {
  id: string
  product: string
  reference: string
  quantity: string
  unit: string
  unitPrice: string
  taxRate: string
  lineAmount: string
}

const PAPER_SALES_LINES: PaperSalesLine[] = [
  {
    id: "paper-line-01",
    product: "员工福利组合包 A",
    reference: "SKU-FL-2026-A · 标准企业版",
    quantity: "320",
    unit: "份",
    unitPrice: "3800.00",
    taxRate: "6.00",
    lineAmount: "1216000.00",
  },
  {
    id: "paper-line-02",
    product: "企业关怀服务",
    reference: "SRV-CARE-01 · 年度服务",
    quantity: "1",
    unit: "项",
    unitPrice: "68650.00",
    taxRate: "6.00",
    lineAmount: "68650.00",
  },
]

const PAPER_SALES_COLUMNS: PaperDocumentColumn<PaperSalesLine>[] = [
  {
    id: "product",
    header: "商品或服务",
    cell: (row) => (
      <div>
        <div className="font-medium">{row.product}</div>
        <div className="num mt-1 text-xs text-muted-foreground">
          {row.reference}
        </div>
      </div>
    ),
  },
  {
    id: "quantity",
    header: "数量",
    align: "end",
    numeric: true,
    cell: (row) => row.quantity,
  },
  {
    id: "unit",
    header: "单位",
    align: "center",
    cell: (row) => row.unit,
  },
  {
    id: "unit-price",
    header: "含税单价",
    align: "end",
    numeric: true,
    cell: (row) => <MoneyValue value={row.unitPrice} />,
  },
  {
    id: "tax-rate",
    header: "税率",
    align: "end",
    numeric: true,
    cell: (row) => <RateValue value={row.taxRate} precision={2} />,
  },
  {
    id: "line-amount",
    header: "含税金额",
    align: "end",
    numeric: true,
    cell: (row) => <MoneyValue value={row.lineAmount} />,
  },
]

const VOUCHER_VALUES = {
  category: "通用购物卡",
  fulfillmentDeadline: {
    dateTime: "2026-08-05T18:00:00+08:00",
    label: "2026-08-05 18:00",
  },
  faceValue: "¥1,000.00",
  cardCount: "500 张",
  unitPriceGross: "¥970.00",
  transactionAmount: "¥485,000.00",
  faceValueTotal: "¥500,000.00",
  giftAmount: "¥15,000.00",
  giftRate: "3.00%",
  cardForm: "电子卡",
} as const

function ShowcaseSection({
  title,
  description,
  children,
}: {
  title: string
  description: string
  children: React.ReactNode
}) {
  return (
    <section className="border-t border-border py-6 first:border-t-0 first:pt-0">
      <div className="mb-4 flex flex-col gap-1">
        <h2 className="font-heading text-lg font-semibold text-foreground">
          {title}
        </h2>
        <p className="text-sm text-muted-foreground">{description}</p>
      </div>
      {children}
    </section>
  )
}

export default function BusinessComponentsPreviewPage() {
  const [dark, setDark] = React.useState(false)
  const [section, setSection] = React.useState("lists")
  const [task, setTask] = React.useState("components")
  const [previewOpen, setPreviewOpen] = React.useState(false)
  const [filterOpen, setFilterOpen] = React.useState(false)
  const [selectionScope, setSelectionScope] = React.useState<
    "page" | "filtered"
  >("page")
  const [sorting, setSorting] = React.useState<SortingState>([])
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [rowSelection, setRowSelection] = React.useState<RowSelectionState>({
    "order-01": true,
  })
  const [columnVisibility, setColumnVisibility] =
    React.useState<VisibilityState>({})
  const [savedView, setSavedView] = React.useState<string | undefined>(
    "pending"
  )
  const [businessObject, setBusinessObject] = React.useState<
    string | undefined
  >("customer-01")

  const selectedCount = Object.keys(rowSelection).length

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", dark)
  }, [dark])

  return (
    <ErpAppShell
      contentLabel="业务组件预览"
      sidebarHeader={
        <div className="flex h-tabs items-center gap-2 px-2">
          <div className="flex size-6 shrink-0 items-center justify-center rounded-md bg-sidebar-primary text-xs font-semibold text-sidebar-primary-foreground">
            E
          </div>
          <span className="truncate text-sm font-semibold group-data-[collapsible=icon]:hidden">
            员工福利 ERP
          </span>
        </div>
      }
      sidebarContent={
        <SidebarGroup>
          <SidebarGroupLabel>业务导航</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {NAVIGATION.map((item) => {
                const Icon = item.icon
                return (
                  <SidebarMenuItem key={item.label}>
                    <SidebarMenuButton
                      tooltip={item.label}
                      isActive={"active" in item && item.active}
                    >
                      <Icon aria-hidden="true" />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                    {"badge" in item ? (
                      <SidebarMenuBadge>{item.badge}</SidebarMenuBadge>
                    ) : null}
                  </SidebarMenuItem>
                )
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      }
      sidebarFooter={
        <p className="px-2 text-xs text-sidebar-foreground/70 group-data-[collapsible=icon]:hidden">
          静态组件验收环境
        </p>
      }
      topbar={
        <GlobalTopbar
          search={{
            ariaLabel: "搜索组件",
            placeholder: "搜索组件名称或业务模式",
            shortcut: "⌘ K",
          }}
          actions={[
            {
              actionKey: "theme",
              label: dark ? "浅色模式" : "深色模式",
              icon: dark ? SunIcon : MoonIcon,
              variant: "ghost",
              onClick: () => setDark((current) => !current),
            },
            {
              actionKey: "notice",
              label: "通知",
              icon: BellIcon,
              variant: "ghost",
              badge: { label: "4", variant: "destructive" },
              onClick: noop,
            },
          ]}
        />
      }
      maintenanceBanner={
        <MaintenanceBanner
          tone="info"
          title="业务组件验收页"
          description="当前页面只使用静态数据；真实页面的数据请求与表单状态仍分别交给 TanStack Query 和 TanStack Form。"
          action={{ label: "验收说明", onClick: noop }}
        />
      }
      taskTabs={
        <TaskTabs
          value={task}
          onValueChange={setTask}
          items={[
            {
              value: "components",
              label: "业务组件",
              icon: LayoutDashboardIcon,
            },
            { value: "order", label: "SO-20260731-0148", icon: FileCheck2Icon },
            {
              value: "exceptions",
              label: "异常任务",
              icon: TriangleAlertIcon,
              badge: { label: "2", variant: "warning" },
            },
          ]}
        />
      }
    >
      <main className="mx-auto flex w-full max-w-7xl flex-col gap-6 p-4 lg:p-6">
        <PageHeader
          title="业务组件库"
          description="64 个业务组件的静态预览与交互验收；基础色彩、密度和状态语义请对照 Theme 页面。"
          breadcrumbs={[
            { id: "design", label: "设计系统", href: "/theme" },
            { id: "business", label: "业务组件", current: true },
          ]}
          status={{ label: "13 个模块", tone: "success" }}
          metadata={
            <DataFreshness
              updatedAt="2026-07-31 18:30"
              dateTime="2026-07-31T18:30:00+08:00"
              state="fresh"
              label="验收快照"
            />
          }
          actions={
            <div className="flex flex-wrap items-center gap-2">
              <Link
                href="/theme"
                className={buttonVariants({ variant: "outline" })}
              >
                查看 Theme
              </Link>
              <PageActions
                actions={[
                  {
                    actionKey: "archive",
                    label: "归档快照",
                    icon: ArchiveIcon,
                    variant: "secondary",
                    onClick: noop,
                  },
                  {
                    actionKey: "new",
                    label: "登记缺口",
                    icon: PlusIcon,
                    onClick: noop,
                  },
                ]}
              />
            </div>
          }
        />

        <Tabs value={section} onValueChange={setSection}>
          <TabsList
            variant="line"
            aria-label="业务组件分组"
            className="max-w-full overflow-x-auto"
          >
            <TabsTrigger value="lists">列表与选择</TabsTrigger>
            <TabsTrigger value="documents">单据与流程</TabsTrigger>
            <TabsTrigger value="domains">领域规则</TabsTrigger>
            <TabsTrigger value="feedback">反馈与审计</TabsTrigger>
          </TabsList>

          <TabsContent value="lists" className="space-y-6">
            <ShowcaseSection
              title="页面摘要"
              description="PageHeader、PageActions、MetricStrip 与 DataFreshness 形成统一的页面入口层。"
            >
              <MetricStrip columns={4}>
                <MetricItem
                  label="正式组件"
                  value="64"
                  detail="来自 13 个业务模块"
                  status={{ label: "已覆盖", tone: "success" }}
                />
                <MetricItem
                  label="当前筛选命中"
                  value="128"
                  detail="销售单 · 待处理视图"
                />
                <MetricItem
                  label="待审批"
                  value="12"
                  detail="销售中心"
                  status={{ label: "需关注", tone: "warning" }}
                />
                <MetricItem
                  label="接口异常"
                  value="2"
                  detail="均已形成待办"
                  status={{ label: "可追踪", tone: "info" }}
                />
              </MetricStrip>
            </ShowcaseSection>

            <ShowcaseSection
              title="工作任务"
              description="任务条目显式呈现进入队列时间、期限、责任方、原因、影响和下一动作。"
            >
              <WorkTaskItem
                taskType="销售审批"
                businessObject="SO-20260731-0147"
                counterparty="华东数字科技有限公司"
                enteredAt="07-31 16:20"
                enteredDateTime="2026-07-31T16:20:00+08:00"
                dueAt="08-01 12:00"
                dueDateTime="2026-08-01T12:00:00+08:00"
                responsibleParty="销售运营组"
                reason="订单折扣超过当前权限阈值"
                impact="审批通过后锁定价格与客户快照"
                status={{ label: "待处理", tone: "warning" }}
                nextAction={
                  <Button size="sm" onClick={noop}>
                    打开任务
                  </Button>
                }
              />
            </ShowcaseSection>

            <ShowcaseSection
              title="数据列表"
              description="DataTable 基于 TanStack Table v8，示例完整受控且使用稳定 ERP ID 作为行身份。"
            >
              <BusinessTableFrame
                title="销售订单"
                description="工具栏、选择范围、表格、列设置和分页均由业务层组合。"
                toolbar={
                  <ListToolbar
                    savedView={
                      <SavedViewPicker
                        views={[
                          {
                            id: "pending",
                            label: "我的待处理",
                            scope: "personal",
                          },
                          {
                            id: "team",
                            label: "团队全部订单",
                            scope: "team",
                            readOnly: true,
                          },
                        ]}
                        value={savedView}
                        onValueChange={setSavedView}
                      />
                    }
                    search={
                      <Input
                        aria-label="搜索销售订单"
                        placeholder="单号、客户或外部单号"
                      />
                    }
                    filters={
                      <Button
                        variant="outline"
                        onClick={() => setFilterOpen(true)}
                      >
                        高级筛选
                      </Button>
                    }
                    actions={
                      <Button
                        variant="outline"
                        onClick={() => setPreviewOpen(true)}
                      >
                        打开快速预览
                      </Button>
                    }
                  />
                }
                selectionBar={
                  <SelectionScopeBar
                    scope={selectionScope}
                    selectedCount={
                      selectionScope === "filtered" ? 128 : selectedCount
                    }
                    pageItemCount={ORDERS.length}
                    filteredCount={128}
                    filterSummary="状态：待审批、已生效"
                    onScopeChange={setSelectionScope}
                    onClearSelection={() => setRowSelection({})}
                    actions={
                      <Button size="sm" variant="secondary" onClick={noop}>
                        批量导出
                      </Button>
                    }
                  />
                }
                table={
                  <DataTable
                    data={ORDERS}
                    columns={ORDER_COLUMNS}
                    getRowId={(row) => row.id}
                    rowCount={128}
                    rowLabel={(row) => `${row.number} ${row.customer}`}
                    sorting={sorting}
                    onSortingChange={setSorting}
                    pagination={pagination}
                    onPaginationChange={setPagination}
                    rowSelection={rowSelection}
                    onRowSelectionChange={setRowSelection}
                    columnVisibility={columnVisibility}
                    onColumnVisibilityChange={setColumnVisibility}
                    enableRowSelection
                    manualPagination
                    manualSorting
                    manualFiltering
                    layout="inset"
                    density="comfortable"
                    showColumnVisibility={false}
                    onRowPreview={() => setPreviewOpen(true)}
                    renderToolbar={(table) => (
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <p className="text-xs text-muted-foreground">
                          单击行打开快速预览；Enter 优先详情，未绑定时回退预览。
                        </p>
                        <DataTableViewOptions table={table} />
                      </div>
                    )}
                  />
                }
                footer={
                  <>
                    <span className="text-xs text-muted-foreground">
                      显示服务端准确总数
                    </span>
                    <DataFreshness updatedAt="18:28:16" state="syncing" />
                  </>
                }
              />
            </ShowcaseSection>
          </TabsContent>

          <TabsContent value="documents" className="space-y-6">
            <ShowcaseSection
              title="纸质单据预览"
              description="将服务端确认的正式事实投影为统一纸张版式；窄屏横向查看，打印时移除应用画布与阴影。"
            >
              <PaperDocument
                issuer="美烨企业福利服务有限公司"
                title="销售订单"
                subtitle="员工福利采购"
                documentNumber="SO-20260731-0148"
                version="v4"
                status={{ label: "已生效", tone: "success" }}
                parties={[
                  {
                    id: "buyer",
                    label: "购方",
                    name: "北方能源集团有限公司",
                    reference: "CUST-0018",
                    fields: [
                      {
                        id: "buyer-tax",
                        label: "统一社会信用代码",
                        value: "91110000123456789X",
                        numeric: true,
                      },
                      {
                        id: "buyer-contact",
                        label: "联系人",
                        value: "刘嘉 · 138 **** 6208",
                      },
                      {
                        id: "buyer-address",
                        label: "交付地址",
                        value: "北京市朝阳区建国路 88 号",
                      },
                      {
                        id: "buyer-department",
                        label: "采购部门",
                        value: "集团行政采购中心",
                      },
                    ],
                  },
                  {
                    id: "seller",
                    label: "销方",
                    name: "美烨企业福利服务有限公司",
                    reference: "ORG-SH-0001",
                    fields: [
                      {
                        id: "seller-tax",
                        label: "统一社会信用代码",
                        value: "91310000MA1FL2026X",
                        numeric: true,
                      },
                      {
                        id: "seller-contact",
                        label: "销售负责人",
                        value: "赵晨 · 021-**** 1806",
                      },
                      {
                        id: "seller-address",
                        label: "联系地址",
                        value: "上海市徐汇区宜山路 900 号",
                      },
                      {
                        id: "seller-bank",
                        label: "结算账户",
                        value: "招商银行上海分行 · 尾号 2188",
                      },
                    ],
                  },
                ]}
                metadata={[
                  {
                    id: "effective-at",
                    label: "生效日期",
                    value: "2026-07-31",
                  },
                  { id: "owner", label: "销售负责人", value: "赵晨" },
                  { id: "currency", label: "币种", value: "CNY 人民币" },
                  { id: "settlement", label: "结算方式", value: "先款后货" },
                ]}
                columns={PAPER_SALES_COLUMNS}
                rows={PAPER_SALES_LINES}
                getRowId={(row) => row.id}
                totals={[
                  {
                    id: "line-total",
                    label: "明细含税合计",
                    value: <MoneyValue value="1284650.00" />,
                  },
                  {
                    id: "discount",
                    label: "整单优惠",
                    value: <MoneyValue value="0.00" />,
                  },
                  {
                    id: "receivable",
                    label: "应收总额",
                    description: "人民币壹佰贰拾捌万肆仟陆佰伍拾元整",
                    value: <MoneyValue value="1284650.00" />,
                    emphasized: true,
                  },
                ]}
                remarks={
                  <p>
                    交付前须完成全额到账复核；商品规格、税率及金额以当前有效版本为准。
                  </p>
                }
                signature={
                  <dl className="grid grid-cols-2 gap-8 text-sm">
                    <div>
                      <dt className="text-muted-foreground">制单</dt>
                      <dd className="mt-3 border-b border-border pb-2">王敏</dd>
                    </div>
                    <div>
                      <dt className="text-muted-foreground">审核</dt>
                      <dd className="mt-3 border-b border-border pb-2">陈莉</dd>
                    </div>
                  </dl>
                }
                seal={
                  <div className="flex items-center gap-3 rounded-lg border border-border p-3 text-sm">
                    <StampIcon
                      className="size-5 shrink-0 text-primary"
                      aria-hidden="true"
                    />
                    <div>
                      <div className="font-medium">电子签章已验证</div>
                      <div className="mt-1 text-xs text-muted-foreground">
                        预览不展示印模
                      </div>
                    </div>
                  </div>
                }
                footer={
                  <div className="flex items-center justify-between gap-6">
                    <span>生成时间：2026-07-31 18:28:16（Asia/Shanghai）</span>
                    <span className="num">SO-20260731-0148 · v4</span>
                  </div>
                }
              />
            </ShowcaseSection>

            <ShowcaseSection
              title="销售单据"
              description="单据头、摘要、章节、状态轴、金额口径与关联事实保持清晰分层。"
            >
              <div className="space-y-6">
                <DocumentHeader
                  title="销售订单"
                  documentNumber="SO-20260731-0148"
                  version="v4"
                  primaryStatus={{ label: "已生效", tone: "success" }}
                  statuses={[
                    {
                      id: "fulfillment",
                      label: "履约",
                      status: { label: "部分履约", tone: "warning" },
                    },
                    {
                      id: "payment",
                      label: "回款",
                      status: { label: "待复核", tone: "info" },
                    },
                    {
                      id: "invoice",
                      label: "开票",
                      status: { label: "未开", tone: "neutral" },
                    },
                  ]}
                  secondaryActions={
                    <ConflictResolutionDialog
                      trigger={<Button variant="outline">冲突处理</Button>}
                      currentVersion="v4"
                      localBaseline="v3"
                      actor="王敏"
                      changedAt="18:12"
                      diff={
                        <p className="text-sm">
                          含税金额由 ¥1,260,000.00 调整为 ¥1,284,650.00。
                        </p>
                      }
                      onReload={noop}
                      onSaveCopy={noop}
                      onCompare={noop}
                    />
                  }
                  primaryAction={
                    <FormalActionConfirmDialog
                      trigger={<Button>提交生效</Button>}
                      actionLabel="生效"
                      fromStatus="待审批"
                      toStatus={{ label: "已生效", tone: "success" }}
                      lockedFields={["客户快照", "价格与税率", "销售明细"]}
                      effects={["形成正式销售事实", "进入履约与回款责任轨道"]}
                      nextDepartment="履约运营组"
                      onConfirm={() => Promise.resolve()}
                    />
                  }
                />

                <StatusTrackSummary
                  variant="table"
                  tracks={[
                    {
                      id: "main",
                      label: "主状态",
                      status: {
                        label: "已生效",
                        tone: "success",
                        description: "版本 v4",
                      },
                    },
                    {
                      id: "fulfillment",
                      label: "履约",
                      status: {
                        label: "部分履约",
                        tone: "warning",
                        description: "2 个责任轨道进行中",
                      },
                    },
                    {
                      id: "payment",
                      label: "回款",
                      status: {
                        label: "待复核",
                        tone: "info",
                        description: "最近到账待认领",
                      },
                    },
                  ]}
                />

                <DocumentSummary
                  columns="four"
                  items={[
                    {
                      id: "customer",
                      label: "客户",
                      value: "北方能源集团",
                      emphasized: true,
                    },
                    { id: "owner", label: "销售负责人", value: "赵晨" },
                    {
                      id: "effective",
                      label: "生效时间",
                      value: "2026-07-31 16:02",
                    },
                    {
                      id: "amount",
                      label: "含税总额",
                      value: <MoneyValue value="1284650.00" taxBasis="gross" />,
                      numeric: true,
                    },
                  ]}
                />

                <DocumentSection
                  title="销售明细"
                  description="编辑字段由 TanStack Form 注入；此处使用禁用静态 slot 验收布局。"
                  action={
                    <DraftSaveIndicator state="saved" savedAt="18:20:06" />
                  }
                >
                  <EditableLineItemTable
                    items={LINE_ITEMS}
                    columns={LINE_COLUMNS}
                    getRowId={(item) => item.id}
                    mode="edit"
                    disabled
                    onRemoveItem={noop}
                    onAddItem={noop}
                    addDisabledReason="预览页不修改业务数据"
                  />
                </DocumentSection>

                <DocumentSection title="责任轨道" collapsible>
                  <ResponsibilityPanel
                    tracks={[
                      {
                        id: "delivery",
                        label: "商品履约",
                        description: "完成出库与客户签收",
                        status: { label: "进行中", tone: "warning" },
                        owner: "履约运营组",
                        dueAt: {
                          dateTime: "2026-08-05T18:00:00+08:00",
                          label: "08-05 18:00",
                        },
                        action: (
                          <Button size="sm" onClick={noop}>
                            查看履约
                          </Button>
                        ),
                      },
                      {
                        id: "invoice",
                        label: "开票",
                        status: { label: "未开始", tone: "neutral" },
                        owner: "财务结算组",
                        disabledReason: "需等待客户开票信息复核",
                        blocker: { reason: "客户税号待确认", tone: "warning" },
                      },
                    ]}
                  />
                </DocumentSection>

                <div className="grid gap-6 xl:grid-cols-2">
                  <DocumentSection title="关联单据">
                    <RelatedDocumentList
                      documents={[
                        {
                          id: "delivery-01",
                          documentType: "履约单",
                          documentNumber: "DO-20260731-0036",
                          status: { label: "部分完成", tone: "warning" },
                          measure: {
                            kind: "quantity",
                            value: "260",
                            unit: "份",
                          },
                          owner: "履约运营组",
                          openAction: (
                            <Button size="sm" variant="ghost" onClick={noop}>
                              打开
                            </Button>
                          ),
                        },
                      ]}
                    />
                  </DocumentSection>
                  <DocumentSection title="版本记录">
                    <RevisionTimeline
                      revisions={[
                        {
                          id: "v4",
                          version: 4,
                          source: "erp-change",
                          actor: "王敏",
                          effectiveAt: {
                            dateTime: "2026-07-31T18:12:00+08:00",
                            label: "07-31 18:12",
                          },
                          reason: "客户补充采购范围后更新金额。",
                          isCurrent: true,
                        },
                        {
                          id: "v3",
                          version: 3,
                          source: "mall-sync",
                          actor: "商城同步任务",
                          effectiveAt: {
                            dateTime: "2026-07-31T16:02:00+08:00",
                            label: "07-31 16:02",
                          },
                        },
                      ]}
                    />
                  </DocumentSection>
                </div>

                <div className="grid gap-6 xl:grid-cols-2">
                  <DocumentTotals
                    items={[
                      {
                        id: "gross",
                        label: "含税总额",
                        basis: "销售口径",
                        value: (
                          <MoneyValue value="1284650.00" taxBasis="gross" />
                        ),
                      },
                      {
                        id: "paid",
                        label: "已分配回款",
                        basis: "过账净额",
                        value: <MoneyValue value="620000.00" />,
                      },
                    ]}
                    warning="成本覆盖不完整时，利润值必须与覆盖率一起解读。"
                  />
                  <DocumentAttachmentList
                    attachments={[
                      {
                        id: "contract",
                        name: "采购合同.pdf",
                        state: "done",
                        required: true,
                        description: "2.4 MB · 已完成病毒扫描",
                        onOpen: noop,
                      },
                      {
                        id: "quote",
                        name: "报价确认单.xlsx",
                        state: "processing",
                        description: "正在解析",
                      },
                      {
                        id: "evidence",
                        name: "客户盖章页.png",
                        state: "error",
                        errorMessage: "文件校验失败",
                        onRetry: noop,
                      },
                    ]}
                  />
                </div>
              </div>
            </ShowcaseSection>

            <ShowcaseSection
              title="编辑、审批与批处理"
              description="受控编辑骨架只接收页面注入的字段和已计算结果，不在组件内保存业务表单。"
            >
              <div className="space-y-6">
                <EditorPresence
                  editors={[{ id: "u1", name: "王敏" }]}
                  viewers={[
                    { id: "u2", name: "赵晨" },
                    { id: "u3", name: "陈莉" },
                  ]}
                />
                <SequentialProcessBar
                  current={3}
                  total={12}
                  leaseStatus="active"
                  onBack={noop}
                  onProcess={noop}
                  onProcessNext={noop}
                  onReclaim={noop}
                />
                <div className="grid gap-6 xl:grid-cols-2">
                  <ApprovalDecisionPanel
                    summaryItems={[
                      {
                        id: "amount",
                        label: "审批金额",
                        value: <MoneyValue value="1284650.00" />,
                        numeric: true,
                      },
                      {
                        id: "discount",
                        label: "折扣率",
                        value: <RateValue value="92.00" precision={2} />,
                        numeric: true,
                      },
                    ]}
                    opinionField={
                      <div className="space-y-2">
                        <Label htmlFor="approval-opinion">审批意见</Label>
                        <Textarea
                          id="approval-opinion"
                          defaultValue="静态预览：字段由业务页的 TanStack Form 提供。"
                          disabled
                        />
                      </div>
                    }
                    effects={["同意后形成正式销售版本", "价格与客户快照将锁定"]}
                    onApprove={noop}
                    onReject={noop}
                  />
                  <AllocationWorkspace
                    title="回款分配"
                    summary={{
                      totalToAllocate: "¥620,000.00",
                      allocated: "¥620,000.00",
                      difference: "¥0.00",
                    }}
                    allocations={LINE_ITEMS.slice(0, 1)}
                    columns={LINE_COLUMNS}
                    getRowId={(item) => item.id}
                    disabled
                    addDisabledReason="预览页为只读"
                    onAddAllocation={noop}
                    onRemoveAllocation={noop}
                    statusNotice={
                      <BusinessStatusBadge
                        label="分配平衡"
                        tone="success"
                        context="detail"
                        description="差额为零"
                      />
                    }
                    actions={<Button disabled>确认分配</Button>}
                  />
                </div>
                <BatchImpactPreview
                  filterSummary="状态为已生效，履约状态为待处理"
                  selectionScope="全部符合当前筛选的 128 条记录"
                  estimated={128}
                  processable={124}
                  skipped={4}
                  skippedReason="4 条记录当前无导出权限"
                  background
                  sensitiveFields={["客户联系人", "手机号"]}
                />
              </div>
            </ShowcaseSection>
          </TabsContent>

          <TabsContent value="domains" className="space-y-6">
            <ShowcaseSection
              title="卡券销售"
              description="唯一明细把来源事实、派生金额、卡形态与 Asia/Shanghai 履约期限放在同一约束边界内。"
            >
              <CardVoucherLineItem mode="readonly" values={VOUCHER_VALUES} />
            </ShowcaseSection>

            <ShowcaseSection
              title="履约与库存门禁"
              description="先款后货与库存余额只展示服务端已经计算的正式事实。"
            >
              <div className="space-y-6">
                <PrepaymentGate
                  condition={{
                    kind: "ratio",
                    required: "50.00%",
                    description: "按有效净付款分配计算",
                  }}
                  allocated="48.26%"
                  gap="1.74%"
                  updatedAt={{
                    dateTime: "2026-07-31T18:26:00+08:00",
                    label: "07-31 18:26",
                  }}
                  allowed={false}
                  paymentAction={
                    <Button variant="outline" onClick={noop}>
                      查看付款分配
                    </Button>
                  }
                />
                <InventoryBalanceSummary
                  onHand="1,280"
                  reserved="360"
                  available="920"
                  pendingInbound="480"
                  pendingOutbound="260"
                  unit="份"
                  updatedAt={{
                    dateTime: "2026-07-31T18:25:00+08:00",
                    label: "07-31 18:25",
                  }}
                />
              </div>
            </ShowcaseSection>

            <ShowcaseSection
              title="售后、成本与接口异常"
              description="售后四轨不合并状态，成本覆盖不以零替代未知，接口异常先查询原结果再决定动作。"
            >
              <div className="space-y-6">
                <AfterSalesTrackPanel
                  request={{
                    applicability: "required",
                    status: "completed",
                    description: "客户取消 20 张电子卡",
                    owner: "客户运营组",
                    occurredAt: {
                      dateTime: "2026-07-31T17:00:00+08:00",
                      label: "07-31 17:00",
                    },
                    evidence: "ASR-20260731-009",
                  }}
                  refund={{
                    applicability: "required",
                    status: "pending",
                    amount: "¥19,400.00",
                    owner: "商城售后组",
                    action: (
                      <Button size="sm" onClick={noop}>
                        查看退款
                      </Button>
                    ),
                  }}
                  balanceRestoration={{
                    applicability: "required",
                    status: "completed",
                    amount: "¥20,000.00",
                    owner: "卡券运营组",
                    evidence: "余额恢复回执编号 4481",
                  }}
                  supplierRefund={{
                    applicability: "not-applicable",
                    status: "not-applicable",
                    reason: "本次订单未发生供应商采购",
                  }}
                />
                <CostCoverageNotice
                  basis="STANDARD"
                  coveragePercent={86.4}
                  coverageLabel="86.40%"
                  coverageState="partial"
                  breakdown={{
                    ACTUAL: "¥286,000.00",
                    STANDARD: "¥918,650.00",
                    NONE: "¥80,000.00",
                  }}
                  profitBasis="按当前已覆盖成本计算，不含 NONE 范围"
                />
                <InterfaceErrorResolutionPanel
                  errorClass="result-unknown"
                  status="manual-required"
                  errorCode="EXTERNAL_RESULT_UNKNOWN"
                  businessImpact="20 张卡券交付终态未知，不得重复下单"
                  latestAttempt={{
                    attemptNumber: 2,
                    attemptedAt: {
                      dateTime: "2026-07-31T18:08:00+08:00",
                      label: "07-31 18:08",
                    },
                    result: "请求超时，未取得供应商终态",
                    requestSummary: "幂等键已保留，敏感参数已脱敏",
                    responseSummary: "HTTP 504",
                  }}
                  actions={{
                    stage: "query-original",
                    queryOriginal: (
                      <Button size="sm" onClick={noop}>
                        查询原结果
                      </Button>
                    ),
                  }}
                />
              </div>
            </ShowcaseSection>
          </TabsContent>

          <TabsContent value="feedback" className="space-y-6">
            <ShowcaseSection
              title="空态、失败与异步区块"
              description="反馈组件区分无数据、权限范围、业务阻断和系统失败，并在刷新时保留旧内容。"
            >
              <div className="space-y-6">
                <div className="grid gap-6 xl:grid-cols-2">
                  <BusinessEmptyState
                    kind="filter"
                    action={
                      <Button variant="outline" onClick={noop}>
                        清除筛选
                      </Button>
                    }
                  />
                  <BusinessFailureState
                    kind="projection"
                    errorCode="QUERY_MODEL_STALE"
                    nextResponsible="平台运维组"
                    action={
                      <Button variant="outline" onClick={noop}>
                        查看后台任务
                      </Button>
                    }
                  />
                </div>
                <AsyncSectionState status="refreshing">
                  <div className="rounded-lg border border-border bg-card p-4 text-sm">
                    上次成功内容：当前可用库存 920 份。
                  </div>
                </AsyncSectionState>
                <div className="flex flex-wrap items-center gap-3">
                  <GuardedBusinessAction
                    disabled
                    reason="付款门禁尚未满足"
                    nextResponsible="财务结算组"
                  >
                    创建履约单
                  </GuardedBusinessAction>
                  <DraftSaveIndicator
                    state="dirty"
                    message="2 个字段尚未保存"
                  />
                  <SensitiveValue
                    maskedValue="138****8000"
                    label="客户手机号"
                    onReveal={() => Promise.resolve("13800138000")}
                  />
                </div>
                <ValidationSummary
                  issues={[
                    {
                      id: "customer-tax",
                      label: "客户税号",
                      message: "格式不完整",
                    },
                    {
                      id: "delivery-date",
                      label: "履约日期",
                      message: "不能早于订单生效日",
                    },
                  ]}
                  onLocate={noop}
                />
                <FormalActionResult
                  status="processing"
                  reference="JOB-20260731-0188"
                  facts={[
                    { label: "处理范围", value: "124 条销售订单" },
                    { label: "执行方式", value: "后台任务 · 允许部分成功" },
                  ]}
                  actions={
                    <Button variant="outline" size="sm" onClick={noop}>
                      查看任务
                    </Button>
                  }
                />
                <BackgroundJobProgress
                  mode="partialAllowed"
                  status="partial"
                  total={128}
                  completed={128}
                  succeeded={121}
                  skipped={4}
                  failed={3}
                  action={
                    <Button variant="outline" size="sm" onClick={noop}>
                      下载结果
                    </Button>
                  }
                />
              </div>
            </ShowcaseSection>

            <ShowcaseSection
              title="差异与审计"
              description="字段差异与审计事件分开表达，保留操作者、来源和发生时间。"
            >
              <div className="grid gap-6 xl:grid-cols-2">
                <BusinessDiffPanel
                  changes={[
                    {
                      id: "amount",
                      field: "含税总额",
                      before: "¥1,260,000.00",
                      after: "¥1,284,650.00",
                      note: "客户追加服务范围",
                    },
                    {
                      id: "owner",
                      field: "销售负责人",
                      before: "赵晨",
                      after: "王敏",
                      note: "责任移交",
                    },
                  ]}
                />
                <AuditTimeline
                  entries={[
                    {
                      id: "audit-01",
                      action: "销售订单生效",
                      operator: "王敏",
                      occurredAt: "2026-07-31T18:12:00+08:00",
                      occurredAtLabel: "07-31 18:12",
                      source: "ERP",
                      note: "版本 v4 生效",
                    },
                    {
                      id: "audit-02",
                      action: "接收商城订单",
                      operator: "商城同步任务",
                      occurredAt: "2026-07-31T16:02:00+08:00",
                      occurredAtLabel: "07-31 16:02",
                      source: "Mall API",
                    },
                  ]}
                />
              </div>
            </ShowcaseSection>

            <ShowcaseSection
              title="导入与批处理结果"
              description="固定阶段、问题明细与分项结果让失败范围和后续处理可追踪。"
            >
              <div className="space-y-6">
                <ImportStageIndicator
                  stages={{
                    upload: {
                      status: "complete",
                      description: "已上传 1 个文件",
                    },
                    mapping: {
                      status: "complete",
                      description: "字段映射已确认",
                    },
                    validation: {
                      status: "failed",
                      description: "发现 2 个问题",
                    },
                    preview: { status: "pending" },
                    submission: { status: "pending" },
                    result: { status: "pending" },
                  }}
                />
                <ImportIssueTable
                  issues={[
                    {
                      id: "issue-01",
                      rowNumber: 8,
                      field: "customerCode",
                      errorCode: "MAPPING_REQUIRED",
                      message: "客户编号不存在",
                      repairable: true,
                    },
                    {
                      id: "issue-02",
                      rowNumber: 16,
                      field: "amountGross",
                      errorCode: "INVALID_FORMAT",
                      message: "金额必须为十进制字符串",
                      repairable: true,
                    },
                  ]}
                />
                <BatchOperationResult
                  succeeded={[
                    {
                      id: "success-01",
                      label: "SO-20260731-0148",
                      code: "EXPORTED",
                    },
                  ]}
                  skipped={[
                    {
                      id: "skip-01",
                      label: "SO-20260730-0146",
                      detail: "当前状态不允许导出",
                    },
                  ]}
                  failed={[
                    {
                      id: "failed-01",
                      label: "SO-20260731-0147",
                      code: "PERMISSION_DENIED",
                      detail: "无客户敏感字段权限",
                    },
                  ]}
                  retryAction={
                    <Button variant="outline" size="sm" onClick={noop}>
                      仅重试失败项
                    </Button>
                  }
                />
              </div>
            </ShowcaseSection>
          </TabsContent>
        </Tabs>

        <AdvancedFilterSheet
          open={filterOpen}
          onOpenChange={setFilterOpen}
          description="静态字段仅用于验收筛选布局，不会更新 URL 或发起请求。"
          summary={<Badge variant="info">已设置 2 项条件</Badge>}
          onReset={noop}
          onApply={() => setFilterOpen(false)}
        >
          <div className="space-y-2">
            <Label>业务对象</Label>
            <BusinessObjectCombobox
              label="选择客户"
              items={[
                {
                  id: "customer-01",
                  code: "CUST-0018",
                  label: "北方能源集团",
                  status: { label: "有效", tone: "success" },
                  description: "集团客户",
                },
                {
                  id: "customer-02",
                  code: "CUST-0021",
                  label: "华东数字科技有限公司",
                  status: { label: "待复核", tone: "warning" },
                },
              ]}
              value={businessObject}
              onValueChange={setBusinessObject}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="preview-status">主状态</Label>
            <NativeSelect id="preview-status" defaultValue="effective">
              <NativeSelectOption value="effective">已生效</NativeSelectOption>
              <NativeSelectOption value="pending">待审批</NativeSelectOption>
            </NativeSelect>
          </div>
        </AdvancedFilterSheet>

        <QuickPreviewSheet
          open={previewOpen}
          onOpenChange={setPreviewOpen}
          identity="SO-20260731-0148 · 北方能源集团"
          title="销售订单快速预览"
          description="核对状态和关键事实后再进入完整详情。"
          summary={
            <BusinessStatusBadge
              context="preview"
              label="已生效"
              tone="success"
              description="版本 v4 · 2026-07-31 18:12 生效"
            />
          }
          footer={
            <Button onClick={() => setPreviewOpen(false)}>完成预览</Button>
          }
        >
          <StatusMatrix
            items={[
              { id: "main", axis: "主状态", status: "已生效", tone: "success" },
              {
                id: "fulfillment",
                axis: "履约",
                status: "部分履约",
                tone: "warning",
              },
              { id: "payment", axis: "回款", status: "待复核", tone: "info" },
            ]}
          />
          <Separator />
          <DocumentSummary
            columns="two"
            items={[
              {
                id: "amount",
                label: "含税总额",
                value: <MoneyValue value="1284650.00" taxBasis="gross" />,
                numeric: true,
              },
              { id: "owner", label: "销售负责人", value: "王敏" },
            ]}
          />
        </QuickPreviewSheet>
      </main>
    </ErpAppShell>
  )
}
