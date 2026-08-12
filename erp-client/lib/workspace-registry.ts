import type { LucideIcon } from "lucide-react"
import {
    BoxesIcon,
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
    RulerIcon,
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

import { hasAnyPermission } from "@/lib/permissions"

/** 页面模式（文档用语；不在 UI 文案中展示代码）。 */
type WorkspaceMode =
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

/** 侧栏角标键：由 Shell 映射到对应工作面的实时计数，无数据时隐藏。 */
export type WorkspaceNavBadgeKey =
    | "todo-count"
    | "delivery-count"
    | "warehouse-count"

type WorkspaceNavItem = Readonly<{
    id: WorkspaceId
    href: string
    label: string
    icon: LucideIcon
    badge?: WorkspaceNavBadgeKey
    /**
     * 进入该入口所需的模块权限（`resource:action`，OR）。
     * 用户有效权限能覆盖其中任一条时展示；空数组视为无门槛（仅登录态）。
     */
    requiredPermissions: readonly string[]
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
    badge?: WorkspaceNavBadgeKey
    href?: string
    label?: string
    /**
     * 覆盖默认模块权限。默认见 `defaultNavPermissionsFor`。
     * 多资源工作面（如 W14）按具体 href 传入。
     */
    requiredPermissions?: readonly string[]
}>

type WorkspaceNavGroupSpec = Readonly<{
    label: string
    items: readonly WorkspaceNavItemSpec[]
}>

/**
 * 工作面默认入口权限（对齐各域 list/入口动作）。
 * 与预定义角色种子、handler `#[permission(...)]` 一致；前端仅做展示裁剪，鉴权以服务端为准。
 */
function defaultNavPermissionsFor(
    routeId: WorkspaceId,
    href: string,
): readonly string[] {
    switch (routeId) {
        case "W01":
        case "W02":
            return ["work_item:list"]
        case "W03":
            return ["customer:list"]
        case "W04":
            return ["contract:list"]
        case "W05":
        case "W06":
            return ["sales_order:list"]
        case "W07":
            return ["procurement_confirmation:list"]
        case "W08":
            return ["purchase_order:list"]
        case "W09":
            // 仓储 lane 偏收货；采购 lane 偏交付/代发。任一入口动作即可显示对应菜单。
            if (href.includes("lane=warehouse")) {
                return ["purchase_receipt:list", "delivery:list"]
            }
            return [
                "delivery:list",
                "electronic_delivery:list",
                "service_fulfillment:list",
                "purchase_receipt:list",
            ]
        case "W10":
            return ["stock_balance:list"]
        case "W11":
            return ["receivable_account:list"]
        case "W12":
            return ["payable_account:list"]
        case "W13":
            return ["receivable_funds_review:create", "receivable_account:list"]
        case "W14":
            return masterDataPermissionsForHref(href)
        case "W15":
            return ["customer:list"]
        case "W16":
            return ["cost_entry:list", "sales_order:list"]
        case "W17":
            return [
                "mall_sales_sync_job:list",
                "master_mapping_task:list",
                "source_system:list",
            ]
        case "W18":
            return ["legacy_import_batch:list"]
        case "W19":
            return [
                "role:list",
                "audit_log:list",
                "permission:list",
                "admin:list",
            ]
        case "W20":
            return ["supplier_api_connection:list"]
        case "W21":
            return ["supplier_offering:list"]
        case "W22":
            return ["product_publication:list"]
        case "W23":
            return ["sales_order_projection:list"]
        case "W25":
            return ["mall_order:list"]
        case "W26":
            return ["supplier_fulfillment_order:list"]
        case "W27":
            return ["supplier_settlement_statement:list"]
        case "W28":
            return ["mall_order:list", "mall_card_instance:list"]
        case "W29":
            return [
                "integration_error_task:list",
                "reconciliation_difference:list",
            ]
        case "W30":
            return ["mall_consumption_backfill_job:list"]
        default:
            return []
    }
}

/** W14 按资源路径映射入口 list 权限。 */
function masterDataPermissionsForHref(href: string): readonly string[] {
    if (href.includes("/master-data/sellable-items")) {
        return ["sellable_sku:list"]
    }
    if (href.includes("/master-data/categories"))
        return ["product_category:list"]
    if (href.includes("/master-data/brands")) return ["product_brand:list"]
    if (href.includes("/master-data/unit-of-measures")) {
        return ["unit_of_measure:list"]
    }
    if (href.includes("/master-data/voucher-categories")) {
        return ["voucher_category_profile:list"]
    }
    if (href.includes("/master-data/suppliers")) return ["supplier:list"]
    if (href.includes("/master-data/warehouses")) return ["warehouse:list"]
    // 商品与 SKU
    return ["product:list", "sku:list"]
}

function buildWorkspaceNavGroups(
    groups: readonly WorkspaceNavGroupSpec[],
): readonly WorkspaceNavGroup[] {
    return groups.map((group) => ({
        label: group.label,
        items: group.items.map((spec) => {
            const route = getWorkspaceById(spec.routeId)
            const href = spec.href ?? route.navHref
            return {
                id: spec.routeId,
                href,
                label: spec.label ?? route.name,
                icon: spec.icon,
                requiredPermissions:
                    spec.requiredPermissions ??
                    defaultNavPermissionsFor(spec.routeId, href),
                ...(spec.badge ? { badge: spec.badge } : {}),
            }
        }),
    }))
}

/**
 * 按用户有效权限裁剪侧栏分组。
 * - 无模块权限：不展示入口（erp-ui-design §3.3）
 * - 分组内无可见项时整组隐藏
 * - 权限尚未加载时返回空（fail-closed，避免先闪后藏）
 */
export function filterNavGroupsByPermissions(
    groups: readonly WorkspaceNavGroup[],
    permissions: readonly string[] | undefined | null,
): WorkspaceNavGroup[] {
    if (!permissions) return []
    return groups
        .map((group) => ({
            label: group.label,
            items: group.items.filter((item) =>
                hasAnyPermission(permissions, item.requiredPermissions),
            ),
        }))
        .filter((group) => group.items.length > 0)
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
        name: "供应商供给",
        mode: "M2+M4",
        mainRoute: "/procurement/supplier-offerings",
        navHref: "/procurement/supplier-offerings",
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
                    label: "待我处理",
                    icon: ListTodoIcon,
                    badge: "todo-count",
                    // 并入了 W07 的入口（见下方"采购与履约"分组说明）：采购角色可能只有
                    // procurement_confirmation:list 而没有 work_item:list，任一权限满足
                    // 即显示，避免降级后采购看不到这个入口。
                    requiredPermissions: [
                        "work_item:list",
                        "procurement_confirmation:list",
                    ],
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
                    routeId: "W08",
                    icon: ClipboardListIcon,
                },
                {
                    routeId: "W09",
                    href: "/fulfillment?lane=procurement",
                    label: "交付与代发",
                    icon: TruckIcon,
                    // 采购岗位「仅我的」待处理数：由 Shell 从 W09 队列计数实时计算。
                    badge: "delivery-count",
                },
                {
                    routeId: "W21",
                    label: "供应商供给",
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
                    // 仓储岗位「仅我的」待处理数：由 Shell 从 W09 队列计数实时计算。
                    badge: "warehouse-count",
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
                    label: "可售商品池",
                    icon: BoxesIcon,
                },
                {
                    routeId: "W14",
                    href: "/master-data/products",
                    label: "商品列表",
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
                    href: "/master-data/unit-of-measures",
                    label: "计量单位",
                    icon: RulerIcon,
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

export function getWorkspaceById(id: WorkspaceId): WorkspaceRouteEntry {
    const found = WORKSPACE_ROUTES.find((route) => route.id === id)
    if (!found) throw new Error(`Unknown workspace ${id}`)
    return found
}
