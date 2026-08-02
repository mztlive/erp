import type { LucideIcon } from "lucide-react"
import {
  BoxesIcon,
  Building2Icon,
  ClipboardCheckIcon,
  ClipboardListIcon,
  FileStackIcon,
  FileTextIcon,
  FolderTreeIcon,
  GaugeIcon,
  HandshakeIcon,
  HistoryIcon,
  LayoutDashboardIcon,
  Link2Icon,
  ListTodoIcon,
  PackageIcon,
  PackageSearchIcon,
  PlugIcon,
  ReceiptIcon,
  ScaleIcon,
  ShieldCheckIcon,
  ShoppingBagIcon,
  ShoppingCartIcon,
  StoreIcon,
  TagsIcon,
  TicketIcon,
  TruckIcon,
  UploadIcon,
  UsersIcon,
  WalletCardsIcon,
  WarehouseIcon,
  WorkflowIcon,
} from "lucide-react"

/** 页面模式（文档用语；不在 UI 文案中展示代码）。 */
export type WorkspaceMode =
  | "M1"
  | "M2"
  | "M3"
  | "M4"
  | "M5"
  | "M6"
  | "M7"
  | "M2+M4"
  | "M2+M5"
  | "M2+M6"
  | "M3+M5"
  | "M3+M4"
  | "M2+M4+M5"
  | "nested"

export type WorkspaceId =
  | "W01"
  | "W02"
  | "W03"
  | "W04"
  | "W05"
  | "W06"
  | "W07"
  | "W08"
  | "W09"
  | "W10"
  | "W11"
  | "W12"
  | "W13"
  | "W14"
  | "W15"
  | "W16"
  | "W17"
  | "W18"
  | "W19"
  | "W20"
  | "W21"
  | "W22"
  | "W23"
  | "W24"
  | "W25"
  | "W26"
  | "W27"
  | "W28"
  | "W29"
  | "W30"

export type WorkspaceNavItem = Readonly<{
  id: WorkspaceId
  href: string
  label: string
  icon: LucideIcon
  badge?: string
  /** When true, still required as a route but not a top-level nav leaf (e.g. W06). */
  navHidden?: boolean
}>

export type WorkspaceNavGroup = Readonly<{
  label: string
  items: readonly WorkspaceNavItem[]
}>

export type WorkspaceRouteEntry = Readonly<{
  id: WorkspaceId
  name: string
  mode: WorkspaceMode
  /** Documented main route (may include path params). */
  mainRoute: string
  /** Concrete SPA path used for navigation (params resolved to defaults where needed). */
  navHref: string
  /** Whether this entry is a nested section of another workspace. */
  nestedUnder?: WorkspaceId
}>

