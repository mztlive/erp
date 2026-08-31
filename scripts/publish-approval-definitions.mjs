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
 * 节点与审批人按下列来源确定（文档有部门时序则照文档；未指定审批人时按岗位分离
 * 与资金/库存内控）。提交人不得审批自己的单据（ForbidSubmitterAsApprover）。
 * 财务三人共用 role-finance，因此资金单必须由出纳 fukuan 提交、总监 caiwu 只审批；
 * 总监自己提交会在运行时失败。本脚本在发布前后校验约定提交人不在任何审批节点。
 *
 * | 类型 | 提交人 | 审批链 | 来源 |
 * | --- | --- | --- | --- |
 * | SalesOrder | xiaoshou | 采购确认 | erp-phase-1.md §7.1 / §7.3.1 |
 * | VoucherSalesOrder | xiaoshou | 销售领导 → 运营 → 财务总监 | 二期 §16；一期商城开单不走此链，但类型必须有已发布定义 |
 * | SalesChangeOrder | xiaoshou | 采购确认履约影响 → 财务复核 | erp-phase-1.md §6.5.1 |
 * | PurchaseOrder | caigou | 财务总监审批 | erp-phase-1.md §11 |
 * | PurchaseChangeOrder | caigou | 仓储确认库存发货影响 → 财务复核 | erp-phase-1.md §6.5.2 |
 * | StockAdjustment | cangchu | 财务审批成本影响 | erp-phase-1.md §6.5.5；仓储提交、财务审成本 |
 * | CustomerReceipt | fukuan | 财务总监审批入账 | erp-phase-1.md §9.1；出纳经办、总监审批 |
 * | CustomerRefund | fukuan | 销售领导确认退款依据 → 财务总监 | §6.4 销售确认依据 + 资金双控 |
 * | SupplierRefund | fukuan | 采购确认退款依据 → 财务总监 | §6.4 采购确认依据 + 资金双控 |
 * | ReceiptReversal | fukuan | 销售领导确认冲正依据 → 财务总监 | 与客户侧资金纠错同一责任 |
 * | PaymentReversal | fukuan | 采购确认冲正依据 → 财务总监 | 与供应商侧资金纠错同一责任 |
 *
 * 审批人选择约束（代码事实）：
 *   - 审批人账号必须 active 且具备 approval_instance:decide（全部业务角色都有）；
 *   - 主体读取校验按类型实现但当前均放行（organization/assignee 非空即可）；
 *   - 岗位分离：提交人不得审批自己的单据；发布期只校验节点间约定，提交人隔离在运行时生效。
 *
 * 幂等策略：
 *   - 已发布 -> 核验节点审批人与约定提交人后跳过；不一致则失败，需 reset 后重发；
 *   - 存在草稿（上次失败残留）-> 复用该草稿继续编辑/发布；
 *   - 无草稿 -> 新建。
 *
 * 用法: node scripts/publish-approval-definitions.mjs
 * 环境变量: API_BASE（默认 http://127.0.0.1:10001）
 */
import { ACCOUNTS, ADMIN, call, ensureDevAccounts, login } from "./dev-seed-lib.mjs";

