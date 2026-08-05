import type { LucideIcon } from "lucide-react"
import {
  BoxesIcon,
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
  | "M2+M3+M4"
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

/** 侧栏导航项定义：仅存 route 引用与导航特有字段，id/默认名称/默认 href 派生自 WORKSPACE_ROUTES。 */
type WorkspaceNavItemSpec = Readonly<{
  routeId: WorkspaceId
  icon: LucideIcon
  badge?: string
  href?: string
  label?: string
}>

type WorkspaceNavGroupSpec = Readonly<{
  label: string
  items: readonly WorkspaceNavItemSpec[]
}>

function buildWorkspaceNavGroups(
  groups: readonly WorkspaceNavGroupSpec[]
): readonly WorkspaceNavGroup[] {
  return groups.map((group) => ({
    label: group.label,
    items: group.items.map((spec) => {
      const route = getWorkspaceById(spec.routeId)
      return {
        id: spec.routeId,
        href: spec.href ?? route.navHref,
        label: spec.label ?? route.name,
        icon: spec.icon,
        ...(spec.badge ? { badge: spec.badge } : {}),
      }
    }),
  }))
}

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
    name: "收货与发货 / 交付与代发",
    mode: "M3+M5",
    mainRoute: "/fulfillment",
    /** 双入口之一；侧栏另有 procurement lane，见 WORKSPACE_NAV_GROUPS */
    navHref: "/fulfillment?lane=warehouse",
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
    name: "公司商品池、商品、类目、供应商与仓库",
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
    name: "供应商商品库与供给管理",
    mode: "M2+M3+M4",
    mainRoute: "/procurement/supplier-catalog",
    navHref: "/procurement/supplier-catalog",
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
export const WORKSPACE_NAV_GROUPS: readonly WorkspaceNavGroup[] =
  buildWorkspaceNavGroups([
  {
    label: "工作",
    items: [
      {
        routeId: "W01",
        icon: LayoutDashboardIcon,
      },
      {
        routeId: "W02",
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
        routeId: "W03",
        icon: UsersIcon,
      },
      {
        routeId: "W04",
        icon: FileTextIcon,
      },
      {
        routeId: "W05",
        label: "销售单",
        icon: ShoppingCartIcon,
      },
    ],
  },
  {
    label: "采购与履约",
    items: [
      {
        routeId: "W07",
        label: "二次确认",
        icon: ClipboardCheckIcon,
        badge: "3",
      },
      {
        routeId: "W08",
        icon: ClipboardListIcon,
      },
      {
        routeId: "W09",
        href: "/fulfillment?lane=procurement",
        label: "交付与代发",
        icon: TruckIcon,
        // 采购 · 李采「仅我的」待处理数（mock/fulfillment-operations.ts 固定夹具）。
        // 接真实队列后改为实时值，见 features/fulfillment-operations/queries.ts。
        badge: "3",
      },
      {
        routeId: "W21",
        label: "供应商商品库",
        icon: PackageSearchIcon,
      },
    ],
  },
  {
    label: "仓储",
    items: [
      {
        routeId: "W09",
        label: "收货与发货",
        icon: PackageIcon,
        // 仓储 · 周航「仅我的」待处理数（mock/fulfillment-operations.ts 固定夹具）。
        // 一线打开侧栏就要看到「我有几件活」；接真实队列后改为实时值。
        badge: "4",
      },
      {
        routeId: "W10",
        icon: WarehouseIcon,
      },
    ],
  },
  {
    label: "财务",
    items: [
      {
        routeId: "W11",
        icon: ReceiptIcon,
      },
      {
        routeId: "W12",
        icon: WalletCardsIcon,
      },
      {
        routeId: "W13",
        icon: ScaleIcon,
      },
    ],
  },
  {
    label: "基础资料",
    items: [
      {
        routeId: "W14",
        href: "/master-data/sellable-items",
        label: "公司商品池",
        icon: BoxesIcon,
      },
      {
        routeId: "W14",
        href: "/master-data/products",
        label: "商品与 SKU",
        icon: PackageIcon,
      },
      {
        routeId: "W14",
        href: "/master-data/categories",
        label: "商品分类",
        icon: FolderTreeIcon,
      },
      {
        routeId: "W14",
        href: "/master-data/brands",
        label: "品牌",
        icon: TagsIcon,
      },
      {
        routeId: "W14",
        href: "/master-data/voucher-categories",
        label: "卡券类目",
        icon: TicketIcon,
      },
      {
        routeId: "W14",
        href: "/master-data/suppliers",
        label: "供应商与资质",
        icon: HandshakeIcon,
      },
      {
        routeId: "W14",
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
        routeId: "W15",
        icon: GaugeIcon,
      },
      {
        routeId: "W16",
        icon: ScaleIcon,
      },
      {
        routeId: "W28",
        label: "卡券经营分析",
        icon: ShoppingBagIcon,
      },
    ],
  },
  {
    label: "商城与发布",
    items: [
      {
        routeId: "W22",
        icon: StoreIcon,
      },
      {
        routeId: "W23",
        icon: WorkflowIcon,
      },
      {
        routeId: "W25",
        icon: PackageIcon,
      },
    ],
  },
  {
    label: "供应商 API",
    items: [
      {
        routeId: "W20",
        label: "API 连接",
        icon: PlugIcon,
      },
      {
        routeId: "W26",
        icon: HandshakeIcon,
      },
      {
        routeId: "W27",
        icon: FileStackIcon,
      },
    ],
  },
  {
    label: "治理",
    items: [
      {
        routeId: "W17",
        icon: Link2Icon,
      },
      {
        routeId: "W18",
        icon: UploadIcon,
      },
      {
        routeId: "W29",
        label: "接口错误与对账",
        icon: ShieldCheckIcon,
      },
      {
        routeId: "W30",
        icon: HistoryIcon,
      },
    ],
  },
  {
    label: "系统",
    items: [
      {
        routeId: "W19",
        icon: ShieldCheckIcon,
      },
    ],
  },
])

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
