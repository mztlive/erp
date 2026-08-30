#!/usr/bin/env node
/**
 * 审批流程定义发布脚本（开发种子 / E2E 前置，幂等）。
 *
 * 数据库 reset 会删除全部审批定义（approval_process_definitions 等），
 * 按合同（approval-workflow-contract.md §4.3/§4.4）每个 PROCESS_REQUIRED 类型
 * 必须先创建并发布定义，单据才能进入审批；否则创建返回 APPROVAL_PROCESS_NOT_CONFIGURED。
 *
 * SupplierPayment 固定为 NO_APPROVAL：采购单最终审批提供付款授权，出纳在付款任务中直接
 * 登记并过账，不得为 supplier_payment 创建或发布审批定义。
 *
 * 节点与审批人按下列来源确定（文档有明确部门时序则照文档；未指定审批人时按岗位分离
 * 与公司标准资金/库存控制设计）。提交人不得审批自己的单据（ForbidSubmitterAsApprover）。
 *
 * | 类型 | 审批链 | 来源 |
 * | --- | --- | --- |
 * | SalesOrder | 采购确认 | erp-phase-1.md §7.1 / §7.3.1：采购确认是生效闸门 |
 * | VoucherSalesOrder | 销售领导 → 运营 → 财务总监 | 二期 §16 销售领导审商务、运营审执行；财务审应收/配赠为资金内控 |
 * | SalesChangeOrder | 采购确认履约影响 → 财务复核 | erp-phase-1.md §6.5.1 |
 * | PurchaseOrder | 财务总监审批 | erp-phase-1.md §11：财务总监审核采购单 |
 * | PurchaseChangeOrder | 仓储确认库存发货影响 → 财务复核 | erp-phase-1.md §6.5.2 |
 * | StockAdjustment | 财务审批成本影响 | erp-phase-1.md §6.5.5 未指定审批人；仓储提交，财务审成本，满足岗位分离 |
 * | CustomerReceipt | 财务总监审批入账 | §6.5.4 业务部门事先确认依据，财务经办创建；总监过账审批 |
 * | CustomerRefund | 销售领导确认退款依据 → 财务总监 | §6.4 销售确认依据 + 资金流出双控 |
 * | SupplierRefund | 采购确认退款依据 → 财务总监 | §6.4 采购确认依据 + 资金双控 |
 * | ReceiptReversal | 销售领导确认冲正依据 → 财务总监 | 与客户侧资金纠错同一责任 |
 * | PaymentReversal | 采购确认冲正依据 → 财务总监 | 与供应商侧资金纠错同一责任 |
 *
 * 审批人选择约束（代码事实）：
 *   - 审批人账号必须 active 且具备 approval_instance:decide（全部业务角色都有）；
 *   - 主体读取校验按类型实现但当前均放行（organization/assignee 非空即可）；
 *   - 岗位分离：提交人不得审批自己的单据。
 *
 * 幂等策略：
 *   - 已发布 -> 跳过；
 *   - 存在草稿（上次失败残留）-> 复用该草稿继续编辑/发布；
 *   - 无草稿 -> 新建。
 *
 * 用法: node scripts/publish-approval-definitions.mjs
 * 环境变量: API_BASE（默认 http://127.0.0.1:10001）
 */
import { ADMIN, call, ensureDevAccounts, login } from "./dev-seed-lib.mjs"

