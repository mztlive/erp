/**
 * [flow-23] 真实成本分配、实际经营盈亏与筛选 CSV 下载。
 * 合同：docs/erp-phase-1.md §7.5、§9.4；预计/确认成本不进入实际利润，未覆盖不得作零成本。
 * 销售/采购/履约事实通过生产命令形成，报表、成本下钻及下载通过真实浏览器验证。
 * W16 当前只提供只读成本详情；手工费用使用 POST /cost-entries 登记。
 */
import { readFile } from "node:fs/promises";

import { expect, test, type Locator, type Page } from "../helpers/test";
import { API_BASE, apiGet, apiToken } from "../helpers/api";
import {
  analyticsCommand,
  analyticsDate,
  completeAnalyticsService,
  prepareAnalyticsOrders,
  type AnalyticsOrder,
} from "../helpers/financial-analytics";
import { openLoggedInWorkspace } from "../helpers/login";
import { ensureDefaultProcurementOwner } from "../helpers/procurement";
import { workPage } from "../helpers/workspace-page";

const VISIBLE = { timeout: 20_000 };
type Amounts = {
  netSalesRevenue: string;
  actualProcurementCostNet: string;
  actualFulfillmentCostNet: string;
  reductionsNet: string;
  actualProfitLossNet?: string;
};
type Report = {
  scopeVersion: string;
  formulaVersion: string;
  freshness: { projectedAt: string };
  coverage: { coveredNetRevenue: string; uncoveredNetRevenue: string; coverageRate: string };
  totals: Amounts;
  stageReference: Array<{ stage: string; totalNet: string }>;
  rows: {
    total: number;
    items: Array<
      Amounts & {
        objectId: string;
        identityLabel: string;
        coverageState: string;
        coverageBlockers: Array<{ code: string }>;
        costEntryIds: string[];
      }
    >;
  };
};
type Cost = {
  id: string;
  cost_stage: string;
  cost_type: string;
  net_amount: string;
  source_fact_type: string;
  source_document_id: string;
  allocations: Array<{ sales_order_id: string; allocated_net_amount: string }>;
};

/** 报表预期只使用整数分计算，避免测试把金额浮点误差当业务事实。 */
function subtractMoney(left: string, right: string): string {
  const cents = (value: string) => BigInt(value.replace(".", ""));
  const result = cents(left) - cents(right);
  const sign = result < 0n ? "-" : "";
  const value = result < 0n ? -result : result;
  return `${sign}${value / 100n}.${String(value % 100n).padStart(2, "0")}`;
}

function reportQuery(customerId: string, overrides: Record<string, unknown> = {}) {
  return {
    from: analyticsDate(),
    to: analyticsDate(),
    period_basis: "sales_order_effective_date",
    scope_id: "authorized",
    customer_id: customerId,
    coverage: "all",
    dimension: "sales_order",
    page: 1,
    page_size: 20,
    sort: "identityLabel:asc",
    ...overrides,
  };
}

function expense(
  order: AnalyticsOrder,
  evidenceId: string,
  key: string,
  stage: string,
  net: string,
) {
  return {
    cost_type: "delivery",
    cost_stage: stage,
    cost_scope: "non_voucher_fulfillment",
    gross_amount: net,
    net_amount: net,
    tax_amount: "0.00",
    tax_inclusion: true,
    input_tax_rate: "0.00",
    occurred_at: Math.floor(Date.now() / 1000) - 1,
    source_fact_type: "E2E_FINANCIAL_EXPENSE",
    source_document_id: key,
    source_line_id: "1",
    source_version: "1",
    evidence_attachment_id: evidenceId,
    allocations: [
      {
        sales_order_id: order.id,
        sales_order_line_id: null,
        allocated_gross_amount: net,
        allocated_net_amount: net,
      },
    ],
  };
}

async function uiReport(
  page: Page,
  customerId: string,
  overrides: Record<string, string> = {},
): Promise<Report> {
  const params = new URLSearchParams({
    from: analyticsDate(),
    to: analyticsDate(),
    periodBasis: "sales_order_effective_date",
    customerId,
    coverage: "all",
    sort: "identityLabel:asc",
    ...overrides,
  });
  const response = page.waitForResponse(
    (item) =>
      item.request().method() === "GET" &&
      new URL(item.url()).pathname === "/admin/actual-profit-loss",
  );
  await page.goto(`/analytics/profit-loss?${params}`);
  const result = await response;
  expect(result.ok(), await result.text()).toBe(true);
  await expect(workPage(page).getByRole("heading", { name: "实际经营盈亏", exact: true })).toBeVisible(
    VISIBLE,
  );
  return ((await result.json()) as { data: Report }).data;
}