/** Full W01–W30 index aligned with docs/ui-workspaces/README.md. */
export const WORKSPACE_ROUTES: readonly WorkspaceRouteEntry[] = [
  {
    id: "W01",
    name: "今日工作台",
    mode: "M1",
    mainRoute: "/workspace",
    navHref: "/workspace",
  },
  {
    id: "W02",
    name: "待办队列（统一）",
    mode: "M3",
    mainRoute: "/workspace/tasks",
    navHref: "/workspace/tasks",
  },
  {
    id: "W03",
    name: "客户中心",
    mode: "M4",
    mainRoute: "/sales/customers",
    navHref: "/sales/customers",
  },
  {
    id: "W04",
    name: "合同",
    mode: "M2+M4",
    mainRoute: "/sales/contracts",
    navHref: "/sales/contracts",
  },
  {
    id: "W05",
    name: "销售单（统一）",
    mode: "M2+M4+M5",
    mainRoute: "/sales/orders",
    navHref: "/sales/orders",
  },
  {
    id: "W06",
    name: "客户验收",
    mode: "nested",
    mainRoute: "/sales/orders/:salesOrderId?section=acceptance",
    navHref: "/sales/orders/so_1002?section=acceptance",
    nestedUnder: "W05",
  },
  {
    id: "W07",
    name: "二次确认队列",
    mode: "M3",
    mainRoute: "/procurement/confirm",
    navHref: "/procurement/confirm",
  },
  {
    id: "W08",
    name: "采购单",
    mode: "M2+M4+M5",
    mainRoute: "/procurement/orders",
    navHref: "/procurement/orders",
  },
  {
    id: "W09",
    name: "履约作业",
    mode: "M3+M5",
    mainRoute: "/fulfillment",
    navHref: "/fulfillment",
  },
  {
    id: "W10",
    name: "库存台账",
    mode: "M2+M6",
    mainRoute: "/inventory",
    navHref: "/inventory",
  },
  {
    id: "W11",
    name: "客户往来",
    mode: "M2+M5",
    mainRoute: "/finance/customer-accounts",
    navHref: "/finance/customer-accounts",
  },
  {
    id: "W12",
    name: "供应商往来",
    mode: "M2+M5",
    mainRoute: "/finance/supplier-accounts",
    navHref: "/finance/supplier-accounts",
  },
  {
    id: "W13",
    name: "卡券票款复核",
    mode: "M3",
    mainRoute: "/finance/card-funds-review",
    navHref: "/finance/card-funds-review",
  },
  {
    id: "W14",
    name: "可销售项目、商品、类目、供应商与仓库",
    mode: "M2+M4",
    mainRoute: "/master-data/:resource",
    navHref: "/master-data/sellable-items",
  },
  {
    id: "W15",
    name: "客户经营质量",
    mode: "M6",
    mainRoute: "/analytics/customer-quality",
    navHref: "/analytics/customer-quality",
  },
  {
    id: "W16",
    name: "实际经营盈亏",
    mode: "M6",
    mainRoute: "/analytics/profit-loss",
    navHref: "/analytics/profit-loss",
  },
  {
    id: "W17",
    name: "商城同步与映射",
    mode: "M7",
    mainRoute: "/governance/mall-sync",
    navHref: "/governance/mall-sync",
  },
  {
    id: "W18",
    name: "导入与期初",
    mode: "M7",
    mainRoute: "/governance/imports",
    navHref: "/governance/imports",
  },
  {
    id: "W19",
    name: "权限与审计",
    mode: "M2",
    mainRoute: "/system/access-audit",
    navHref: "/system/access-audit",
  },
  {
    id: "W20",
    name: "API 供应商连接",
    mode: "M2+M4",
    mainRoute: "/supplier-api/connections",
    navHref: "/supplier-api/connections",
  },
  {
    id: "W21",
    name: "商品供给管理",
    mode: "M3+M4",
    mainRoute: "/supplier-api/catalog",
    navHref: "/supplier-api/catalog",
  },
  {
    id: "W22",
    name: "商品发布",
    mode: "M2+M4",
    mainRoute: "/commerce/publications",
    navHref: "/commerce/publications",
  },
  {
    id: "W23",
    name: "执行信息",
    mode: "M2+M4",
    mainRoute: "/commerce/execution-projections",
    navHref: "/commerce/execution-projections",
  },
  {
    id: "W24",
    name: "主责迁移批次",
    mode: "M7",
    mainRoute: "/governance/ownership-migrations",
    navHref: "/governance/ownership-migrations",
  },
  {
    id: "W25",
    name: "商城消费订单",
    mode: "M2+M4",
    mainRoute: "/commerce/consumption-orders",
    navHref: "/commerce/consumption-orders",
  },
  {
    id: "W26",
    name: "供应商订单",
    mode: "M2+M4",
    mainRoute: "/supplier-api/orders",
    navHref: "/supplier-api/orders",
  },
  {
    id: "W27",
    name: "API 结算",
    mode: "M2+M4",
    mainRoute: "/supplier-api/settlements",
    navHref: "/supplier-api/settlements",
  },
  {
    id: "W28",
    name: "卡券消费台账与经营分析",
    mode: "M6",
    mainRoute: "/analytics/card-business",
    navHref: "/analytics/card-business",
  },
  {
    id: "W29",
    name: "接口错误与对账中心",
    mode: "M7",
    mainRoute: "/governance/integration-errors",
    navHref: "/governance/integration-errors",
  },
  {
    id: "W30",
    name: "历史消费回填",
    mode: "M7",
    mainRoute: "/governance/history-backfill",
    navHref: "/governance/history-backfill",
  },
] as const

