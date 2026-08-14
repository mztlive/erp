/**
 * W09 岗位通道（lane）：侧栏两个业务入口，底层同一作业引擎。
 *
 * - warehouse → 收货与发货（入库 + 公司仓发）
 * - procurement → 交付与代发（直发 + 电子 + 服务）
 *
 * lane 只决定**标题、说明和面包屑**。可见作业类型仍由角色在服务端收敛
 * （见 `fulfillment-roles.ts` 与工作面文档 §2.2）—— 这里不再放第二份类型清单，
 * 否则前端会多出一套可能与服务端对不上的可见性口径。
 */

export type FulfillmentLane = "warehouse" | "procurement"

export type FulfillmentLaneHeader = {
    /** 侧栏 / 页头标题 */
    label: string
    /** 页头说明 */
    description: string
    /** 面包屑上级分组；无归属岗位时为空 */
    group?: { label: string; href: string }
}

const FULFILLMENT_LANES: Record<
    FulfillmentLane,
    FulfillmentLaneHeader & {
        value: FulfillmentLane
        /** 侧栏 href */
        navHref: string
    }
> = {
    warehouse: {
        value: "warehouse",
        label: "收货与发货",
        description: "处理待入库和公司仓发货，连续做完再下一条。",
        group: { label: "仓储", href: "/fulfillment?lane=warehouse" },
        navHref: "/fulfillment?lane=warehouse",
    },
    procurement: {
        value: "procurement",
        label: "交付与代发",
        description: "处理供应商直发、电子交付和线下服务。",
        group: { label: "采购与履约", href: "/procurement/confirm" },
        navHref: "/fulfillment?lane=procurement",
    },
}

/**
 * 无归属岗位时的中性页头。
 *
 * 用于没声明岗位的跨页深链。
 * **不要**在这两种情况下退回「收货与发货」—— 那会在最显眼的位置，
 * 对着一张电子交付单据写「收货与发货」。中性短名与 `lib/ui-text.ts` 的 W09 一致。
 */
const FULFILLMENT_NEUTRAL_HEADER: FulfillmentLaneHeader = {
    label: "履约处理",
    description: "入库、公司仓发、供应商直发、电子交付与线下服务。",
}

function parseLaneParam(raw: string | null): FulfillmentLane | null {
    if (raw === "warehouse" || raw === "procurement") return raw
    return null
}

/**
 * 解析当前岗位通道：显式 lane > 无（中性页头）。
 *
 * 返回 null 表示「这次进来没有确定的岗位」：从别处深链而来源并不知道
 * 该落哪个岗位。此时页头走 `FULFILLMENT_NEUTRAL_HEADER`，且不把 lane
 * 写回 URL —— 写回等于替用户选择一个尚未确认的岗位。
 */
export function resolveLane(laneRaw: string | null): FulfillmentLane | null {
    return parseLaneParam(laneRaw)
}

/** 页头/面包屑用：有岗位取岗位口径，无岗位取中性口径。 */
export function laneHeader(
    lane: FulfillmentLane | null,
): FulfillmentLaneHeader {
    return lane ? FULFILLMENT_LANES[lane] : FULFILLMENT_NEUTRAL_HEADER
}