async function failedCommand(token: string, endpoint: string, body: unknown, statuses: number[]) {
  const response = await fetch(`${API_BASE}${endpoint}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(30_000),
  });
  const result = (await response.json()) as {
    success: boolean;
    code?: string;
    errorMessage?: string;
  };
  expect(statuses, result.errorMessage).toContain(response.status);
  expect(result.success).toBe(false);
  return result;
}

async function downloadCsvTransfer(
  page: Page,
  button: Locator,
  kind: "profit_loss" | "receivables" | "supplier_payables",
) {
  const job = page
    .waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === "/account/data-transfers" &&
        response.request().postDataJSON().kind === kind,
      { timeout: 90_000 },
    )
    .then(async (response) => {
      expect(response.ok(), await response.text()).toBe(true);
      const { data } = (await response.json()) as { data: { id: string } };
      expect(data.id).toBeTruthy();
      return { id: data.id, input: response.request().postDataJSON().input as unknown };
    });
  const result = page.waitForResponse(
    async (response) => {
      const path = new URL(response.url()).pathname;
      if (response.request().method() !== "GET" || !/^\/account\/data-transfers\/[^/]+\/result$/.test(path)) {
        return false;
      }
      return path === `/account/data-transfers/${(await job).id}/result`;
    },
    { timeout: 90_000 },
  );
  const [download, response, { input }] = await Promise.all([
    page.waitForEvent("download", { timeout: 90_000 }),
    result,
    job,
    button.click(),
  ]);
  expect(response.ok(), await response.text()).toBe(true);
  const { data: file } = (await response.json()) as {
    data: { file_name: string; content_base64: string; metadata: Record<string, unknown> };
  };
  expect(download.suggestedFilename()).toBe(file.file_name);
  expect(await download.failure()).toBeNull();
  const path = await download.path();
  expect(path).toBeTruthy();
  const bytes = await readFile(path!);
  const content = Buffer.from(file.content_base64, "base64");
  const bom = Buffer.from("\uFEFF");
  expect(bytes).toEqual(kind === "profit_loss" ? Buffer.concat([bom, content]) : content);
  const csv = bytes.toString("utf8");
  expect(csv.startsWith("\uFEFF")).toBe(true);
  return { csv: csv.slice(1), fileName: file.file_name, input, metadata: file.metadata };
}

async function exportCsv(page: Page) {
  const result = await downloadCsvTransfer(
    page,
    workPage(page).locator("#actual-profit-loss-header-export"),
    "profit_loss",
  );
  expect(result.fileName).toMatch(/实际盈亏.*\.csv$/);
  return result;
}

/** 真实应收/应付导出必须包含当前单据、金额与范围版本。 */
async function expectFundsCsv(
  page: Page,
  kind: "customer" | "supplier",
  documentId: string,
  gross: string,
) {
  const customer = kind === "customer";
  const endpoint = customer ? "/admin/receivable-accounts" : "/admin/payable-accounts";
  const documentParameter = customer ? "salesOrderId" : "purchaseOrderId";
  const wireParameter = customer ? "sales_order_id" : "source_document_id";
  const initialResponse = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === endpoint &&
      new URL(response.url()).searchParams.get(wireParameter) === documentId,
  );
  await page.goto(
    `/finance/${kind}-accounts/scope?view=${customer ? "receivable" : "payable"}&${documentParameter}=${documentId}`,
  );
  const initial = await initialResponse;
  expect(initial.ok(), await initial.text()).toBe(true);
  const result = (await initial.json()).data as {
    scope_version: string;
    total: number;
    items: Array<{
      id: string;
      account_seq: number;
      gross_total: string;
      visible_settled_share: string;
      settled_total: string;
      open_total: string;
    }>;
  };
  expect(result.total).toBe(1);
  expect(result.items).toHaveLength(1);
  expect(result.items[0]!.gross_total).toBe(gross);
  const view = customer ? "receivable" : "payable";
  const exported = await downloadCsvTransfer(
    page,
    workPage(page).locator(`#${customer ? "customer-receivables" : "supplier-payables"}-scope-export`),
    customer ? "receivables" : "supplier_payables",
  );
  expect(exported.fileName).toBe(customer ? "客户应收.csv" : "供应商应付.csv");
  expect(exported.input).toMatchObject({ view, [wireParameter]: documentId });
  expect(exported.metadata).toMatchObject({ row_count: 1, scope_version: result.scope_version });
  const rows = exported.csv.split("\r\n");
  expect(rows).toHaveLength(2);
  expect(rows[0]).toContain(customer ? '"销售单"' : '"采购单"');
  expect(rows[1]).toContain(`"${documentId}"`);
  const account = result.items[0]!;
  expect(account.visible_settled_share).toMatch(/^0(?:\.0+)?$/);
  expect(account.settled_total).toMatch(/^0(?:\.0+)?$/);
  expect(account.open_total).toBe(gross);
  expect(rows[1]).toContain(
    `"${account.visible_settled_share}","${gross}","${account.settled_total}","${gross}"`,
  );
  expect(rows[1]).toContain(`"${result.scope_version}"`);
}