const DEFINITIONS = [
  {
    type: "sales_order",
    name: "销售单审批（实物及服务）",
    submitter: "sales",
    nodes: [{ node_name: "采购确认", display_order: 1, assignee: "procurement" }],
  },
  {
    type: "voucher_sales_order",
    name: "卡券销售单审批",
    submitter: "sales",
    nodes: [
      { node_name: "销售领导审批商务条件", display_order: 1, assignee: "salesLeader" },
      { node_name: "运营确认执行可行", display_order: 2, assignee: "operations" },
      { node_name: "财务审批应收与配赠", display_order: 3, assignee: "finance" },
    ],
  },
  {
    type: "sales_change_order",
    name: "销售变更单审批",
    submitter: "sales",
    nodes: [
      { node_name: "采购确认履约影响", display_order: 1, assignee: "procurement" },
      { node_name: "财务复核金额与应收", display_order: 2, assignee: "finance" },
    ],
  },
  {
    type: "purchase_order",
    name: "采购单审批",
    submitter: "procurement",
    nodes: [{ node_name: "财务总监审批", display_order: 1, assignee: "finance" }],
  },
  {
    type: "purchase_change_order",
    name: "采购变更单审批",
    submitter: "procurement",
    nodes: [
      { node_name: "仓储确认库存发货影响", display_order: 1, assignee: "warehouse" },
      { node_name: "财务复核金额与应付", display_order: 2, assignee: "finance" },
    ],
  },
  {
    type: "stock_adjustment",
    name: "库存调整单审批",
    submitter: "warehouse",
    nodes: [{ node_name: "财务审批成本影响", display_order: 1, assignee: "finance" }],
  },
  {
    type: "customer_receipt",
    name: "客户回款单审批",
    submitter: "payment",
    nodes: [{ node_name: "财务总监审批入账", display_order: 1, assignee: "finance" }],
  },
  {
    type: "customer_refund",
    name: "客户退款单审批",
    submitter: "payment",
    nodes: [
      { node_name: "销售领导确认退款依据", display_order: 1, assignee: "salesLeader" },
      { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
    ],
  },
  {
    type: "supplier_refund",
    name: "供应商退款单审批",
    submitter: "payment",
    nodes: [
      { node_name: "采购确认退款依据", display_order: 1, assignee: "procurement" },
      { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
    ],
  },
  {
    type: "receipt_reversal",
    name: "回款冲正单审批",
    submitter: "payment",
    nodes: [
      { node_name: "销售领导确认冲正依据", display_order: 1, assignee: "salesLeader" },
      { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
    ],
  },
  {
    type: "payment_reversal",
    name: "付款冲正单审批",
    submitter: "payment",
    nodes: [
      { node_name: "采购确认冲正依据", display_order: 1, assignee: "procurement" },
      { node_name: "财务总监审批", display_order: 2, assignee: "finance" },
    ],
  },
];

async function findVersionId(adminToken, documentType, status) {
  const versions = await call("GET", `/admin/approval-processes/${documentType}/versions`, {
    token: adminToken,
  });
  if (!Array.isArray(versions)) return null;
  const match = versions.find((version) => version && version.status === status);
  return match ? match.definition_id : null;
}

/**
 * 校验脚本定义与服务端审批政策完全一致，禁止遗漏必须审批类型或配置无需审批类型。
 */
function ensureDefinitionsMatchCatalog(catalog) {
  const configuredTypes = new Set(DEFINITIONS.map((definition) => definition.type));
  if (configuredTypes.size !== DEFINITIONS.length) {
    throw new Error("审批种子存在重复单据类型");
  }

  const requiredTypes = new Set(
    catalog
      .filter((row) => row.approval_requirement === "PROCESS_REQUIRED")
      .map((row) => row.document_type),
  );
  const missingTypes = [...requiredTypes].filter((type) => !configuredTypes.has(type));
  const forbiddenTypes = [...configuredTypes].filter((type) => !requiredTypes.has(type));
  if (missingTypes.length === 0 && forbiddenTypes.length === 0) return;

  const details = [];
  if (missingTypes.length > 0) details.push(`缺少 ${missingTypes.join("、")}`);
  if (forbiddenTypes.length > 0) details.push(`不得配置 ${forbiddenTypes.join("、")}`);
  throw new Error(`审批种子与服务端政策不一致：${details.join("；")}`);
}

function accountLabel(key) {
  const spec = ACCOUNTS[key];
  if (!spec) {
    throw new Error(`开发账号目录中不存在 ${key}`);
  }
  return `${spec.account}（${spec.label}）`;
}

function describeDefinition(definition) {
  const chain = definition.nodes
    .map((node) => `${node.node_name}(${ACCOUNTS[node.assignee].account})`)
    .join(" → ");
  return `提交 ${ACCOUNTS[definition.submitter].account} → ${chain}`;
}

/**
 * 约定提交人不得出现在任何审批节点，否则运行时 ForbidSubmitterAsApprover 会拒绝提交。
 */
function ensureSubmitterSeparation(definition) {
  if (!definition.submitter) {
    throw new Error(`${definition.type} 未约定提交人`);
  }
  if (!ACCOUNTS[definition.submitter]) {
    throw new Error(`${definition.type} 提交人 ${definition.submitter} 不在开发账号目录`);
  }
  const collision = definition.nodes.find((node) => node.assignee === definition.submitter);
  if (collision) {
    throw new Error(
      `${definition.type} 约定提交人 ${accountLabel(definition.submitter)} 担任节点「${collision.node_name}」，违反 ForbidSubmitterAsApprover`,
    );
  }
}

function ensureAllSubmitterSeparation() {
  for (const definition of DEFINITIONS) {
    ensureSubmitterSeparation(definition);
  }
}

function resolveAssigneeId(userIds, assignee) {
  const userId = userIds[assignee];
  if (!userId) {
    throw new Error(`审批人 ${assignee} 未在开发账号目录中`);
  }
  return userId;
}

/**
 * 核对已发布节点的审批人与种子一致，且不含约定提交人。
 */
async function assertPublishedNodesMatchSeed(adminToken, definition, userIds) {
  const definitionId = await findVersionId(adminToken, definition.type, "PUBLISHED");
  if (!definitionId) {
    throw new Error(`${definition.type} 未找到已发布版本，无法核验岗位分离`);
  }
  const detail = await call("GET", `/admin/approval-process-definitions/${definitionId}`, {
    token: adminToken,
  });
  const liveNodes = Array.isArray(detail.nodes) ? [...detail.nodes] : [];
  liveNodes.sort((left, right) => (left.display_order ?? 0) - (right.display_order ?? 0));
  if (liveNodes.length !== definition.nodes.length) {
    throw new Error(
      `${definition.type} 已发布节点数 ${liveNodes.length} 与种子 ${definition.nodes.length} 不一致`,
    );
  }
  const submitterId = resolveAssigneeId(userIds, definition.submitter);
  for (let index = 0; index < definition.nodes.length; index += 1) {
    const expected = definition.nodes[index];
    const live = liveNodes[index];
    const expectedId = resolveAssigneeId(userIds, expected.assignee);
    if (live.assignee_user_id !== expectedId) {
      throw new Error(
        `${definition.type} 第 ${index + 1} 节点审批人与种子不一致：期望 ${accountLabel(expected.assignee)}`,
      );
    }
    if (live.assignee_user_id === submitterId) {
      throw new Error(
        `${definition.type} 已发布节点含约定提交人 ${accountLabel(definition.submitter)}，岗位分离不成立`,
      );
    }
  }
}

async function main() {
  const adminToken = await login(ADMIN.account, ADMIN.password);
  console.log("admin 登录成功");

  const seeded = await ensureDevAccounts(adminToken, { checkPassword: false });
  const userIds = Object.fromEntries(Object.entries(seeded).map(([key, row]) => [key, row.id]));
  console.log("审批人账号 id:", JSON.stringify(userIds));

  const catalog = await call("GET", "/admin/approval-processes/catalog", { token: adminToken });
  ensureDefinitionsMatchCatalog(catalog);
  ensureAllSubmitterSeparation();
  const byType = new Map(catalog.map((row) => [row.document_type, row]));

  let created = 0;
  let skipped = 0;
  for (const def of DEFINITIONS) {
    const row = byType.get(def.type);
    if (!row) {
      console.warn(`跳过: 目录中不存在类型 ${def.type}`);
      continue;
    }
    if (row.configuration_status === "PUBLISHED") {
      await assertPublishedNodesMatchSeed(adminToken, def, userIds);
      console.log(
        `跳过: ${def.type} 已有已发布定义（版本 ${row.published_version}），已核验 ${describeDefinition(def)}`,
      );
      skipped += 1;
      continue;
    }

    let definitionId = await findVersionId(adminToken, def.type, "DRAFT");
    let lockVersion;
    if (definitionId) {
      const detail = await call("GET", `/admin/approval-process-definitions/${definitionId}`, {
        token: adminToken,
      });
      lockVersion = detail.definition_lock_version;
      console.log(`复用草稿: ${def.type}（${definitionId}，lock=${lockVersion}）`);
    } else {
      const draft = await call("POST", "/admin/approval-process-definitions/drafts", {
        token: adminToken,
        body: {
          document_type: def.type,
          name: def.name,
          draft_source: "EMPTY",
          idempotency_key: `dev-${def.type}-${Date.now()}`,
        },
      });
      definitionId = draft.definition_id;
      lockVersion = draft.definition_lock_version;
      console.log(`新建草稿: ${def.type}（${definitionId}，lock=${lockVersion}）`);
    }

    const nodes = def.nodes.map((n) => ({
      node_name: n.node_name,
      display_order: n.display_order,
      assignee_user_id: resolveAssigneeId(userIds, n.assignee),
    }));
    const updated = await call("PUT", `/admin/approval-process-definitions/${definitionId}/nodes`, {
      token: adminToken,
      body: { expected_definition_lock_version: String(lockVersion), nodes },
    });
    lockVersion = updated.definition_lock_version;

    await call("POST", `/admin/approval-process-definitions/${definitionId}/publish`, {
      token: adminToken,
      body: {
        expected_definition_lock_version: String(lockVersion),
        idempotency_key: `dev-${def.type}-publish-${Date.now()}`,
      },
    });
    await assertPublishedNodesMatchSeed(adminToken, def, userIds);
    console.log(`已发布: ${def.type}（${describeDefinition(def)}）`);
    created += 1;
  }
  console.log(`完成: 新建 ${created} 个定义，跳过 ${skipped} 个已存在定义`);
  console.log("岗位分离约定（caiwu 不得提交自己要审的资金单）:");
  for (const definition of DEFINITIONS) {
    console.log(`  ${definition.type}: ${describeDefinition(definition)}`);
  }
}

main().catch((error) => {
  console.error("发布审批定义失败:", error.message);
  process.exit(1);
});