const DEFINITIONS = [
    {
        type: "sales_order",
        name: "销售单审批（实物及服务）",
        nodes: [{ node_name: "采购确认", display_order: 1, assignee: "procurement" }],
    },
    {
        type: "voucher_sales_order",
        name: "卡券销售单审批",
        nodes: [
            { node_name: "销售领导审批商务条件", display_order: 1, assignee: "salesLeader" },
            { node_name: "运营确认执行可行", display_order: 2, assignee: "operations" },
            { node_name: "财务审批应收与配赠", display_order: 3, assignee: "finance" },
        ],
    },
    {
        type: "sales_change_order",
        name: "销售变更单审批",
        nodes: [
            { node_name: "采购确认履约影响", display_order: 1, assignee: "procurement" },
            { node_name: "财务复核金额与应收", display_order: 2, assignee: "finance" },
        ],
    },
    {
        type: "purchase_order",
        name: "采购单审批",
        nodes: [{ node_name: "财务总监审批", display_order: 1, assignee: "finance" }],
    },
    {
        type: "purchase_change_order",
        name: "采购变更单审批",
        nodes: [
            { node_name: "仓储确认库存发货影响", display_order: 1, assignee: "warehouse" },
            { node_name: "财务复核金额与应付", display_order: 2, assignee: "finance" },
        ],
    },
    {
        type: "stock_adjustment",
        name: "库存调整单审批",
        nodes: [{ node_name: "财务审批成本影响", display_order: 1, assignee: "finance" }],
    },
    {
        type: "customer_receipt",
        name: "客户回款单审批",
        nodes: [{ node_name: "财务总监审批入账", display_order: 1, assignee: "finance" }],
    },
    {
        type: "customer_refund",
        name: "客户退款单审批",
        nodes: [
            { node_name: "销售领导确认退款依据", display_order: 1, assignee: "salesLeader" },
            { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
        ],
    },
    {
        type: "supplier_refund",
        name: "供应商退款单审批",
        nodes: [
            { node_name: "采购确认退款依据", display_order: 1, assignee: "procurement" },
            { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
        ],
    },
    {
        type: "receipt_reversal",
        name: "回款冲正单审批",
        nodes: [
            { node_name: "销售领导确认冲正依据", display_order: 1, assignee: "salesLeader" },
            { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
        ],
    },
    {
        type: "payment_reversal",
        name: "付款冲正单审批",
        nodes: [
            { node_name: "采购确认冲正依据", display_order: 1, assignee: "procurement" },
            { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
        ],
    },
]

async function findDraftId(adminToken, documentType) {
    const versions = await call(
        "GET",
        `/admin/approval-processes/${documentType}/versions`,
        { token: adminToken },
    )
    if (!Array.isArray(versions)) return null
    const draft = versions.find((v) => v && v.status === "DRAFT")
    return draft ? draft.definition_id : null
}

/**
 * 校验脚本定义与服务端审批政策完全一致，禁止遗漏必须审批类型或配置无需审批类型。
 */
function ensureDefinitionsMatchCatalog(catalog) {
    const configuredTypes = new Set(DEFINITIONS.map((definition) => definition.type))
    if (configuredTypes.size !== DEFINITIONS.length) {
        throw new Error("审批种子存在重复单据类型")
    }

    const requiredTypes = new Set(
        catalog
            .filter((row) => row.approval_requirement === "PROCESS_REQUIRED")
            .map((row) => row.document_type),
    )
    const missingTypes = [...requiredTypes].filter((type) => !configuredTypes.has(type))
    const forbiddenTypes = [...configuredTypes].filter((type) => !requiredTypes.has(type))
    if (missingTypes.length === 0 && forbiddenTypes.length === 0) return

    const details = []
    if (missingTypes.length > 0) details.push(`缺少 ${missingTypes.join("、")}`)
    if (forbiddenTypes.length > 0) details.push(`不得配置 ${forbiddenTypes.join("、")}`)
    throw new Error(`审批种子与服务端政策不一致：${details.join("；")}`)
}

function resolveAssigneeId(userIds, assignee) {
    const userId = userIds[assignee]
    if (!userId) {
        throw new Error(`审批人 ${assignee} 未在开发账号目录中`)
    }
    return userId
}

async function main() {
    const adminToken = await login(ADMIN.account, ADMIN.password)
    console.log("admin 登录成功")

    const seeded = await ensureDevAccounts(adminToken, { checkPassword: false })
    const userIds = Object.fromEntries(Object.entries(seeded).map(([key, row]) => [key, row.id]))
    console.log("审批人账号 id:", JSON.stringify(userIds))

    const catalog = await call("GET", "/admin/approval-processes/catalog", { token: adminToken })
    ensureDefinitionsMatchCatalog(catalog)
    const byType = new Map(catalog.map((row) => [row.document_type, row]))

    let created = 0
    let skipped = 0
    for (const def of DEFINITIONS) {
        const row = byType.get(def.type)
        if (!row) {
            console.warn(`跳过: 目录中不存在类型 ${def.type}`)
            continue
        }
        if (row.configuration_status === "PUBLISHED") {
            console.log(`跳过: ${def.type} 已有已发布定义（版本 ${row.published_version}）`)
            skipped += 1
            continue
        }

        let definitionId = await findDraftId(adminToken, def.type)
        let lockVersion
        if (definitionId) {
            const detail = await call(
                "GET",
                `/admin/approval-process-definitions/${definitionId}`,
                { token: adminToken },
            )
            lockVersion = detail.definition_lock_version
            console.log(`复用草稿: ${def.type}（${definitionId}，lock=${lockVersion}）`)
        } else {
            const draft = await call("POST", "/admin/approval-process-definitions/drafts", {
                token: adminToken,
                body: {
                    document_type: def.type,
                    name: def.name,
                    draft_source: "EMPTY",
                    idempotency_key: `dev-${def.type}-${Date.now()}`,
                },
            })
            definitionId = draft.definition_id
            lockVersion = draft.definition_lock_version
            console.log(`新建草稿: ${def.type}（${definitionId}，lock=${lockVersion}）`)
        }

        const nodes = def.nodes.map((n) => ({
            node_name: n.node_name,
            display_order: n.display_order,
            assignee_user_id: resolveAssigneeId(userIds, n.assignee),
        }))
        const updated = await call(
            "PUT",
            `/admin/approval-process-definitions/${definitionId}/nodes`,
            {
                token: adminToken,
                body: { expected_definition_lock_version: String(lockVersion), nodes },
            },
        )
        lockVersion = updated.definition_lock_version

        await call(
            "POST",
            `/admin/approval-process-definitions/${definitionId}/publish`,
            {
                token: adminToken,
                body: {
                    expected_definition_lock_version: String(lockVersion),
                    idempotency_key: `dev-${def.type}-publish-${Date.now()}`,
                },
            },
        )
        console.log(`已发布: ${def.type}（${def.nodes.map((n) => n.node_name).join(" → ")}）`)
        created += 1
    }
    console.log(`完成: 新建 ${created} 个定义，跳过 ${skipped} 个已存在定义`)
}

main().catch((error) => {
    console.error("发布审批定义失败:", error.message)
    process.exit(1)
})