/** 双口径按正式含税收入统计；零税订单与盈亏净收入可精确对拍。 */
async function expectCurrentAndHistoryQuality(
  token: string,
  customerId: string,
  orders: [AnalyticsOrder, AnalyticsOrder],
  netRevenue: string,
) {
  type Quality = {
    ownershipBasis: string;
    period: { basis: string; timezone: string };
    totals: { objectCount: number; orderCount: number; grossTotal: string; unpricedCount: number };
    rows: {
      total: number;
      items: Array<{
        rowId: string;
        customerId?: string;
        ownerUserId?: string;
        attributionUserId?: string;
        orderCount: number;
        grossTotal: string;
        unpricedCount: number;
      }>;
    };
  };
  const query = {
    from: analyticsDate(),
    to: analyticsDate(),
    customer_id: customerId,
    page: 1,
    page_size: 20,
  };
  const [current, history] = await Promise.all([
    apiGet<Quality>(token, "/admin/customer-quality/current", { ...query, dimension: "customer" }),
    apiGet<Quality>(token, "/admin/customer-quality/history", {
      ...query,
      dimension: "attribution_user",
    }),
  ]);
  for (const view of [current, history]) {
    expect(view.period).toMatchObject({
      basis: "sales_order_effective_date",
      timezone: "Asia/Shanghai",
    });
    expect(view.totals).toEqual({
      objectCount: 1,
      orderCount: 2,
      grossTotal: netRevenue,
      unpricedCount: 0,
    });
    expect(view.rows.total).toBe(1);
  }
  expect(current.ownershipBasis).toBe("sales_order_current_responsibility");
  expect(history.ownershipBasis).toBe("sales_order_first_effective_attribution");
  expect(orders[0].owner_user_id).toBeTruthy();
  expect(orders[1].owner_user_id).toBe(orders[0].owner_user_id);
  expect(current.rows.items[0]).toMatchObject({
    customerId,
    ownerUserId: orders[0].owner_user_id,
    orderCount: 2,
    grossTotal: netRevenue,
  });
  expect(history.rows.items[0]).toMatchObject({
    attributionUserId: orders[0].owner_user_id,
    orderCount: 2,
    grossTotal: netRevenue,
  });
  for (const order of orders) {
    const contribution = await apiGet<Quality>(token, "/admin/customer-quality/history", {
      ...query,
      dimension: "attribution_user",
      attribution_group: history.rows.items[0]!.rowId,
      q: order.order_no,
    });
    expect(contribution.totals).toEqual({
      objectCount: 1,
      orderCount: 1,
      grossTotal: "200.00",
      unpricedCount: 0,
    });
    expect(contribution.rows.items[0]).toMatchObject({
      attributionUserId: order.owner_user_id,
      orderCount: 1,
      grossTotal: "200.00",
    });
  }
}

