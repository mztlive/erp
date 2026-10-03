import { randomUUID } from "node:crypto";

import { expect, type Page } from "./test";
import { API_BASE, apiGet } from "./api";
import { approveCurrentDocument, openWorkspaceTask } from "./ui";
import { expandSourcingEditor } from "./sourcing";
import { uploadAcceptanceEvidence } from "./fulfillment";

const VISIBLE = { timeout: 20_000 };
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

/** 完成一笔真实采购、服务及客户验收，让实际成本和覆盖证据自然产生。 */
export async function completeAnalyticsService(
  order: AnalyticsOrder,
  salesPage: Page,
  procurementPage: Page,
  financePage: Page,
  procurementToken: string,
): Promise<{ purchaseId: string; procurementNet: string; procurementGross: string }> {
  await openWorkspaceTask(procurementPage, "待供给分配", order.order_no, "procurement");
  await expandSourcingEditor(procurementPage, "家电上门安装");
  await procurementPage.locator("#procurement-orders-create-preview").click();
  const submitted = procurementPage.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname === "/admin/purchase-orders/from-sourcing",
  );
  await procurementPage
    .locator("#procurement-orders-create-preview-confirm")
    .click({ force: true });
  const response = await submitted;
  expect(response.ok(), await response.text()).toBe(true);
  await expect(procurementPage.getByText("已创建 1 张采购单并提交审批。")).toBeVisible(VISIBLE);
  await openWorkspaceTask(financePage, "采购单审批", order.order_no, "approval");
  await approveCurrentDocument(financePage);
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
  await salesPage.locator("#sales-orders-acceptance-register-open").click();
  await uploadAcceptanceEvidence(salesPage);
  await salesPage.locator("#sales-orders-acceptance-register-submit").click();
  const accepted = salesPage.waitForResponse(
    (item) =>
      item.request().method() === "POST" &&
      new URL(item.url()).pathname === "/admin/customer-acceptances/commit",
    { timeout: 60_000 },
  );
  await salesPage.locator("#sales-orders-acceptance-confirm-confirm").click();
  const acceptance = await accepted;
  expect(acceptance.ok(), await acceptance.text()).toBe(true);
  return {
    purchaseId: service.purchase_order_id,
    procurementNet: purchase.totals.net,
    procurementGross: purchase.totals.gross,
  };
}
