import { workPage } from "./workspace-page"
import { randomUUID } from "node:crypto";

import { expect, type Page } from "./test";
import { API_BASE, apiGet, apiToken } from "./api";
import { approveCurrentDocument, openWorkspaceTask, openWorkspaceTaskRow } from "./ui";
import { openLoggedInWorkspace } from "./login";
import { uploadAcceptanceEvidence } from "./fulfillment";
import { confirmSupplyAllocation } from "./sourcing";

const SERVICE_SKU = "SVC-INSTALL-01";
const IMAGE = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=",
  "base64",
);

export type AnalyticsOrder = {
  id: string;
  order_no: string;
  commercial_status: string;
  version: number;
  current_revision_id: string;
  owner_user_id: string;
  fulfillment_progress: string;
  lines: Array<{ id: string }>;
  revisions: Array<{
    id: string;
    net_amount: string;
  }>;
};

/** 上海业务日期，与报表的自然日边界保持一致。 */
export function analyticsDate(): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
}

/** 正式 API 命令必须同时检查 HTTP、响应信封和业务结果。 */
export async function analyticsCommand<T>(
  token: string,
  endpoint: string,
  body: unknown,
): Promise<T> {
  const response = await fetch(`${API_BASE}${endpoint}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(60_000),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  const envelope = (await response.json()) as { success: boolean; data: T };
  expect(envelope.success).toBe(true);
  return envelope.data;
}

/** 真实文件资产用于无合同开单和手工费用凭证，不写库伪造附件。 */
export async function uploadAnalyticsEvidence(token: string): Promise<string> {
  const form = new FormData();
  form.append(
    "file",
    new Blob([new Uint8Array(IMAGE)], { type: "image/png" }),
    `analytics-${randomUUID()}.png`,
  );
  form.append("sensitivity_class", "sensitive");
  form.append("retention_class", "long_term");
  const response = await fetch(`${API_BASE}/admin/file-assets/upload`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
    body: form,
    signal: AbortSignal.timeout(30_000),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return ((await response.json()) as { data: { id: string } }).data.id;
}

/** 建档、提交与审批全部使用生产入口，产生两笔各 200 元不含税收入。 */
export async function prepareAnalyticsOrders(
  token: string,
  adminToken: string,
  procurementPage: Page,
  suffix: string,
): Promise<{
  customerId: string;
  customerName: string;
  evidenceId: string;
  orders: [AnalyticsOrder, AnalyticsOrder];
}> {
  const companies = await apiGet<{ items: Array<{ id: string; party_no: string; legal_name: string }> }>(adminToken, "/admin/companies", { status: "active", page_size: 100 });
  const ourCompany = companies.items.find((company) => company.party_no === "FSY" || company.legal_name === "北京福尚云科技有限公司");
  expect(ourCompany, "销售与采购必须引用已建档的我方公司主体").toBeTruthy();
  const customerName = `盈亏验收客户${suffix}`;
  const customer = await analyticsCommand<{ customer_id: string }>(
    token,
    "/admin/customer-profiles",
    {
      idempotency_key: `analytics-customer-${suffix}`,
      legal_name: customerName,
      unified_credit_code: null,
      default_payment_term_id: "POSTPAY_NET15",
      effective_from: analyticsDate(),
      change_reason: "E2E 实际经营盈亏验收",
    },
  );
  const admins = await apiGet<Array<{ id: string; account: string }>>(adminToken, "/admin/admins");
  const sales = admins.find((item) => item.account === "xiaoshou");
  expect(sales).toBeTruthy();
  const pool = await apiGet<{
    items: Array<{ sku_id: string; sku_revision_id: string; sku_no: string }>;
  }>(token, "/admin/sellable-skus", { q: SERVICE_SKU, page_size: 100 });
  const sku = pool.items.find((item) => item.sku_no === SERVICE_SKU);
  expect(sku, "固定种子应提供线下服务销售 SKU").toBeTruthy();
  const evidenceId = await uploadAnalyticsEvidence(token);
  const orders: AnalyticsOrder[] = [];
  for (const label of ["A", "B"]) {
    const created = await analyticsCommand<AnalyticsOrder>(token, "/admin/sales-orders", {
      order_no: `XS-PL-${suffix}-${label}`,
      business_type: "GOODS_SERVICE",
      contract_id: null,
      customer_id: customer.customer_id,
      idempotency_key: `analytics-order-${suffix}-${label}`,
      intent: "SUBMIT",
      evidence_file_asset_ids: [evidenceId],
      draft: {
        editor_user_id: sales!.id,
        requested_contract_revision_id: null,
        no_contract_terms: {
          settlement_party_id: ourCompany!.id,
          payment_term_code: "POSTPAY_NET15",
          payment_term_name: "货到 15 天",
          invoice_type: "增值税专用发票",
          tax_point: "0",
        },
        lines: [
          {
            line_no: 1,
            line_type: "GOODS_SERVICE",
            sales_tax_rate: "0",
            item_name_snapshot: "家电上门安装（标准台）",
            spec_snapshot: "标准台",
            unit_snapshot: "次",
            goods: {
              sku_id: sku!.sku_id,
              sku_revision_id: sku!.sku_revision_id,
              service_region: "北京",
              fulfillment_due_at: Math.floor(Date.now() / 1000) + 30 * 86400,
              quantity: "2",
              base_unit_code: "CI",
              unit_price_gross: "100.00",
              procurement_supply_price_gross: "65.00",
              procurement_price_basis: "BULK",
              pricing_mode: "MANUAL",
            },
          },
        ],
      },
    });
    await openWorkspaceTask(procurementPage, "销售单审批", created.order_no, "approval");
    await approveCurrentDocument(procurementPage);
    const effective = await apiGet<AnalyticsOrder>(token, `/admin/sales-orders/${created.id}`);
    expect(effective.commercial_status).toBe("EFFECTIVE");
    expect(
      effective.revisions.find((item) => item.id === effective.current_revision_id)?.net_amount,
    ).toBe("200.00");
    orders.push(effective);
  }
  return {
    customerId: customer.customer_id,
    customerName,
    evidenceId,
    orders: orders as [AnalyticsOrder, AnalyticsOrder],
  };
}

export type AnalyticsPurchase = { purchaseId: string; procurementNet: string; procurementGross: string; service: { id: string; version: number; purchase_order_id: string } };

/** 采购审批按本人队列中的正式任务 ID 打开。 */
export async function approveSourcedPurchase(page: Page, purchaseId: string): Promise<void> {
  const token = await apiToken("caigoushenpi");
  const matching = async () => {
    const queue = await apiGet<{ items: Array<{
      id: string; business_object_id: string; business_object_type: string; allowed_actions: string[];
    }> }>(token, "/admin/work-items", {
      scope: "mine", work_item_type: "DOCUMENT_APPROVAL", purchase_order_ids: purchaseId, page_size: 20,
    });
    return queue.items.filter((item) => item.business_object_id === purchaseId && item.business_object_type === "purchase_order");
  };
  await expect.poll(async () => (await matching()).length).toBe(1);
  const [task] = await matching();
  expect(task!.allowed_actions).toContain("APPROVE");
  const filters = encodeURIComponent(JSON.stringify({ purchase_orders: [purchaseId] }));
  await page.goto(`/workspace/tasks?filters=${filters}`);
  await openWorkspaceTaskRow(page, task!.id);
  await approveCurrentDocument(page);
}

/** 创建并通过一张真实采购，付款和履约仍由后续正式命令分别执行。 */
export async function createAnalyticsPurchase(
  order: AnalyticsOrder,
  procurementPage: Page,
  procurementToken: string,
): Promise<AnalyticsPurchase> {
  await openWorkspaceTask(procurementPage, "待安排采购", order.order_no, "procurement");
  const allocation = await confirmSupplyAllocation(procurementPage);
  expect(allocation.orders).toHaveLength(1);
  expect(allocation.stock_reservations).toHaveLength(0);
  expect(allocation.work_item_status).toBe("COMPLETED");
  const browser = procurementPage.context().browser();
  if (!browser) throw new Error("采购审批需要浏览器上下文");
  const approver = await openLoggedInWorkspace(browser, "caigoushenpi");
  try {
    await approveSourcedPurchase(approver.page, allocation.orders[0]!.purchase_order_id);
  } finally {
    await approver.context.close();
  }
  const line = order.lines[0]!;
  const services = await apiGet<{
    items: Array<{ id: string; version: number; purchase_order_id: string }>;
  }>(procurementToken, "/admin/service-fulfillments", {
    sales_order_line_id: line.id,
  });
  expect(services.items).toHaveLength(1);
  const service = services.items[0]!;
  const purchase = await apiGet<{ totals: { net: string; gross: string }; status: string }>(
    procurementToken,
    `/admin/purchase-orders/${service.purchase_order_id}`,
  );
  expect(purchase.status).toBe("EFFECTIVE");
  return { purchaseId: service.purchase_order_id, procurementNet: purchase.totals.net, procurementGross: purchase.totals.gross, service };
}

/** 完成一笔真实采购、服务及客户验收，让实际成本和覆盖证据自然产生。 */
export async function completeAnalyticsService(
  order: AnalyticsOrder,
  salesPage: Page,
  procurementPage: Page,
  _financePage: Page,
  procurementToken: string,
): Promise<{ purchaseId: string; procurementNet: string; procurementGross: string }> {
  const purchase = await createAnalyticsPurchase(order, procurementPage, procurementToken);
  const service = purchase.service;
  const form = new FormData();
  const reference = "pending-file:analytics-service";
  const now = Math.floor(Date.now() / 1000);
  form.append(
    "command",
    JSON.stringify({
      version: service.version,
      result: "SUCCESS",
      completion_note: "E2E 服务完成，验证实际经营成本归集",
      service_location: "北京市客户现场",
      service_started_at: now - 7200,
      service_ended_at: now - 3600,
      quantity: "2",
      evidence_attachment_id: reference,
    }),
  );
  form.append(
    reference,
    new Blob([new Uint8Array(IMAGE)], { type: "image/png" }),
    "analytics-service.png",
  );
  const confirmed = await fetch(`${API_BASE}/admin/service-fulfillments/${service.id}/confirm`, {
    method: "POST",
    headers: { Authorization: `Bearer ${procurementToken}` },
    body: form,
    signal: AbortSignal.timeout(60_000),
  });
  expect(confirmed.ok, await confirmed.clone().text()).toBe(true);
  await openWorkspaceTask(salesPage, "客户验收登记", order.order_no, "fulfillment");
  await workPage(salesPage).locator("#sales-orders-acceptance-register-open").click();
  await uploadAcceptanceEvidence(salesPage);
  await workPage(salesPage).locator("#sales-orders-acceptance-register-submit").click();
  const accepted = salesPage.waitForResponse(
    (item) =>
      item.request().method() === "POST" &&
      new URL(item.url()).pathname === "/admin/customer-acceptances/commit",
    { timeout: 60_000 },
  );
  await workPage(salesPage).locator("#sales-orders-acceptance-confirm-confirm").click();
  const acceptance = await accepted;
  expect(acceptance.ok(), await acceptance.text()).toBe(true);
  return {
    purchaseId: purchase.purchaseId,
    procurementNet: purchase.procurementNet,
    procurementGross: purchase.procurementGross,
  };
}