test("flow-23 成本归集与盈亏：未覆盖留空、阶段隔离、实际金额下钻和真实 CSV", async ({
  browser,
}) => {
  test.setTimeout(8 * 60 * 1000);
  const suffix = Date.now().toString(36).toUpperCase();
  const admin = await openLoggedInWorkspace(browser, "admin");
  const sales = await openLoggedInWorkspace(browser, "xiaoshou");
  const procurement = await openLoggedInWorkspace(browser, "caigou");
  const finance = await openLoggedInWorkspace(browser, "caiwu");
  try {
    await ensureDefaultProcurementOwner(admin.page);
    const adminToken = await apiToken("admin");
    const salesToken = await apiToken("xiaoshou");
    const procurementToken = await apiToken("caigou");
    const fixture = await prepareAnalyticsOrders(salesToken, adminToken, procurement.page, suffix);
    const [orderA, orderB] = fixture.orders;
    const before = await apiGet<Report>(
      adminToken,
      "/admin/actual-profit-loss",
      reportQuery(fixture.customerId),
    );
    expect(before.rows.total).toBe(2);
    expect(before.totals.netSalesRevenue).toBe("400.00");
    expect(before.totals).not.toHaveProperty("actualProfitLossNet");
    for (const row of before.rows.items) {
      expect(row.coverageState).toBe("UNCOVERED");
      expect(row).not.toHaveProperty("actualProfitLossNet");
      expect(row.coverageBlockers.map((item) => item.code)).toEqual(
        expect.arrayContaining(["SUPPLY_COST_MISSING", "FULFILLMENT_OPEN"]),
      );
    }
    const source = await completeAnalyticsService(
      orderA,
      sales.page,
      procurement.page,
      finance.page,
      procurementToken,
    );
    const posted = await apiGet<{ items: Cost[] }>(adminToken, "/admin/cost-entries", {
      source_document_id: source.purchaseId,
      cost_stage: "actual",
    });
    expect(posted.items).toHaveLength(1);
    const supplyCost = posted.items[0]!;
    expect(supplyCost).toMatchObject({
      cost_stage: "actual",
      cost_type: "product",
      source_fact_type: "purchase_fulfillment",
      source_document_id: source.purchaseId,
      net_amount: source.procurementNet,
    });
    expect(supplyCost.allocations).toEqual([
      expect.objectContaining({
        sales_order_id: orderA.id,
        sales_order_line_id: orderA.lines[0]!.id,
        allocated_net_amount: source.procurementNet,
      }),
    ]);
    const actualExpense = expense(
      orderA,
      fixture.evidenceId,
      `PL-${suffix}-actual`,
      "actual",
      "20.00",
    );
    const actual = await analyticsCommand<Cost>(adminToken, "/admin/cost-entries", actualExpense);
    expect(actual.allocations).toEqual([
      expect.objectContaining({ sales_order_id: orderA.id, allocated_net_amount: "20.00" }),
    ]);
    await analyticsCommand(
      adminToken,
      "/admin/cost-entries",
      expense(orderA, fixture.evidenceId, `PL-${suffix}-reduction`, "reduction", "5.00"),
    );
    for (const stage of ["expected", "confirmed"]) {
      await analyticsCommand(
        adminToken,
        "/admin/cost-entries",
        expense(orderA, fixture.evidenceId, `PL-${suffix}-${stage}`, stage, "1000.00"),
      );
    }
    await failedCommand(adminToken, "/admin/cost-entries", actualExpense, [409]);
    const unbalanced = expense(
      orderA,
      fixture.evidenceId,
      `PL-${suffix}-unbalanced`,
      "actual",
      "20.00",
    );
    unbalanced.allocations[0]!.allocated_net_amount = "19.99";
    await failedCommand(adminToken, "/admin/cost-entries", unbalanced, [400, 422]);
    const query = reportQuery(fixture.customerId);
    const expectedProfit = subtractMoney("185.00", source.procurementNet);
    const report = await uiReport(admin.page, fixture.customerId);
    expect(report.formulaVersion).toBe("non-voucher-net-v1");
    expect(report.coverage).toMatchObject({
      coveredNetRevenue: "200.00",
      uncoveredNetRevenue: "200.00",
      coverageRate: "50.00%",
    });
    const completed = report.rows.items.find((row) => row.objectId === orderA.id)!;
    expect(completed).toMatchObject({
      netSalesRevenue: "200.00",
      actualProcurementCostNet: source.procurementNet,
      actualFulfillmentCostNet: "20.00",
      reductionsNet: "5.00",
      actualProfitLossNet: expectedProfit,
      coverageState: "COVERED",
    });
    expect(completed.costEntryIds).toContain(actual.id);
    expect(report.rows.items.find((row) => row.objectId === orderB.id)).not.toHaveProperty(
      "actualProfitLossNet",
    );
    expect(report.totals).toMatchObject({
      netSalesRevenue: "400.00",
      actualProcurementCostNet: source.procurementNet,
      actualFulfillmentCostNet: "20.00",
      reductionsNet: "5.00",
      actualProfitLossNet: expectedProfit,
    });
    await expectCurrentAndHistoryQuality(
      adminToken,
      fixture.customerId,
      fixture.orders,
      report.totals.netSalesRevenue,
    );
    const analytics = workPage(admin.page);
    const profitCell = (order: AnalyticsOrder) => analytics.getByRole("row")
      .filter({ hasText: order.order_no })
      .locator('[data-column-id="actualProfitLossNet"]');
    await expect(profitCell(orderA)).toContainText(expectedProfit);
    await expect(profitCell(orderB)).toHaveText("待归集成本");
    await analytics.locator(`#actual-profit-loss-row-${orderA.id}-fulfillment-cost`).click();
    const sheet = analytics.getByRole("dialog", { name: "成本记录", exact: true });
    await expect(sheet).toBeVisible(VISIBLE);
    await sheet.locator(`#actual-profit-loss-cost-entry-${actual.id}`).click();
    await expect(sheet).toContainText("配送");
    await expect(sheet).toContainText("20.00");
    await expect(sheet).toContainText(actualExpense.source_document_id);
    await sheet.locator("#actual-profit-loss-cost-detail-dismiss").click();
    const exported = await exportCsv(admin.page);
    expect(exported.metadata.rowCount).toBe(2);
    expect(exported.csv).toContain(`"${orderA.order_no}"`);
    expect(exported.csv).toContain(`"${orderB.order_no}"`);
    expect(exported.csv).toContain(
      `"200.00","${source.procurementNet}","20.00","5.00","${expectedProfit}"`,
    );
    expect(exported.input).toMatchObject({
      customer_id: fixture.customerId,
      scope_version: report.scopeVersion,
    });
    const filtered = await uiReport(admin.page, fixture.customerId, {
      costType: "delivery",
      q: orderA.order_no,
    });
    expect(filtered.rows.total).toBe(1);
    expect(filtered.rows.items[0]).toMatchObject({
      actualProcurementCostNet: source.procurementNet,
      actualFulfillmentCostNet: "20.00",
      actualProfitLossNet: expectedProfit,
    });
    const filteredExport = await exportCsv(admin.page);
    expect(filteredExport.metadata.rowCount).toBe(1);
    expect(filteredExport.input).toMatchObject({
      customer_id: fixture.customerId,
      scope_version: filtered.scopeVersion,
      cost_types: "delivery",
      q: orderA.order_no,
    });
    expect(filteredExport.csv).toContain(orderA.order_no);
    expect(filteredExport.csv).not.toContain(orderB.order_no);
    await expectFundsCsv(admin.page, "customer", orderA.id, "200.00");
    await expectFundsCsv(admin.page, "supplier", source.purchaseId, source.procurementGross);
    const stale = await failedCommand(
      adminToken,
      "/admin/actual-profit-loss/exports",
      { ...query, scope_version: "forged-version" },
      [409],
    );
    expect(stale.code).toBe("DATA_SCOPE_CHANGED");
    await failedCommand(
      adminToken,
      "/admin/actual-profit-loss/exports",
      { ...query, period_basis: "cost_occurred_date" },
      [400, 422],
    );
    await failedCommand(
      salesToken,
      "/admin/cost-entries",
      expense(orderB, fixture.evidenceId, `PL-${suffix}-denied`, "actual", "10.00"),
      [403],
    );
    const queryString = new URLSearchParams(
      Object.entries(query).map(([key, value]) => [key, String(value)]),
    );
    for (const endpoint of [
      `/admin/cost-entries/${actual.id}`,
      `/admin/actual-profit-loss?${queryString}`,
    ]) {
      const denied = await admin.page.request.get(`${API_BASE}${endpoint}`, {
        headers: { Authorization: `Bearer ${salesToken}` },
      });
      expect(denied.status(), await denied.text()).toBe(403);
    }
  } finally {
    await Promise.all([
      admin.context.close(),
      sales.context.close(),
      procurement.context.close(),
      finance.context.close(),
    ]);
  }
});