/** Shell navigation groups covering every navigable W main route. */
export const WORKSPACE_NAV_GROUPS: readonly WorkspaceNavGroup[] = [
  {
    label: "工作",
    items: [
      {
        id: "W01",
        href: "/workspace",
        label: "今日工作台",
        icon: LayoutDashboardIcon,
      },
      {
        id: "W02",
        href: "/workspace/tasks",
        label: "待办队列",
        icon: ListTodoIcon,
        badge: "18",
      },
    ],
  },
  {
    label: "销售",
    items: [
      {
        id: "W03",
        href: "/sales/customers",
        label: "客户中心",
        icon: UsersIcon,
      },
      {
        id: "W04",
        href: "/sales/contracts",
        label: "合同",
        icon: FileTextIcon,
      },
      {
        id: "W05",
        href: "/sales/orders",
        label: "销售单",
        icon: ShoppingCartIcon,
      },
    ],
  },
  {
    label: "采购与履约",
    items: [
      {
        id: "W07",
        href: "/procurement/confirm",
        label: "二次确认",
        icon: ClipboardCheckIcon,
        badge: "3",
      },
      {
        id: "W08",
        href: "/procurement/orders",
        label: "采购单",
        icon: ClipboardListIcon,
      },
      {
        id: "W09",
        href: "/fulfillment",
        label: "履约作业",
        icon: TruckIcon,
      },
      {
        id: "W10",
        href: "/inventory",
        label: "库存台账",
        icon: WarehouseIcon,
      },
    ],
  },
  {
    label: "财务",
    items: [
      {
        id: "W11",
        href: "/finance/customer-accounts",
        label: "客户往来",
        icon: ReceiptIcon,
      },
      {
        id: "W12",
        href: "/finance/supplier-accounts",
        label: "供应商往来",
        icon: WalletCardsIcon,
      },
      {
        id: "W13",
        href: "/finance/card-funds-review",
        label: "卡券票款复核",
        icon: ScaleIcon,
      },
    ],
  },
  {
    label: "基础资料",
    items: [
      {
        id: "W14",
        href: "/master-data/sellable-items",
        label: "可销售项目",
        icon: BoxesIcon,
      },
      {
        id: "W14",
        href: "/master-data/products",
        label: "商品与 SKU",
        icon: PackageIcon,
      },
      {
        id: "W14",
        href: "/master-data/categories",
        label: "商品分类",
        icon: FolderTreeIcon,
      },
      {
        id: "W14",
        href: "/master-data/brands",
        label: "品牌",
        icon: TagsIcon,
      },
      {
        id: "W14",
        href: "/master-data/voucher-categories",
        label: "卡券类目",
        icon: TicketIcon,
      },
      {
        id: "W14",
        href: "/master-data/suppliers",
        label: "供应商与资质",
        icon: HandshakeIcon,
      },
      {
        id: "W14",
        href: "/master-data/warehouses",
        label: "仓库",
        icon: WarehouseIcon,
      },
    ],
  },
  {
    label: "分析",
    items: [
      {
        id: "W15",
        href: "/analytics/customer-quality",
        label: "客户经营质量",
        icon: GaugeIcon,
      },
      {
        id: "W16",
        href: "/analytics/profit-loss",
        label: "实际经营盈亏",
        icon: ScaleIcon,
      },
      {
        id: "W28",
        href: "/analytics/card-business",
        label: "卡券经营分析",
        icon: ShoppingBagIcon,
      },
    ],
  },
  {
    label: "商城与发布",
    items: [
      {
        id: "W22",
        href: "/commerce/publications",
        label: "商品发布",
        icon: StoreIcon,
      },
      {
        id: "W23",
        href: "/commerce/execution-projections",
        label: "执行信息",
        icon: WorkflowIcon,
      },
      {
        id: "W25",
        href: "/commerce/consumption-orders",
        label: "商城消费订单",
        icon: PackageIcon,
      },
    ],
  },
  {
    label: "供应商 API",
    items: [
      {
        id: "W20",
        href: "/supplier-api/connections",
        label: "API 连接",
        icon: PlugIcon,
      },
      {
        id: "W21",
        href: "/supplier-api/catalog",
        label: "商品供给",
        icon: PackageSearchIcon,
      },
      {
        id: "W26",
        href: "/supplier-api/orders",
        label: "供应商订单",
        icon: HandshakeIcon,
      },
      {
        id: "W27",
        href: "/supplier-api/settlements",
        label: "API 结算",
        icon: FileStackIcon,
      },
    ],
  },
  {
    label: "治理",
    items: [
      {
        id: "W17",
        href: "/governance/mall-sync",
        label: "商城同步与映射",
        icon: Link2Icon,
      },
      {
        id: "W18",
        href: "/governance/imports",
        label: "导入与期初",
        icon: UploadIcon,
      },
      {
        id: "W24",
        href: "/governance/ownership-migrations",
        label: "主责迁移",
        icon: Building2Icon,
      },
      {
        id: "W29",
        href: "/governance/integration-errors",
        label: "接口错误与对账",
        icon: ShieldCheckIcon,
      },
      {
        id: "W30",
        href: "/governance/history-backfill",
        label: "历史消费回填",
        icon: HistoryIcon,
      },
    ],
  },
  {
    label: "系统",
    items: [
      {
        id: "W19",
        href: "/system/access-audit",
        label: "权限与审计",
        icon: ShieldCheckIcon,
      },
    ],
  },
]

/** Flat list of every main nav href (and W06 nested path) for verification. */
export function getAllWorkspaceNavHrefs(): readonly string[] {
  const nav = WORKSPACE_NAV_GROUPS.flatMap((group) =>
    group.items.map((item) => item.href)
  )
  const nested = WORKSPACE_ROUTES.filter((route) => route.nestedUnder).map(
    (route) => route.navHref
  )
  return [...nav, ...nested]
}

export function getWorkspaceById(id: WorkspaceId): WorkspaceRouteEntry {
  const found = WORKSPACE_ROUTES.find((route) => route.id === id)
  if (!found) throw new Error(`Unknown workspace ${id}`)
  return found
}
